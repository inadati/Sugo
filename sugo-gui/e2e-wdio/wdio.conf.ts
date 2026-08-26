import path from "node:path";
import fs from "node:fs";
import os from "node:os";
import { fileURLToPath } from "node:url";

// sugo-gui's package.json sets `"type": "module"`, so this config file has no
// CommonJS `__dirname`; derive it from `import.meta.url` instead.
const __dirname = path.dirname(fileURLToPath(import.meta.url));

/**
 * WebdriverIO config for the "real Tauri app + real SQLite file" E2E suite.
 *
 * Why this exists (see design.md L157 and the eval-axes reject on
 * e2e-verification-authenticity): sugo-gui/e2e/folders.spec.ts drives the
 * frontend against a stub `window.__TAURI_INTERNALS__.invoke` — useful for
 * exercising Vue component behaviour cheaply, but it never touches
 * src-tauri/src/commands.rs, sugo-infra's SQLite repository, or an actual
 * Tauri IPC boundary. This suite closes that gap: it launches the *compiled*
 * Tauri binary (built with `--features webdriver`, see
 * sugo-gui/src-tauri/Cargo.toml) against an isolated real SQLite file via the
 * `SUGO_DB` env var, drives it through `@wdio/tauri-service`'s embedded
 * WebDriver provider (tauri-plugin-wdio-webdriver, native WKWebView APIs on
 * macOS — no `tauri-driver` needed; `tauri-driver` itself does not support
 * macOS, see docs/e2e-webdriver.md), and asserts against the DB file on disk
 * afterwards.
 */

// A fresh, isolated DB file per run so this suite never touches the
// developer's real ~/.sugo/sugo.db. Exported so the spec file can read the
// same path back to assert against the real on-disk SQLite data.
//
// IMPORTANT: WDIO evaluates this config module twice per run — once in the
// launcher process (which uses `SUGO_DB` to spawn the app via
// @wdio/tauri-service's embedded provider) and again in the forked worker
// process that actually runs the spec. If this file unconditionally called
// `mkdtempSync` every time it's imported, the worker would compute a
// *different* random directory than the one the launcher already spawned
// the app against, and every DB assertion in the spec would silently query
// an empty file the app never touched (sqlite3 creates a 0-byte placeholder
// when opening a nonexistent path) instead of failing loudly. Only mint a
// new path when `SUGO_DB` isn't already inherited from the parent process,
// so the launcher and every worker agree on the same file.
function resolveE2eDbPath(): string {
  if (process.env.SUGO_DB) return process.env.SUGO_DB;
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "sugo-wdio-e2e-"));
  const dbPath = path.join(dir, "sugo.db");
  process.env.SUGO_DB = dbPath;
  return dbPath;
}
export const SUGO_E2E_DB_PATH = resolveE2eDbPath();

const appBinaryPath = path.resolve(
  __dirname,
  "../../target/debug/sugo-gui",
);

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: ["./specs/**/*.e2e.ts"],
  maxInstances: 1,
  maxInstancesPerCapability: 1,

  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath,
        driverProvider: "embedded",
        captureBackendLogs: true,
        captureFrontendLogs: true,
        startTimeout: 60000,
      },
    ],
  ],

  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application: appBinaryPath,
      },
    },
  ],

  logLevel: "info",
  bail: 0,
  waitforTimeout: 10000,
  connectionRetryTimeout: 90000,
  connectionRetryCount: 3,

  framework: "mocha",
  mochaOpts: {
    ui: "bdd",
    timeout: 60000,
  },

  reporters: ["spec"],
};
