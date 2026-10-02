import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { downloadBytesWorthSaying, modelDownloadNote, modelDownloadVersion } from "./modelDownload";
import { warningLines, warningText } from "./warnings";
import type { Warning } from "./types";

const en = i18n.getFixedT("en");
const zh = i18n.getFixedT("zh-CN");

describe("downloadBytesWorthSaying", () => {
  it("is a known number of bytes above 0, and null for anything else", () => {
    expect(downloadBytesWorthSaying(4_683_087_520)).toBe(4_683_087_520);
    expect(downloadBytesWorthSaying(1)).toBe(1);
    // Not known: null, or left out by a candidate from before the field.
    expect(downloadBytesWorthSaying(null)).toBeNull();
    expect(downloadBytesWorthSaying(undefined)).toBeNull();
    // 「最多约0 B」 reads as something broken.
    expect(downloadBytesWorthSaying(0)).toBeNull();
    expect(downloadBytesWorthSaying(Number.NaN)).toBeNull();
    expect(downloadBytesWorthSaying(-5)).toBeNull();
  });
});

describe("modelDownloadNote and modelDownloadVersion", () => {
  it("say the most an update downloads, in either language, as an upper bound", () => {
    expect(modelDownloadNote(zh, 4_683_087_520)).toBe("需要下载已更改的模型文件，最多约4.7 GB。");
    expect(modelDownloadNote(en, 4_683_087_520)).toBe("Downloads the model files that changed, up to about 4.7 GB.");
    expect(modelDownloadVersion(zh, 2_542_796_928)).toBe("有新版本 · 最多约2.5 GB");
    expect(modelDownloadVersion(en, 2_542_796_928)).toBe("New version · up to about 2.5 GB");
    // A small change, in the units `formatBytes` picks.
    expect(modelDownloadNote(zh, 312_000_000)).toBe("需要下载已更改的模型文件，最多约312 MB。");
  });

  it("say nothing where the number is not known, for the old words to stand", () => {
    for (const bytes of [null, undefined, 0]) {
      expect(modelDownloadNote(zh, bytes)).toBeNull();
      expect(modelDownloadVersion(en, bytes)).toBeNull();
    }
  });
});

describe("a model's update note with its size", () => {
  const plan: Warning[] = [{ ThirdPartyRegistry: { host: "modelscope.cn" } }, "DownloadsModelChanges"];

  it("replaces only DownloadsModelChanges's sentence, in the plan's order", () => {
    const lines = warningLines(zh, plan, [], undefined, 4_683_087_520);
    expect(lines.note.map((line) => line.text)).toEqual([
      "此模型来自modelscope.cn，不是Ollama官方模型库。",
      "需要下载已更改的模型文件，最多约4.7 GB。",
    ]);
    // Still a note, not a caution, with nothing behind an ⓘ.
    expect(lines.note[1]).toMatchObject({ caution: false, detail: null });
  });

  it("keeps the plain sentence when the size is not known, in either language", () => {
    expect(warningLines(zh, plan).note.map((line) => line.text)).toEqual([
      "此模型来自modelscope.cn，不是Ollama官方模型库。",
      "需要下载已更改的模型文件。",
    ]);
    expect(warningText(en, "DownloadsModelChanges", undefined, null)).toBe("Downloads the model files that changed.");
    expect(warningText(en, "DownloadsModelChanges", undefined, 0)).toBe("Downloads the model files that changed.");
  });

  it("gives no other warning the size", () => {
    expect(warningText(en, "CompilesLocally", undefined, 4_683_087_520)).toBe(
      "This compiles on your Mac and takes a while.",
    );
  });
});
