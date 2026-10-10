import { beforeAll, describe, expect, it, vi } from "vitest";
import type { Settings } from "../lib/types";

// The preview's stand-in for "@tauri-apps/api/core" (docs/ui-preview.md),
// imported as itself: `src/test/setup.ts` mocks the real module, which this
// one only takes its types from. Imported once the console line every
// preview page logs is kept out of the test's output.
let invoke: typeof import("./mockTauri").invoke;

beforeAll(async () => {
  vi.spyOn(console, "info").mockImplementation(() => {});
  ({ invoke } = await import("./mockTauri"));
});

/** Every microtask the task that asked can run, and many more. */
async function drainMicrotasks(): Promise<void> {
  for (let turn = 0; turn < 200; turn += 1) await Promise.resolve();
}

describe("the preview's invoke", () => {
  it("answers in a task after the one that asked, as over Tauri's IPC, even what the backend knows at once", async () => {
    let answered = false;
    const call = invoke<Settings>("get_settings").then((settings) => {
      answered = true;
      return settings;
    });

    // The mock backend has its settings at hand; a page that awaited one
    // answer after another would otherwise run them all in the task that
    // asked, and draw no frame until the last.
    await drainMicrotasks();
    expect(answered).toBe(false);

    const settings = await call;
    expect(answered).toBe(true);
    expect(settings.language).toBe("System");
  });

  it("hands a refusal back the same way, in words of the backend's own", async () => {
    let settled = false;
    const call = invoke("no_such_command").finally(() => {
      settled = true;
    });

    await drainMicrotasks();
    expect(settled).toBe(false);
    await expect(call).rejects.toBe("Command no_such_command not found");
  });

  it("answers in the order the backend did", async () => {
    const order: string[] = [];
    const calls = ["get_settings", "list_operations", "get_snapshot", "get_settings"].map((cmd, index) =>
      invoke(cmd).then(() => order.push(`${index}:${cmd}`)),
    );

    await Promise.all(calls);
    expect(order).toEqual(["0:get_settings", "1:list_operations", "2:get_snapshot", "3:get_settings"]);
  });
});
