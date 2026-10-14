import { afterEach, describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import { HistoryNotice, historyNoticeKey } from "./HistoryNotice";
import { useHistoryClearState } from "../lib/history";
import { NO_HISTORY, type HistoryPersistence, type HistoryView } from "../lib/types";

const mockInvoke = vi.mocked(invoke);

function view(persistence: HistoryPersistence, unsaved_clear = false): HistoryView {
  return { run: "now", cleared_before: null, records: [], persistence, unsaved_clear };
}

afterEach(async () => {
  mockInvoke.mockReset();
  useHistoryClearState.setState({ unconfirmed: false });
  await i18n.changeLanguage("en");
});

describe("historyNoticeKey", () => {
  it("says nothing before an answer, for the stand-in, and for a history being saved", () => {
    expect(historyNoticeKey(undefined)).toBeNull();
    expect(historyNoticeKey(NO_HISTORY)).toBeNull();
    expect(historyNoticeKey(view("saved"))).toBeNull();
    expect(historyNoticeKey(view("pending"))).toBeNull();
  });

  it("has words of its own for each history kept in memory only", () => {
    expect(historyNoticeKey(view("unreadable"))).toBe("unreadable");
    expect(historyNoticeKey(view("malformed"))).toBe("malformed");
    expect(historyNoticeKey(view("newer"))).toBe("newer");
    expect(historyNoticeKey(view("writer_unavailable"))).toBe("writerUnavailable");
    expect(historyNoticeKey(view("unattached"))).toBe("unattached");
  });
});

describe("historyNoticeKey after a write that failed (r24 R74-2)", () => {
  it("says a Clear not in the file as such, and once it is, a later failure as any write", () => {
    expect(historyNoticeKey(view("write_failed", true))).toBe("clearUnsaved");
    expect(historyNoticeKey(view("write_failed", false))).toBe("writeFailed");
    // A write under way says nothing, Clear or not: it ends in a moment.
    expect(historyNoticeKey(view("pending", true))).toBeNull();
  });

  it("says a Clear that went unanswered until one is answered, under what the file says", () => {
    expect(historyNoticeKey(view("saved"), true)).toBe("clearUnconfirmed");
    expect(historyNoticeKey(undefined, true)).toBe("clearUnconfirmed");
    expect(historyNoticeKey(view("write_failed"), true)).toBe("writeFailed");
    expect(historyNoticeKey(view("unreadable"), true)).toBe("unreadable");
  });
});

describe("HistoryNotice after a write that failed", () => {
  const cases: [string, HistoryView, boolean, string, RegExp][] = [
    ["en", view("write_failed", true), false, "Clear History couldn't be saved.", /may be listed again after you reopen it/],
    ["en", view("write_failed"), false, "Update History couldn't be saved.", /tried again after the next update and when you quit/],
    ["en", view("saved"), true, "Couldn't confirm Clear History.", /click Clear History/],
    ["zh-CN", view("write_failed", true), false, "无法保存“清除记录”。", /重新打开后可能会再次列出/],
    ["zh-CN", view("write_failed"), false, "无法保存“最近的更新记录”。", /退出此App时会再试一次/],
    ["zh-CN", view("saved"), true, "无法确认是否已清除记录。", /点按“清除记录”/],
    ["zh-Hant", view("write_failed", true), false, "無法儲存「清除記錄」。", /重新開啟後可能會再次列出/],
    ["zh-Hant", view("write_failed"), false, "無法儲存「最近的更新記錄」。", /結束此App時會再試一次/],
    ["zh-Hant", view("saved"), true, "無法確認是否已清除記錄。", /點按「清除記錄」/],
  ];
  it.each(cases)("in %s says it", async (lang, answer, unconfirmed, title, detail) => {
    await i18n.changeLanguage(lang);
    useHistoryClearState.setState({ unconfirmed });
    mockInvoke.mockImplementation((cmd: string) =>
      cmd === "get_history" ? Promise.resolve(answer) : Promise.resolve(undefined),
    );
    renderWithProviders(<HistoryNotice />);
    expect(await screen.findByText(title)).toBeInTheDocument();
    expect(i18n.t(`updates.justUpdated.storage.${historyNoticeKey(answer, unconfirmed)}Detail`)).toMatch(detail);
  });
});

describe("HistoryNotice", () => {
  // Each one's line, and what its ⓘ says to do, in the three languages: a
  // file that cannot be read is made readable again, a damaged one put
  // back or moved away, a newer one's left to the newer Banager, and the
  // other two name no file at all.
  const cases: [string, HistoryPersistence, string, RegExp][] = [
    ["en", "unreadable", "Update History couldn't be read and isn't being saved.", /make history\.json .* readable again/],
    ["en", "malformed", "Update History is damaged and isn't being saved.", /good copy of history\.json .* Trash/],
    ["en", "newer", "Update History was saved by a newer version of Banager and isn't being saved here.", /use the newer version/],
    ["en", "writer_unavailable", "Update History isn't being saved.", /^Saving it couldn't start\. [^/]*$/],
    ["en", "unattached", "Update History isn't available.", /^[^/]*$/],
    ["zh-CN", "unreadable", "无法读取“最近的更新记录”，目前不会保存。", /history\.json重新可读/],
    ["zh-CN", "malformed", "“最近的更新记录”已损坏，目前不会保存。", /完好副本.*废纸篓/],
    ["zh-CN", "newer", "“最近的更新记录”由较新版本的此App保存，这里不会保存。", /请使用较新版本/],
    ["zh-CN", "writer_unavailable", "目前不会保存“最近的更新记录”。", /^[^/]*$/],
    ["zh-CN", "unattached", "“最近的更新记录”不可用。", /^[^/]*$/],
    ["zh-Hant", "unreadable", "無法讀取「最近的更新記錄」，目前不會儲存。", /history\.json重新可讀取/],
    ["zh-Hant", "malformed", "「最近的更新記錄」已損毀，目前不會儲存。", /完好副本.*垃圾桶/],
    ["zh-Hant", "newer", "「最近的更新記錄」由較新版本的此App儲存，這裡不會儲存。", /請使用較新版本/],
    ["zh-Hant", "writer_unavailable", "目前不會儲存「最近的更新記錄」。", /^[^/]*$/],
    ["zh-Hant", "unattached", "「最近的更新記錄」無法使用。", /^[^/]*$/],
  ];
  it.each(cases)("in %s, for %s, says %s", async (lang, persistence, title, detail) => {
    await i18n.changeLanguage(lang);
    mockInvoke.mockImplementation((cmd: string) =>
      cmd === "get_history" ? Promise.resolve(view(persistence)) : Promise.resolve(undefined),
    );
    renderWithProviders(<HistoryNotice />);
    expect(await screen.findByText(title)).toBeInTheDocument();
    const key = historyNoticeKey(view(persistence));
    expect(i18n.t(`updates.justUpdated.storage.${key}Detail`)).toMatch(detail);
  });

  it("is not drawn for a history being saved", async () => {
    mockInvoke.mockImplementation((cmd: string) =>
      cmd === "get_history" ? Promise.resolve(view("saved")) : Promise.resolve(undefined),
    );
    const { container } = renderWithProviders(<HistoryNotice />);
    await vi.waitFor(() => expect(mockInvoke.mock.calls.some(([cmd]) => cmd === "get_history")).toBe(true));
    expect(container.querySelector("[data-history-notice]")).toBeNull();
  });
});
