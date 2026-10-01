/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";
const host = process.env.TAURI_DEV_HOST;

// The app's version, as Settings' About shows it (`__APP_VERSION__`,
// declared in src/vite-env.d.ts): the one in tauri.conf.json, which is
// the version Tauri gives the bundle. Set into the page when it is built
// rather than asked of Tauri at run time, so the browser preview, which
// has no Tauri, shows it too.
const APP_VERSION: string = JSON.parse(
  readFileSync(new URL("./src-tauri/tauri.conf.json", import.meta.url), "utf-8"),
).version;

// `vite --mode mock` (`pnpm dev:mock`): the UI in a plain browser with a
// mock backend and no Tauri, for screenshots (docs/ui-preview.md). Only
// that mode aliases "@tauri-apps/api/core" to src/dev/mockTauri.ts,
// "@tauri-apps/api/event" to src/dev/mockTauriEvent.ts,
// "@tauri-apps/api/window" to src/dev/mockTauriWindow.ts and
// "@tauri-apps/plugin-opener" to src/dev/mockTauriOpener.ts, and serves on
// its own port; every other mode -- `pnpm dev` under `pnpm tauri dev`,
// `pnpm build` under `pnpm tauri build`, vitest's `test` -- resolves
// exactly the config it did before the mode existed.
const MOCK_MODE = "mock";
const MOCK_PORT = 1430;

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  const mock = mode === MOCK_MODE;
  return {
    plugins: [react(), tailwindcss()],

    define: {
      __APP_VERSION__: JSON.stringify(APP_VERSION),
    },

    ...(mock
      ? {
          resolve: {
            alias: [
              {
                find: /^@tauri-apps\/api\/core$/,
                replacement: fileURLToPath(new URL("./src/dev/mockTauri.ts", import.meta.url)),
              },
              {
                find: /^@tauri-apps\/api\/event$/,
                replacement: fileURLToPath(new URL("./src/dev/mockTauriEvent.ts", import.meta.url)),
              },
              {
                find: /^@tauri-apps\/api\/window$/,
                replacement: fileURLToPath(new URL("./src/dev/mockTauriWindow.ts", import.meta.url)),
              },
              {
                find: /^@tauri-apps\/plugin-opener$/,
                replacement: fileURLToPath(new URL("./src/dev/mockTauriOpener.ts", import.meta.url)),
              },
            ],
          },
        }
      : {}),

    // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
    //
    // 1. prevent Vite from obscuring rust errors
    clearScreen: false,
    // 2. tauri expects a fixed port, fail if that port is not available
    server: {
      port: mock ? MOCK_PORT : 1420,
      strictPort: true,
      // The preview is a page in this Mac's browser, never a Tauri mobile
      // target: TAURI_DEV_HOST stays `tauri dev`'s, and its HMR port 1421
      // stays free for it.
      host: mock ? false : host || false,
      hmr:
        host && !mock
          ? {
              protocol: "ws",
              host,
              port: 1421,
            }
          : undefined,
      watch: {
        // 3. tell Vite to ignore watching `src-tauri`
        ignored: ["**/src-tauri/**"],
      },
    },
    test: {
      environment: "jsdom",
      setupFiles: ["src/test/setup.ts"],
      css: false,
      // Only the project's own test files: none from a build or output
      // folder, where a vendor package unpacked by hand would otherwise be
      // collected and its tests run (2026-10-02). The first two are
      // vitest's defaults, which setting `exclude` replaces.
      exclude: [
        "**/node_modules/**",
        "**/.git/**",
        "target/**",
        "src-tauri/target/**",
        "dist/**",
        "dist-ssr/**",
        ".superpowers/**",
      ],
    },
  };
});
