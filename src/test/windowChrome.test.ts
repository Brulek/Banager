import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import tauriConfig from "../../src-tauri/tauri.conf.json";
import capability from "../../src-tauri/capabilities/default.json";
import packageJson from "../../package.json";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const [mainWindow] = tauriConfig.app.windows;

describe("the window", () => {
  it("draws its title bar as an overlay with no title, over the page", () => {
    expect(mainWindow.titleBarStyle).toBe("Overlay");
    expect(mainWindow.hiddenTitle).toBe(true);
    // No private macOS API: what it would buy here is a transparent window.
    expect("macOSPrivateApi" in tauriConfig.app).toBe(false);
  });

  it("puts the traffic lights 19px in from the left and from the top, their centre 26px down", () => {
    // tao, the window library under Tauri, puts the close button's left
    // edge at `x`, and makes the title bar `y` taller than a light, the
    // lights staying as high above its bottom edge as macOS had them. On
    // macOS 27 -- measured in an AppKit window set up as tao sets up this
    // one -- a light is a 14px circle filling its button, 9px above that
    // edge, so its top is at `y - 9`. 19px in from the left and from the
    // top is where macOS 27 draws them in a window with a toolbar. Other
    // versions size and place the buttons differently, so there they can
    // sit a little off this line.
    const LIGHT = 14;
    const { x, y } = mainWindow.trafficLightPosition;
    const top = y - 9;
    expect(x).toBe(19);
    expect(top).toBe(19);
    // The line the Sidebar's first row (52px, the lights with 19px above
    // and below them) and the page header's row (32px, 10px down) centre on.
    expect(top + LIGHT / 2).toBe(26);
  });

  it("opens at 960 by 640, and goes no smaller than 800 by 560", () => {
    // 960 wide: an Updates row shows its source's name beside the tool's
    // from a window 928px wide (ToolRow's `@2xl`: 672px of row, with the
    // 208px sidebar, and 12px of the list's and 12px of the row's own
    // padding on each side, around it); narrower, down to 800, the
    // source's name gives way to the tool's. 640 tall: 40px more of the
    // list than 600 gives, in the 3:2 of 960 by 640, and the window still
    // fits a 13-inch MacBook's screen with room around it.
    expect([mainWindow.width, mainWindow.height]).toEqual([960, 640]);
    expect([mainWindow.minWidth, mainWindow.minHeight]).toEqual([800, 560]);
  });

  it("lets the page's drag regions move the window", () => {
    // Tauri's drag script asks for `plugin:window|start_dragging`, which
    // `core:default` does not allow; the zoom on a double-click, which it
    // asks for as `internal_toggle_maximize`, it does.
    expect(capability.permissions).toContain("core:default");
    expect(capability.permissions).toContain("core:window:allow-start-dragging");
  });

  it("gives the page none of the window-state plugin's commands", () => {
    // Rust restores the window's size and position and saves them
    // (`run()` in src-tauri/src/lib.rs); the page has no part in it.
    expect(capability.permissions.filter((p) => p.startsWith("window-state:"))).toEqual([]);
  });
});

describe("pnpm tauri:mock", () => {
  const MOCK_CONFIG = "src-tauri/tauri.mock.conf.json5";
  // Plain JSON apart from its whole-line `//` comments, as the file says.
  const mock = JSON.parse(readFileSync(path.join(ROOT, MOCK_CONFIG), "utf-8").replace(/^\s*\/\/.*$/gm, "")) as {
    identifier: string;
    build: { beforeDevCommand: string; devUrl: string };
  };
  const previewPort = /const MOCK_PORT = (\d+);/.exec(readFileSync(path.join(ROOT, "vite.config.ts"), "utf-8"))?.[1];

  it("is tauri dev with the mock's config merged over the app's", () => {
    expect(packageJson.scripts["tauri:mock"]).toBe(`tauri dev --config ${MOCK_CONFIG}`);
  });

  it("loads Vite in mock mode on a port of its own, never the real front end", () => {
    const port = new URL(mock.build.devUrl).port;
    expect(mock.build.beforeDevCommand).toBe(`pnpm exec vite --mode mock --port ${port} --strictPort`);
    // Not `pnpm dev`'s, which serves the real front end, and not the
    // browser preview's, which may be open beside it.
    expect(mock.build.devUrl).not.toBe(tauriConfig.build.devUrl);
    expect(previewPort).toBe("1430");
    expect(port).not.toBe(previewPort);
  });

  it("has an identifier of its own, so a folder of its own", () => {
    expect(mock.identifier).not.toBe(tauriConfig.identifier);
  });
});
