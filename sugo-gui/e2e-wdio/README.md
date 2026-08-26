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

## Running it

```bash
# 1. Build the app with the webdriver server compiled in, and the JS-global
#    Tauri API exposed (only @wdio/tauri-service's internal diagnostics need
#    window.__TAURI__; the app's own code always uses @tauri-apps/api).
npx tauri build --debug --no-bundle --features webdriver \
  --config '{"app":{"withGlobalTauri":true}}'

# 2. Run the suite against that binary.
npm run test:e2e-wdio
```

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
