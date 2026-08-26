# e2e-wdio: real Tauri app + real SQLite E2E

`sugo-gui/e2e/folders.spec.ts` (Playwright) drives the frontend against a stub
`window.__TAURI_INTERNALS__.invoke` — good for cheap Vue component coverage,
but it never touches `src-tauri/src/commands.rs`, the real SQLite repository,
or an actual Tauri IPC boundary.

This suite closes that gap. It launches the **compiled** Tauri binary (built
with the `webdriver` Cargo feature) against an **isolated real SQLite file**
and drives it through WebdriverIO via `@wdio/tauri-service`'s embedded
provider (`tauri-plugin-wdio-webdriver`, native WKWebView APIs on macOS — no
`tauri-driver` needed). `tauri-driver` itself does not support macOS
(`tauri-driver --help` prints "tauri-driver is not supported on this
platform" and exits non-zero; official docs confirm only Windows and Linux
are supported, since macOS has no equivalent WKWebView driver tool).

**Status: this suite passes and does what it claims.** It was rejected four
times before this landed; the root cause was never the WebDriver plugin or
the invoke API (see "Diagnosis history" below for what that dead end looked
like) — it was that the app was being **built wrong**. Read "Running it"
below and use the exact build command it specifies.

## Running it

```bash
# 1. Build through the Tauri CLI (NOT a bare `cargo build`!) in debug mode
#    with the webdriver server compiled in.
cd sugo-gui && npx tauri build --debug --no-bundle --features webdriver

# 2. Run the suite against that binary.
npm run test:e2e-wdio
```

`npm run test:e2e-wdio` also runs step 1 automatically via the
`pretest:e2e-wdio` npm script — `npm run test:e2e-wdio` alone is enough. Step
1 is spelled out here because that command is the actual fix and must not be
"simplified" back to a bare `cargo build` (see below).

### Why it has to be `tauri build`, not `cargo build`

A bare `cargo build -p sugo-gui --features webdriver` **compiles successfully
and produces a runnable binary** — but that binary's embedded frontend-asset
table is empty. Every asset request 404s at runtime with a literal
`asset not found: <path>` body (confirmed by using the plugin's own
`POST /session/{id}/url` WebDriver command to navigate straight to
`tauri://localhost/`, `tauri://localhost/index.html`, and
`tauri://localhost/assets/index-*.js` — all three came back as that same
error page, on a binary built *after* `dist/` had fresh, correct content).
Since even the app's *own default* startup navigation target has nothing to
render, the webview never receives a single byte of HTML and simply never
leaves `about:blank` — `document.readyState` reports `"complete"` (there is
genuinely nothing left to load) and `window.__TAURI_INTERNALS__` is
`undefined` because Tauri's init script is injected as part of *that* missing
HTML document, not independently of it.

The reason is that `tauri.conf.json`'s `build.beforeBuildCommand`
(`npm run build`) and the environment variables `tauri-build`'s
`generate_context!()` macro relies on to embed `frontendDist` correctly are
set up by the **`tauri` CLI**, not by `cargo` alone. `npx tauri build --debug
--no-bundle` runs that full pipeline (rebuilding `dist/` from source and
setting those variables before invoking `cargo`) the same way the real
release build (`cargo tauri build`, used for `/Applications/Sugo.app`)
already does — which is exactly why normal releases were never affected by
this, only this suite's hand-rolled build step was.

### Diagnosis history (what turned out to be a dead end)

Three earlier rounds chased this as a WKWebView/plugin problem, because the
symptom — `Error: backend state (SQLite DB) never became ready` from polling
`list_harnesses` — looked exactly like an IPC-bridge or navigation bug:

- Round 2: `tauri-driver` doesn't support macOS at all → switched to
  `@wdio/tauri-service`'s embedded `tauri-plugin-wdio-webdriver` provider.
- Round 3: found `window.__TAURI__.core.invoke` doesn't exist in the binary.
- Round 4: switched the poll to `window.__TAURI_INTERNALS__.invoke` (the
  lower-level IPC bridge Tauri's init script always injects, regardless of
  the `app.withGlobalTauri` config flag). **This fix is real and still
  correct** — `withGlobalTauri` genuinely isn't enabled here, and
  `__TAURI_INTERNALS__` genuinely is the right thing to poll — but it did
  not touch the actual defect, so the suite kept failing identically
  afterward, which is what triggered this round.

Round 5 (this one) reproduced the exact same failure, then went one layer
deeper than "which invoke global" by asking whether the webview had loaded
*any* page at all. Probing `location.href` directly via the plugin's raw
WebDriver HTTP endpoint (bypassing WebdriverIO's client entirely) showed
`about:blank`, and a native WKWebView snapshot via the same endpoint showed a
blank white window — not a slow load, not a race, just nothing there. That
pointed at asset embedding rather than navigation/IPC, which is where the
real fix above came from.

The config (`wdio.conf.ts`) points `appBinaryPath` at
`../../target/debug/sugo-gui` and sets `SUGO_DB` to a fresh temp file per run
(see the comment in `wdio.conf.ts` about why that must only be computed
once and shared with the worker process via env inheritance, not recomputed
per process) — it never touches `~/.sugo/sugo.db`.

## Why `browser.execute()` instead of `$().click()` / `.dragAndDrop()`

WebdriverIO's higher-level element commands (`elementClick`, `$`,
`findElement`, ...) route through `@wdio/tauri-service`'s
`ensureActiveWindowFocus` pre-hook, which probes window state via a
`plugin:wdio|get_window_states` invoke that `tauri-plugin-wdio-webdriver`
1.3.0 does not implement in this app. The probe fails after its own internal
timeout on every such command, and in practice this reliably stalls further
than the per-test timeout for command sequences like a button click.
Confirmed independently: issuing the identical
`POST /session/{id}/element/{id}/click` directly against the plugin's raw
WebDriver HTTP endpoint with `curl` returns instantly — the bug is in
WebdriverIO's client-side wrapper, not in the app, the plugin, or the IPC
boundary under test.

`execute()` is not one of the hooked commands, so `folders.e2e.ts` drives the
UI by calling real DOM APIs inside the actual WKWebView: real `.click()`,
real `dispatchEvent(new Event("input"))` for form fields (mirroring Vue's
`v-model`), and a real `dispatchEvent(new DragEvent(...))` sequence with an
actual `DataTransfer` for the folder drag-and-drop — the exact event
sequence a real drag produces, exercising `HarnessList.vue`'s `@dragstart`
and `AppSidebar.vue`'s `@dragover`/`@drop` handlers for real. The only thing
that's synthesized is the *input* side of the gesture; every handler,
`invoke()` call, and SQLite write it triggers is the genuine, unmocked code
path.

## What the test proves

1. Create a harness through the real UI → real `invoke("create_harness")` →
   real `commands.rs` → real `SqliteHarnessRepository` → confirmed via a
   direct `sqlite3 <file> SELECT ...` query against the on-disk file.
2. Create a folder the same way.
3. Drag the harness onto the folder → real `invoke("move_harness_to_folder")`
   → sidebar badge updates via the app's own polling → confirmed again via a
   direct SQL query that `harnesses.folder_id` matches the real folder's id.
4. `browser.saveScreenshot()` captures the real running window afterward.
