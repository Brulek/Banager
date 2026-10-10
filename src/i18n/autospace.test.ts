import { afterEach, describe, expect, it, vi } from "vitest";
import i18n from "./index";
import { AUTOSPACE, autospace, autospacePostProcessor, lacksTextAutospace } from "./autospace";

const gap = AUTOSPACE;

describe("autospace", () => {
  it("puts a narrow space where Chinese meets a Latin letter or a digit, in either order", () => {
    expect(autospace("有12个工具")).toBe(`有${gap}12${gap}个工具`);
    expect(autospace("更新ffmpeg：已成功")).toBe(`更新${gap}ffmpeg：已成功`);
    expect(autospace("用Homebrew安装的App")).toBe(`用${gap}Homebrew${gap}安装的${gap}App`);
    // One Latin letter between two Chinese characters gets a gap on each side.
    expect(autospace("中a中")).toBe(`中${gap}a${gap}中`);
  });

  it("leaves the space between two Latin words, and a unit after a number, as it is", () => {
    expect(autospace("退出App Store")).toBe(`退出${gap}App Store`);
    expect(autospace("大于1 GB的App")).toBe(`大于${gap}1 GB${gap}的${gap}App`);
  });

  it("changes nothing next to Chinese punctuation, in English, or where a space is already there", () => {
    for (const text of ["“ffmpeg”", "无法卸载，因为设置了“UV_TOOL_DIR”。", "Update All", "中 a", "简体中文"]) {
      expect(autospace(text)).toBe(text);
    }
  });

  it("spaces a string once however often it runs", () => {
    for (const text of ["还有3条", "更新ffmpeg：已成功", "中a中"]) {
      expect(autospace(autospace(text))).toBe(autospace(text));
    }
  });
});

describe("lacksTextAutospace", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("is true only where CSS.supports is there to ask and says no", () => {
    vi.stubGlobal("CSS", { supports: () => false });
    expect(lacksTextAutospace()).toBe(true);
    vi.stubGlobal("CSS", { supports: () => true });
    expect(lacksTextAutospace()).toBe(false);
    vi.stubGlobal("CSS", {});
    expect(lacksTextAutospace()).toBe(false);
    vi.stubGlobal("CSS", undefined);
    expect(lacksTextAutospace()).toBe(false);
  });

  it("asks about text-autospace: normal", () => {
    const supports = vi.fn(() => true);
    vi.stubGlobal("CSS", { supports });
    lacksTextAutospace();
    expect(supports).toHaveBeenCalledWith("text-autospace", "normal");
  });
});

describe("the autospace post-processor", () => {
  it("spaces Chinese only", () => {
    const run = (lng: string) => autospacePostProcessor.process("有12个工具", "k", { lng }, {});
    expect(run("zh-Hant")).toBe(`有${gap}12${gap}个工具`);
    expect(run("zh-CN")).toBe(`有${gap}12${gap}个工具`);
    expect(run("en")).toBe("有12个工具");
  });

  it("falls back to the translator's language when the call names none", () => {
    expect(autospacePostProcessor.process("有12个工具", "k", {}, { language: "zh-CN" })).toBe(
      `有${gap}12${gap}个工具`,
    );
    expect(autospacePostProcessor.process("有12个工具", "k", {}, { language: "en" })).toBe("有12个工具");
  });

  it("is off in the tests' web view, which says it spaces Chinese itself, so they see the strings as written", async () => {
    // jsdom answers yes to CSS.supports("text-autospace", "normal").
    expect(lacksTextAutospace()).toBe(false);
    expect(i18n.options.postProcess).toBe(false);
    const zh = i18n.getFixedT("zh-CN");
    expect(zh("updates.count", { count: 3 })).toBe("3个可更新");
  });

  it("spaces what is put into a string too: 今天 and the time, where the web view cannot", () => {
    // `history.today` has no space of its own (src/components/JustUpdated.tsx):
    // on macOS 13.3-15.3 this processor puts the gap in after the time is.
    const zh = i18n.getFixedT("zh-CN");
    expect(zh("history.today", { time: "14:02", postProcess: "autospace" })).toBe(`今天${gap}14:02`);
    expect(zh("history.today", { time: "14:02" })).toBe("今天14:02");
  });
});
