import { describe, expect, it, vi } from "vitest";
import i18n from "../i18n";
import {
  failedLookupsNotice,
  failedLookupsOf,
  failedLookupsProblem,
  isFailedLookup,
  saysWhyInToolWords,
} from "./failedLookups";
import type { ArtifactKey, UpdateCandidate } from "./types";

const key = (name: string): ArtifactKey => ({ instance_id: "npm:/usr/local", kind: "Package", name });

function row(name: string, overrides: Partial<UpdateCandidate> = {}): UpdateCandidate {
  return {
    key: key(name),
    current: "1.0.0",
    target: "1.0.0",
    channel: "Native",
    checkable: false,
    warnings: [{ Message: "npm outdated -g: npm error code ENOTFOUND" }, "TransientLookupFailure"],
    blocked: null,
    ...overrides,
  };
}

const noHiding = { ignored_updates: [], skipped_versions: [], snoozed_updates: [] };

describe("isFailedLookup", () => {
  it("is a row the check could not look up, with a tool's own words for why", () => {
    expect(isFailedLookup(row("a"))).toBe(true);
  });

  it("counts pip's empty successful reply with an exhausted connection-abort warning", () => {
    const failed = row("cowsay", {
      key: { instance_id: "pip:python3", kind: "Package", name: "cowsay" },
      warnings: [
        { Message: "pip list --outdated: ProtocolError('Connection aborted.', RemoteDisconnected('Remote end closed connection without response'))" },
        "TransientLookupFailure",
      ],
    });
    expect(isFailedLookup(failed)).toBe(true);
    expect(failedLookupsOf([failed], noHiding)).toEqual([failed]);
    expect(failedLookupsNotice(i18n.getFixedT("en"), [failed])).toMatchObject({
      values: { count: 1 },
      action: { id: "checkAgain", labelKey: "header.checkAgain" },
    });
  });

  it("is not a row no later check will mend, nor one that was checked", () => {
    expect(isFailedLookup(row("crate", { warnings: ["NonRegistrySource"] }))).toBe(false);
    // A 404 for a model made with `ollama create`, Antigravity CLI on an
    // Intel Mac: the tool's words, and no `TransientLookupFailure` from
    // Rust -- never counted, or the warning would never go (walk-2 review
    // 1.1). Its why is still the tool's words, for 「显示原因」.
    for (const words of [
      "registry returned status 404",
      "Antigravity CLI's update manifest is not yet verified on Intel Macs (x86_64)",
    ]) {
      const lasting = row("lasting", { warnings: [{ Message: words }] });
      expect(isFailedLookup(lasting)).toBe(false);
      expect(saysWhyInToolWords(lasting)).toBe(true);
    }
    // A certificate rustls would not accept, a redirect the client will
    // not follow: met again on every check (round-5 review finding 6).
    for (const warnings of [
      [
        { Message: "PyPI request failed: secure connection to pypi.org failed: invalid peer certificate: UnknownIssuer" },
        { SecureConnectionFailed: { host: "pypi.org" } },
      ],
      [{ Message: "PyPI request failed: refused: refusing to follow a redirect" }],
    ] as UpdateCandidate["warnings"][]) {
      expect(isFailedLookup(row("lasting", { warnings }))).toBe(false);
    }
    expect(saysWhyInToolWords(row("crate", { warnings: ["NonRegistrySource"] }))).toBe(false);
    expect(isFailedLookup(row("fine", { checkable: true, target: "1.1.0", warnings: [] }))).toBe(false);
    // Checked, with a note of its own: not a failed lookup.
    expect(isFailedLookup(row("noted", { checkable: true, target: "1.1.0" }))).toBe(false);
  });
});

describe("failedLookupsOf", () => {
  it("uses the caller's clock to reveal a failed lookup exactly when its snooze expires", () => {
    const failed = row("snoozed");
    const until = 1_800_000_000;
    const settings = { ...noHiding, snoozed_updates: [{ key: failed.key, until }] };
    // A view may still be rendering an earlier minute than the wall clock.
    const clock = vi.spyOn(Date, "now").mockReturnValue((until + 60) * 1000);
    try {
      expect(failedLookupsOf([failed], settings, until * 1000 - 1)).toEqual([]);
      expect(failedLookupsOf([failed], settings, until * 1000)).toEqual([failed]);
    } finally {
      clock.mockRestore();
    }
  });

  it("counts the rows the Updates page lists, not one the user hid, nor one no check will mend", () => {
    const rows = [
      row("a"),
      row("b"),
      row("hidden"),
      row("crate", { warnings: ["NonRegistrySource"] }),
      row("model", { warnings: [{ Message: "registry returned status 404" }] }),
    ];
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
    const notice = failedLookupsNotice(zh, [
      row("a"),
      row("b", { warnings: [{ Message: "registry returned status 503" }, "TransientLookupFailure"] }),
    ]);
    expect(zh(notice!.titleKey, notice!.values)).toBe("2个工具没有检查成功");
    expect(zh(notice!.descriptionKey, notice!.values)).toBe("可能还有更新没有列出。可以稍后点按“重新检查”再试。");
    expect(en("updates.lookupsFailedTitle", { count: 1 })).toBe("1 tool couldn't be checked");
    expect(en("warnings.transientLookupFailure")).toBe("You can click “Check Again” later.");
  });

  it("claims only the network as a lookup's cause: an update's words for the others would be wrong of a read", () => {
    // 「没有权限修改它的文件」 of a manifest it could not read, 「请等另一个操作
    // 完成」 of a lookup: no cause is claimed instead (walk-2 review 1.3).
    for (const words of [
      "could not read local manifest /Users/me/.ollama/x: Permission denied (os error 13)",
      "npm outdated -g: npm error code ENOSPC: no space left on device",
      "Another active Homebrew process is already in progress",
    ]) {
      const notice = failedLookupsNotice(en, [row("a", { warnings: [{ Message: words }, "TransientLookupFailure"] })]);
      expect(notice!.titleKey).toBe("updates.lookupsFailedTitle");
      expect(en(notice!.descriptionKey, notice!.values)).toBe(
        "Some updates may not be listed. You can click “Check Again” later.",
      );
    }
  });

  it("gives the Overview's row what it means, why and what to do, under a line that says how many", () => {
    expect(failedLookupsProblem(en, [])).toBeNull();
    const network = failedLookupsProblem(zh, [row("a"), row("b")]);
    expect(network).toMatchObject({ id: "lookups-failed", variant: "warning", action: { id: "checkAgain" } });
    expect(zh(network!.titleKey)).toBe("可能还有更新没有列出");
    expect(zh(network!.descriptionKey)).toBe("网络连接失败，请检查网络连接后重试。");
    const unknown = failedLookupsProblem(en, [
      row("a", { warnings: [{ Message: "registry returned status 503" }, "TransientLookupFailure"] }),
    ]);
    expect(en(unknown!.titleKey)).toBe("Some updates may not be listed");
    expect(en(unknown!.descriptionKey)).toBe("You can click “Check Again” later.");
  });
});
