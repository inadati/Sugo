import { defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  test: {
    environment: "happy-dom",
    globals: true,
    // e2e/ は @playwright/test 用の仕様ファイル。vitest の対象から除外する。
    exclude: ["**/node_modules/**", "e2e/**"],
  },
});
