import { afterEach, describe, expect, it, vi } from "vitest";
import { act, waitFor } from "@testing-library/react";
import i18n from "../i18n";
import { renderWithProviders } from "../test/setup";
import { lazyDescriptionTable } from "./toolDescriptions";
import { useSearchTexts } from "./useSearchTexts";
import type { SearchText } from "./searchMatch";
import { NO_FACTS, type InstalledArtifact, type ManagerInstance } from "./types";

const brew: ManagerInstance = {
  id: "brew:/opt/homebrew",
  adapter_id: "brew",
  exe_path: "/opt/homebrew/bin/brew",
  prefix: "/opt/homebrew",
  scope: "User",
  version: "7.0.3",
  answered_at: null,
  unverified_version: null,
  read_only_reason: null,
  status: { unavailable: null, notes: [] },
};
const jq: InstalledArtifact = {
  key: { instance_id: brew.id, kind: "Formula", name: "jq" },
  display_name: "jq",
  version: "1.8.2",
  reason: "Requested",
  description: "Lightweight and flexible command-line JSON processor",
  homepage: null,
  size_bytes: null,
  installed_at: null,
  path: null,
  auto_updates: false,
  uninstall_blocked: null,
  facts: NO_FACTS,
};
const snapshot = { instances: [brew], artifacts: [jq] };

// What the probe last made: the texts by tool.
let texts: ReadonlyMap<string, SearchText> | null = null;

function Probe({ searching }: { searching: boolean }) {
  texts = useSearchTexts(snapshot, searching);
  return null;
}

afterEach(async () => {
  texts = null;
  await i18n.changeLanguage("en");
});

describe("useSearchTexts", () => {
  it("reads no table and makes nothing until there is a search, then both languages' lines", async () => {
    const readEnglish = vi.fn(async () => ({}));
    const readChinese = vi.fn(async () => ({ "brew:jq": "命令行JSON处理工具" }));
    const { rerender } = renderWithProviders(<Probe searching={false} />, {
      toolDescriptions: { en: lazyDescriptionTable(readEnglish), "zh-CN": lazyDescriptionTable(readChinese) },
    });
    // Rendered, and every effect run: not even the window's own table.
    await act(async () => {});
    expect(texts).toBeNull();
    expect(readEnglish).not.toHaveBeenCalled();
    expect(readChinese).not.toHaveBeenCalled();

    rerender(<Probe searching />);
    // jq's line as the row shows it, then its Chinese line once that table has arrived.
    await waitFor(() => expect([...(texts?.values() ?? [])][0]?.otherLine).toBe("命令行json处理工具"));
    expect([...(texts?.values() ?? [])][0]).toEqual({
      name: "jq",
      line: "lightweight and flexible command-line json processor",
      packageName: "jq",
      otherLine: "命令行json处理工具",
    });
    expect(readEnglish).toHaveBeenCalledTimes(1);
    expect(readChinese).toHaveBeenCalledTimes(1);
  });
});
