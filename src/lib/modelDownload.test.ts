import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { formatBytes } from "./format";
import { downloadBytesWorthSaying, formatBytesRoundedUp, modelDownloadNote } from "./modelDownload";
import { warningLines, warningText } from "./warnings";
import type { UpdateCandidate, Warning } from "./types";
import { updateVersionColumn } from "../components/updateDetails";

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

describe("formatBytesRoundedUp", () => {
  it("rounds up to the decimal shown, so an upper bound never reads lower than itself", () => {
    // `formatBytes` rounds to the nearest: 4.6 GB for 4.64.
    expect(formatBytes(4_640_000_000)).toBe("4.6 GB");
    expect(formatBytesRoundedUp(4_640_000_000)).toBe("4.7 GB");
    expect(formatBytesRoundedUp(4_683_087_520)).toBe("4.7 GB");
    expect(formatBytesRoundedUp(2_542_796_928)).toBe("2.6 GB");
    expect(formatBytesRoundedUp(4_600_000_001)).toBe("4.7 GB");
  });

  it("writes an exact size as formatBytes does", () => {
    // Sizes with nothing past the one decimal: no rounding either way.
    for (const bytes of [251, 999, 1_000, 312_000_000, 4_600_000_000, 4_700_000_000, 1_000_000_000]) {
      expect(formatBytesRoundedUp(bytes), String(bytes)).toBe(formatBytes(bytes));
    }
    expect(formatBytesRoundedUp(251)).toBe("251 B");
    expect(formatBytesRoundedUp(1_000)).toBe("1 KB");
    expect(formatBytesRoundedUp(312_000_000)).toBe("312 MB");
    expect(formatBytesRoundedUp(4_700_000_000)).toBe("4.7 GB");
    expect(formatBytesRoundedUp(18_174_721_847)).toBe("18.2 GB");
  });

  it("moves to the next unit where rounding up reaches 1000", () => {
    expect(formatBytesRoundedUp(999_950)).toBe("1 MB");
    expect(formatBytesRoundedUp(999_999_999)).toBe("1 GB");
    expect(formatBytesRoundedUp(999_900_001)).toBe("1 GB");
    expect(formatBytesRoundedUp(999_900_000)).toBe("999.9 MB");
  });
});

describe("modelDownloadNote", () => {
  it("says the most an update downloads, in either language, as an upper bound", () => {
    expect(modelDownloadNote(zh, 4_683_087_520)).toBe("需要下载已更改的模型文件，最多约4.7 GB。");
    // "about" held to its number, as the window's other sizes are.
    expect(modelDownloadNote(en, 4_683_087_520)).toBe("Downloads the model files that changed, up to about\u00a04.7 GB.");
    expect(modelDownloadNote(zh, 4_640_000_000)).toBe("需要下载已更改的模型文件，最多约4.7 GB。");
    // A small change, in the units `formatBytes` picks.
    expect(modelDownloadNote(zh, 312_000_000)).toBe("需要下载已更改的模型文件，最多约312 MB。");
  });

  it("says nothing where the number is not known, for the old words to stand", () => {
    for (const bytes of [null, undefined, 0]) {
      expect(modelDownloadNote(zh, bytes)).toBeNull();
      expect(modelDownloadNote(en, bytes)).toBeNull();
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

describe("a model's row", () => {
  it("says only that there is a new version, with or without the size, on the Updates and Installed pages", () => {
    // `updateVersionColumn` draws both pages' version column: the size
    // stays in the confirmation, where a long model name is not cut for it.
    const model: UpdateCandidate = {
      key: { instance_id: "ollama:http://127.0.0.1:11434", kind: "Model", name: "llama3.2:3b" },
      current: "8e4cdead7463ce276b20d4e33341950d7bb40847f70a9882567a188e24ec1f66",
      target: "sha256:25a98d24af806ec8c25c21df601953c6a42f154dfcd8637bc82ec581f1c849aa",
      channel: "Digest",
      checkable: true,
      warnings: [],
      blocked: null,
      download_bytes: 4_683_087_520,
    };
    expect(updateVersionColumn(en, model)).toEqual({ version: "New version" });
    expect(updateVersionColumn(zh, model)).toEqual({ version: "有新版本" });
    expect(updateVersionColumn(zh, { ...model, download_bytes: null })).toEqual({ version: "有新版本" });
  });
});
