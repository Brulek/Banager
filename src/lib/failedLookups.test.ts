import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { failedLookupsNotice, failedLookupsOf, isFailedLookup } from "./failedLookups";
import type { ArtifactKey, UpdateCandidate } from "./types";

const key = (name: string): ArtifactKey => ({ instance_id: "npm:/usr/local", kind: "Package", name });

function row(name: string, overrides: Partial<UpdateCandidate> = {}): UpdateCandidate {
  return {
    key: key(name),
    current: "1.0.0",
    target: "1.0.0",
    channel: "Native",
    checkable: false,
    warnings: [{ Message: "npm outdated -g: npm error code ENOTFOUND" }],
    blocked: null,
    ...overrides,
  };
}

const noHiding = { ignored_updates: [], skipped_versions: [], snoozed_updates: [] };

describe("isFailedLookup", () => {
  it("is a row the check could not look up, with a tool's own words for why", () => {
    expect(isFailedLookup(row("a"))).toBe(true);
  });

  it("is not a row no check will find, nor one that was checked", () => {
    expect(isFailedLookup(row("crate", { warnings: ["NonRegistrySource"] }))).toBe(false);
    expect(isFailedLookup(row("fine", { checkable: true, target: "1.1.0", warnings: [] }))).toBe(false);
    // Checked, with a note of its own: not a failed lookup.
    expect(isFailedLookup(row("noted", { checkable: true, target: "1.1.0" }))).toBe(false);
  });
});

describe("failedLookupsOf", () => {
  it("counts the rows the Updates page lists, not one the user hid", () => {
    const rows = [row("a"), row("b"), row("hidden"), row("crate", { warnings: ["NonRegistrySource"] })];
    const failed = failedLookupsOf(rows, { ...noHiding, ignored_updates: [key("hidden")] });
    expect(failed.map((each) => each.key.name)).toEqual(["a", "b"]);
  });
});

describe("failedLookupsNotice", () => {
  const en = i18n.getFixedT("en");
  const zh = i18n.getFixedT("zh-CN");

  it("is nothing when every lookup worked", () => {
    expect(failedLookupsNotice(en, [])).toBeNull();
  });

  it("names the cause every row's words give, and its own step, with Check Again", () => {
    const notice = failedLookupsNotice(en, [row("a"), row("b")]);
    expect(notice).toMatchObject({
      id: "lookups-failed",
      variant: "warning",
      action: { id: "checkAgain", labelKey: "header.checkAgain" },
    });
    expect(en(notice!.titleKey, notice!.values)).toBe("2 tools couldn't be checked: Connection failed");
    expect(en(notice!.descriptionKey, notice!.values)).toBe(
      "Some updates may not be listed. Check your internet connection, then try again.",
    );
    expect(zh(notice!.titleKey, { ...notice!.values, cause: zh("failure.cause.network") })).toBe(
      "2个工具没有检查成功：网络连接失败",
    );
  });

  it("claims no cause where one row's words give none, and says to check again later", () => {
    const notice = failedLookupsNotice(zh, [row("a"), row("b", { warnings: [{ Message: "registry answered 500" }] })]);
    expect(zh(notice!.titleKey, notice!.values)).toBe("2个工具没有检查成功");
    expect(zh(notice!.descriptionKey, notice!.values)).toBe("可能还有更新没有列出。可以稍后点按“重新检查”再试。");
    expect(en("updates.lookupsFailedTitle", { count: 1 })).toBe("1 tool couldn't be checked");
  });
});
