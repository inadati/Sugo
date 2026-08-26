import { test, expect } from "@playwright/test";

/**
 * Tauri の `invoke` はブラウザ単体では動作しない（`window.__TAURI_INTERNALS__`
 * が存在しないため）。E2E ではこのブリッジを実際の Tauri コマンドと同じ
 * インターフェースでスタブし、Vue 側のコード（AppSidebar / HarnessList /
 * FolderNameDialog）はビルド済みの本物をそのまま動かす。
 *
 * スタブの状態遷移（create_folder / move_harness_to_folder 等）は
 * sugo-gui/src-tauri のコマンド層と同じ意味論を再現しており、SQLite の
 * 永続化・トランザクション・検証ロジック自体は cargo test --workspace
 * （sugo-core / sugo-infra の実 SQLite テスト）で別途検証済みである。
 */
function installTauriStub() {
  interface Harness {
    harness_id: string;
    name: string;
    current_version: number;
    has_draft: boolean;
    folder_id: string | null;
    folder_name: string | null;
  }
  interface Folder {
    folder_id: string;
    name: string;
  }

  const harnesses: Harness[] = [
    {
      harness_id: "h1",
      name: "alpha",
      current_version: 1,
      has_draft: false,
      folder_id: null,
      folder_name: null,
    },
    {
      harness_id: "h2",
      name: "beta",
      current_version: 1,
      has_draft: false,
      folder_id: null,
      folder_name: null,
    },
  ];
  const folders: Folder[] = [];
  let nextFolderId = 1;

  function folderCount(folderId: string): number {
    return harnesses.filter((h) => h.folder_id === folderId).length;
  }

  const w = window as unknown as {
    __TAURI_INTERNALS__?: { invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown> };
  };
  w.__TAURI_INTERNALS__ = {
    invoke: async (cmd: string, args: Record<string, unknown> = {}) => {
      switch (cmd) {
        case "list_harnesses":
          return harnesses.map((h) => ({ ...h }));
        case "list_folders":
          return folders.map((f) => ({ ...f, harness_count: folderCount(f.folder_id) }));
        case "list_trash":
          return [];
        case "create_folder": {
          const name = String(args.name);
          if (folders.some((f) => f.name === name)) {
            throw `フォルダ「${name}」は既に存在します`;
          }
          const folder = { folder_id: `f${nextFolderId++}`, name };
          folders.push(folder);
          return { ...folder, harness_count: 0 };
        }
        case "rename_folder": {
          const f = folders.find((f) => f.folder_id === args.folderId);
          if (f) f.name = String(args.name);
          return null;
        }
        case "delete_folder": {
          const idx = folders.findIndex((f) => f.folder_id === args.folderId);
          const removed = folders[idx];
          folders.splice(idx, 1);
          let moved = 0;
          for (const h of harnesses) {
            if (h.folder_id === args.folderId) {
              h.folder_id = null;
              h.folder_name = null;
              moved++;
            }
          }
          return { name: removed?.name ?? "", moved_to_uncategorized: moved };
        }
        case "move_harness_to_folder": {
          const h = harnesses.find((h) => h.harness_id === args.harnessId);
          if (h) {
            const folderId = (args.folderId as string | null) ?? null;
            h.folder_id = folderId;
            h.folder_name = folders.find((f) => f.folder_id === folderId)?.name ?? null;
          }
          return null;
        }
        case "create_harness": {
          const id = `h${harnesses.length + 1}`;
          harnesses.push({
            harness_id: id,
            name: String(args.name),
            current_version: 1,
            has_draft: false,
            folder_id: null,
            folder_name: null,
          });
          return { harness_id: id };
        }
        case "trash_harness":
          return null;
        default:
          return null;
      }
    },
  };
}

test("フォルダを作ってハーネスをドラッグで移動できる", async ({ page }) => {
  await page.addInitScript(installTauriStub);
  await page.goto("/");

  await expect(page.getByText("alpha")).toBeVisible();
  await expect(page.getByText("beta")).toBeVisible();

  await page.getByTestId("new-folder-btn").click();
  await page.getByTestId("folder-name").fill("E2E確認");
  await page.getByTestId("folder-submit").click();
  await expect(page.getByText("E2E確認")).toBeVisible();

  const row = page.locator('[data-testid="harness-row"]').filter({ hasText: "alpha" });
  const target = page.locator('[data-testid^="folder-drop-"]').filter({ hasText: "E2E確認" });
  await row.dragTo(target);

  // ドロップ後、サイドバーのフォルダ件数バッジが 1 に更新される。
  await expect(target).toContainText("1");

  await target.click();
  await expect(page.locator('[data-testid="harness-row"]')).toHaveCount(1);
  await expect(page.locator('[data-testid="harness-row"]')).toContainText("alpha");

  await page.screenshot({ path: "e2e-folders-after-drag.png" });
});
