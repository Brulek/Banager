import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import type { UpdateCandidate } from "../lib/types";
import { blockedDetail, cannotCheckDetail } from "./updateDetails";

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
      render(<>{cannotCheckDetail(i18n.getFixedT(lang), candidate, false)}</>).container.textContent;
    expect(said("en")).toBe(
      "rustup is set to get updates from a different address, which Banager hasn't checked, so it can't update rustup here.",
    );
    expect(said("zh-CN")).toBe("rustup设为从另一个地址获取更新。这里没有检查该地址，因此无法在这里更新rustup。");
    expect(said("zh-Hant")).toBe("rustup設為從另一個位址取得更新。這裡沒有檢查該位址，因此無法在這裡更新rustup。");
  });
});
