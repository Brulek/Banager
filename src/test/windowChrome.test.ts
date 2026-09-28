import { describe, expect, it } from "vitest";
import tauriConfig from "../../src-tauri/tauri.conf.json";
import capability from "../../src-tauri/capabilities/default.json";

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
