import { describe, expect, it } from "vitest";
import { ELLIPSIS, middleCut } from "./middleCut";

/** Every character 7 wide, "…" too: a monospaced font, so the sums are easy to check. */
const mono = (text: string) => text.length * 7;

describe("middleCut", () => {
  const name = "modelscope.cn/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M";

  it("leaves a name that fits whole", () => {
    expect(middleCut(name, mono(name), mono, 12)).toBe(name);
    expect(middleCut("jq", 10, mono, 12)).toBe("jq");
  });

  it("is one string -- start, …, end -- with nothing between the three", () => {
    const cut = middleCut(name, 300, mono, 12);
    expect(cut).toBe(`${name.slice(0, 29)}${ELLIPSIS}-GGUF:Q4_K_M`);
    // No space, and no second box to leave a hole before the end.
    expect(cut).not.toMatch(/\s/);
    expect(cut.split(ELLIPSIS)).toEqual([name.slice(0, 29), name.slice(-12)]);
  });

  it("keeps the longest start that fits the room, and no longer", () => {
    for (const room of [120, 200, 301, 391]) {
      const cut = middleCut(name, room, mono, 12);
      expect(mono(cut)).toBeLessThanOrEqual(room);
      // One more character of the start would not fit.
      const start = cut.split(ELLIPSIS)[0];
      expect(mono(`${name.slice(0, start.length + 1)}${ELLIPSIS}${name.slice(-12)}`)).toBeGreaterThan(room);
    }
  });

  it("measures with the font it is given, not by counting characters", () => {
    // Wide capitals, narrow everything else.
    const proportional = (text: string) => [...text].reduce((sum, c) => sum + (/[A-Z]/.test(c) ? 10 : 5), 0);
    const cut = middleCut(name, 200, proportional, 12);
    expect(proportional(cut)).toBeLessThanOrEqual(200);
    expect(cut.endsWith(`${ELLIPSIS}-GGUF:Q4_K_M`)).toBe(true);
  });

  it("keeps a character of the start, …, and the end where even that is too wide, for its box to cut", () => {
    expect(middleCut(name, 20, mono, 12)).toBe(`m${ELLIPSIS}-GGUF:Q4_K_M`);
  });
});
