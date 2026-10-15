import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import i18n from "../i18n";
import type { CommandState, InstalledArtifact, ManagerInstance, UpdateCandidate } from "../lib/types";
import { NO_FACTS } from "../lib/types";
import { blockedDetail, cannotCheckDetail, updateVersionColumn } from "./updateDetails";

describe("blockedDetail", () => {
  it("names the package that updates with Homebrew's Node, npm or corepack (R42-2)", () => {
    // `UpdateBlocked::UpdatesWithFormula` is npm's own package's, and
    // corepack's, where its command is a Homebrew formula's link: the
    // sentence says which of the two it is.
    const candidate = (name: string): UpdateCandidate => ({
      key: { instance_id: "npm:/opt/homebrew", kind: "Package", name },
      current: "1.0.0",
      target: "1.0.1",
      channel: "Native",
      checkable: true,
      warnings: [],
      blocked: "UpdatesWithFormula",
    });
    const said = (lang: string, name: string) =>
      render(
        <>{blockedDetail(i18n.getFixedT(lang), candidate(name), "UpdatesWithFormula", undefined, "npm", false)}</>,
      ).container.textContent;
    expect(said("en", "corepack")).toBe(
      "corepack comes with the Node that Homebrew installed and updates along with it. Updating it on its own here would make Homebrew's next update of that Node fail.",
    );
    expect(said("en", "npm")).toContain("npm comes with the Node that Homebrew installed");
    expect(said("zh-CN", "corepack")).toContain("corepack是Homebrew安装的Node自带的，会随Node一起更新。");
    expect(said("zh-Hant", "corepack")).toContain("corepack是Homebrew安裝的Node附帶的，會隨Node一起更新。");
  });
});

describe("cannotCheckDetail", () => {
  it("says rustup is set to another update address, not that it isn't checked on this Mac (R75-3)", () => {
    // `check_updates` in crates/banager-core/src/adapters/standalone/mod.rs:
    // no lookup, the installed version as target, and `blocked` naming why.
    const candidate: UpdateCandidate = {
      key: { instance_id: "standalone-rustup", kind: "Binary", name: "rustup" },
      current: "1.29.1",
      target: "1.29.1",
      channel: "Registry",
      checkable: false,
      warnings: [
        { Message: "rustup is set to get its updates from another address than the one Banager checks" },
        "NotLookedUpHere",
      ],
      blocked: "UpdateSourceMismatch",
    };
    const said = (lang: string) =>
      [
        ...render(<>{cannotCheckDetail(i18n.getFixedT(lang), candidate, false)}</>).container.querySelectorAll(
          "[data-detail-line]",
        ),
      ]
        .map((line) => line.textContent)
        .join("");
    expect(said("en")).toBe(
      "rustup is set to get updates from a different address, which Banager hasn't checked, so it can't update rustup here.",
    );
    expect(said("zh-CN")).toBe("rustup设为从另一个地址获取更新。这里没有检查该地址，因此无法在这里更新rustup。");
    expect(said("zh-Hant")).toBe("rustup設為從另一個位址取得更新。這裡沒有檢查該位址，因此無法在這裡更新rustup。");
  });

  it("lets a pip package whose index refused the check be copied, with or without technical details (r25 R70-1)", async () => {
    // What `check_updates` in crates/banager-core/src/adapters/pip.rs
    // gives a package a sole index answered 403 for: the words carry no
    // address (`without_addresses`), and no "check again".
    const reason = "pip list --outdated: the package index answered HTTP 403 (Forbidden), and no index gave its latest version";
    const candidate: UpdateCandidate = {
      key: { instance_id: "pip:/opt/homebrew/bin/python3.14", kind: "Package", name: "corp-tool" },
      current: "2.0",
      target: "2.0",
      channel: "Native",
      checkable: false,
      warnings: [{ Message: reason }, { Message: reason }],
      blocked: null,
    };
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    for (const [lang, short, copy] of [
      ["en", "Couldn't find its latest version.", "Copy Error Details"],
      ["zh-CN", i18n.getFixedT("zh-CN")("updates.cannotCheckShort"), i18n.getFixedT("zh-CN")("failureRecovery.copy")],
      ["zh-Hant", i18n.getFixedT("zh-Hant")("updates.cannotCheckShort"), i18n.getFixedT("zh-Hant")("failureRecovery.copy")],
    ]) {
      for (const technical of [false, true]) {
        const { container, getByRole, unmount } = render(
          <>{cannotCheckDetail(i18n.getFixedT(lang), candidate, technical)}</>,
        );
        const lines = [...container.querySelectorAll("[data-detail-line]")].map((line) => line.textContent);
        expect(lines).toEqual(technical ? [short, reason] : [short]);
        fireEvent.click(getByRole("button", { name: copy }));
        expect(writeText).toHaveBeenLastCalledWith(reason);
        unmount();
      }
    }
    // A row with no words of a tool's has nothing to copy.
    const { queryByRole } = render(
      <>{cannotCheckDetail(i18n.getFixedT("en"), { ...candidate, warnings: ["TransientLookupFailure"] }, true)}</>,
    );
    expect(queryByRole("button", { name: "Copy Error Details" })).toBeNull();
  });
});

describe("updateVersionColumn", () => {
  it("uses the version read from a self-updated app for the version change (r25 R77-3)", () => {
    const firefox: UpdateCandidate = {
      key: { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "firefox" },
      current: "130.0",
      target: "131.0.3",
      channel: "Native",
      checkable: true,
      warnings: [],
      blocked: null,
      display_current: "131.0.2",
    };
    const en = i18n.getFixedT("en");
    expect(updateVersionColumn(en, firefox)).toEqual({ version: "131.0.2 → 131.0.3", newVersion: "131.0.3" });
    // Without one, Homebrew's record; a row that can't be checked shows
    // the record it has, as before.
    expect(updateVersionColumn(en, { ...firefox, display_current: undefined }).version).toBe("130.0 → 131.0.3");
    expect(updateVersionColumn(en, { ...firefox, checkable: false, target: "130.0" }).version).toBe("130.0");
  });
});

describe("blockedDetail for Antigravity CLI (r25 R75-4)", () => {
  // Google documents `agy update`; Banager hasn't verified running it, so
  // the row has no Update, and opening the tool checks only while its
  // automatic updates are on.
  const agy: UpdateCandidate = {
    key: { instance_id: "standalone-agy", kind: "Binary", name: "agy" },
    current: "1.2.10",
    target: "1.3.0",
    channel: "Registry",
    checkable: true,
    warnings: [],
    blocked: "SelfUpdatesOnly",
  };
  const instance = { exe_path: "/Users/test/.local/bin/agy" } as ManagerInstance;
  const artifact = (state: CommandState | null): InstalledArtifact =>
    ({
      key: agy.key,
      version: "1.2.10",
      facts: { ...NO_FACTS, commands: state === null ? [] : [{ name: "agy", state }] },
    }) as unknown as InstalledArtifact;
  const NOTE: Record<string, string> = {
    en: "Updating Antigravity CLI from here hasn't been verified yet, so Banager doesn't offer Update for it.",
    "zh-CN": "尚未验证从这里更新Antigravity CLI的行为，因此暂不提供更新操作。",
    "zh-Hant": "尚未驗證從這裡更新Antigravity CLI的行為，因此暫不提供更新操作。",
  };
  const AUTOMATIC: Record<string, string> = { en: "automatic updates", "zh-CN": "自动更新", "zh-Hant": "自動更新" };

  it.each(["en", "zh-CN", "zh-Hant"])("says why there is no Update in %s, typed or not", (lang) => {
    for (const state of ["Runs", "Shadowed", null] as const) {
      for (const technical of [false, true]) {
        const lines = [
          ...render(
            <>
              {blockedDetail(
                i18n.getFixedT(lang),
                agy,
                "SelfUpdatesOnly",
                instance,
                "Antigravity CLI",
                technical,
                artifact(state === null ? null : (state as CommandState)),
              )}
            </>,
          ).container.querySelectorAll("[data-detail-line]"),
        ].map((line) => line.textContent ?? "");
        const said = `${lang} ${state} ${technical}: ${lines.join(" | ")}`;
        expect(lines[0], said).toContain(AUTOMATIC[lang]);
        expect(lines, said).toContain(NOTE[lang]);
        // Only this copy's name is typed, and only where it runs.
        expect(lines[0].includes("agy"), said).toBe(state === "Runs");
      }
    }
  });
});
