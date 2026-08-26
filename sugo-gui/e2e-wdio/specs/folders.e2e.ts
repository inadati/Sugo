import { execFileSync } from "node:child_process";
import { SUGO_E2E_DB_PATH } from "../wdio.conf.ts";

/**
 * Real Tauri app + real SQLite file E2E test for the folders feature.
 *
 * Unlike sugo-gui/e2e/folders.spec.ts (which stubs the Tauri invoke bridge
 * entirely), this test drives the actual compiled app: every `invoke()` call
 * made by AppSidebar.vue / HarnessList.vue crosses the real Tauri IPC
 * boundary into src-tauri/src/commands.rs, which calls the real
 * sugo-infra SQLite repository against `SUGO_E2E_DB_PATH` (an isolated file,
 * never the developer's ~/.sugo/sugo.db). After driving the UI we read that
 * file directly with the `sqlite3` CLI to prove the write actually landed on
 * disk, independent of whatever the UI claims.
 *
 * Interactions are issued via `browser.execute()` (real DOM APIs — real
 * `.click()`, real `dispatchEvent(new DragEvent(...))` with a real
 * `DataTransfer`, running inside the actual WKWebView) rather than
 * WebdriverIO's `elementClick`/`$().click()` helpers. This is a deliberate,
 * documented trade-off, not a shortcut around "real": those helpers route
 * through `@wdio/tauri-service`'s `ensureActiveWindowFocus` pre-hook (see
 * node_modules/@wdio/tauri-service/dist/esm/index.js:3024), whose window
 * state probe (`plugin:wdio|get_window_states`) is not implemented by
 * tauri-plugin-wdio-webdriver 1.3.0 in this app and reliably stalls WDIO's
 * own client past the mocha timeout — confirmed by issuing the identical
 * `POST /session/{id}/element/{id}/click` against the plugin's raw WebDriver
 * HTTP endpoint directly with curl, which returns instantly. The bug is in
 * WebdriverIO's client-side wrapper, not in the app, the plugin, or the IPC
 * boundary under test. `execute()` is not one of the hooked commands, so it
 * reaches the real WKWebView immediately and exercises the exact same Vue
 * event handlers a real click or drag would.
 */

/** Runs a read-only query against the real E2E SQLite file and returns stdout. */
function queryDb(sql: string): string {
  return execFileSync("sqlite3", [SUGO_E2E_DB_PATH, sql], {
    encoding: "utf-8",
  }).trim();
}

/** Polls `check` (evaluated in-page) until it returns true or `timeoutMs` elapses. */
async function waitFor(check: () => boolean, timeoutMs = 10000) {
  await browser.waitUntil(() => browser.execute(check), {
    timeout: timeoutMs,
    interval: 200,
  });
}

describe("harness folders (real Tauri app + real SQLite file)", () => {
  it("creates a harness, creates a folder, and moves the harness into it via drag-and-drop", async () => {
    // The embedded wdio-webdriver server (registered via `.plugin()`) can
    // start accepting WebDriver connections before this app's own `.setup()`
    // hook (which opens SUGO_E2E_DB_PATH and creates the schema, see
    // src-tauri/src/lib.rs) has finished — Tauri plugin init and the app's
    // setup closure are not ordered relative to each other. Poll the real
    // `list_harnesses` invoke until the backend state is actually up rather
    // than assuming the window being interactive means the DB is ready.
    await browser.waitUntil(
      async () => {
        const ok = await browser.execute(async () => {
          try {
            await (window as unknown as { __TAURI__: { core: { invoke: (c: string) => Promise<unknown> } } }).__TAURI__.core.invoke(
              "list_harnesses",
            );
            return true;
          } catch {
            return false;
          }
        });
        return ok;
      },
      { timeout: 10000, interval: 100, timeoutMsg: "backend state (SQLite DB) never became ready" },
    );

    // --- Arrange: create one real harness through the real UI/invoke/DB path ---
    await browser.execute(() => {
      (document.querySelector('[data-testid="create-harness-btn"]') as HTMLElement).click();
    });
    await waitFor(() => document.querySelector('[data-testid="name"]') !== null);

    await browser.execute((value: string) => {
      const input = document.querySelector('[data-testid="name"]') as HTMLInputElement;
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value")!.set!;
      setter.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }, "wdio-e2e-alpha");

    await browser.execute(() => {
      (document.querySelector('[data-testid="submit"]') as HTMLElement).click();
    });
    // NewHarnessDialog's `@created` handler (HarnessList.vue:188) navigates
    // straight to the new harness's board view (`/harness/:id`, a top-level
    // route in router/index.ts that is NOT nested under ShellLayout, so the
    // sidebar isn't even rendered there). Use the board view's own "← 一覧"
    // button (HarnessView.vue) to get back to the sidebar + list route.
    await waitFor(() =>
      Array.from(document.querySelectorAll("button")).some((b) => b.textContent?.includes("一覧")),
    );
    await browser.execute(() => {
      const backBtn = Array.from(document.querySelectorAll("button")).find((b) =>
        b.textContent?.includes("一覧"),
      ) as HTMLElement;
      backBtn.click();
    });
    await waitFor(() =>
      Array.from(document.querySelectorAll('[data-testid="harness-row"]')).some((el) =>
        el.textContent?.includes("wdio-e2e-alpha"),
      ),
    );

    // Confirm the harness actually exists in the real DB file before we move it.
    const harnessCountBefore = queryDb(
      "SELECT COUNT(*) FROM harnesses WHERE name = 'wdio-e2e-alpha'",
    );
    expect(harnessCountBefore).toBe("1");

    // --- Arrange: create a real folder through the real UI/invoke/DB path ---
    await browser.execute(() => {
      (document.querySelector('[data-testid="new-folder-btn"]') as HTMLElement).click();
    });
    await waitFor(() => document.querySelector('[data-testid="folder-name"]') !== null);

    await browser.execute((value: string) => {
      const input = document.querySelector('[data-testid="folder-name"]') as HTMLInputElement;
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value")!.set!;
      setter.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }, "E2E確認フォルダ");

    await browser.execute(() => {
      (document.querySelector('[data-testid="folder-submit"]') as HTMLElement).click();
    });
    await waitFor(() =>
      Array.from(document.querySelectorAll('[data-testid^="folder-drop-"]')).some((el) =>
        el.textContent?.includes("E2E確認フォルダ"),
      ),
    );

    const folderCountBefore = queryDb(
      "SELECT COUNT(*) FROM folders WHERE name = 'E2E確認フォルダ'",
    );
    expect(folderCountBefore).toBe("1");

    // --- Act: real HTML5 drag-and-drop of the harness row onto the folder in
    // the sidebar. HarnessList.vue's row has `draggable="true"` +
    // `@dragstart` (calls `dataTransfer.setData`); AppSidebar.vue's folder
    // link has `@dragover.prevent` + `@drop.prevent` (calls
    // `dataTransfer.getData` then the real `invoke("move_harness_to_folder")`).
    // We dispatch the exact same event sequence a real drag produces,
    // constructing a real `DataTransfer` so `getData`/`setData` round-trip
    // for real, rather than mocking the Vue handlers or the invoke bridge.
    await browser.execute(() => {
      const row = Array.from(document.querySelectorAll('[data-testid="harness-row"]')).find(
        (el) => el.textContent?.includes("wdio-e2e-alpha"),
      ) as HTMLElement;
      const target = Array.from(document.querySelectorAll('[data-testid^="folder-drop-"]')).find(
        (el) => el.textContent?.includes("E2E確認フォルダ"),
      ) as HTMLElement;

      const dt = new DataTransfer();
      row.dispatchEvent(new DragEvent("dragstart", { bubbles: true, cancelable: true, dataTransfer: dt }));
      target.dispatchEvent(new DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: dt }));
      target.dispatchEvent(new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: dt }));
    });

    // --- Assert: sidebar badge reflects the move (polled via the app's own 2s refresh) ---
    await waitFor(() => {
      const target = Array.from(document.querySelectorAll('[data-testid^="folder-drop-"]')).find(
        (el) => el.textContent?.includes("E2E確認フォルダ"),
      );
      return !!target && /\b1\b/.test(target.textContent ?? "");
    }, 15000);

    await browser.saveScreenshot("./e2e-wdio/wdio-folders-after-drag.png");

    // --- Assert: the move is persisted in the real SQLite file, not just in UI state ---
    const persistedFolderId = queryDb(
      "SELECT h.folder_id FROM harnesses h WHERE h.name = 'wdio-e2e-alpha'",
    );
    const folderRow = queryDb("SELECT id FROM folders WHERE name = 'E2E確認フォルダ'");
    expect(persistedFolderId).toBe(folderRow);
    expect(persistedFolderId).not.toBe("");
  });
});
