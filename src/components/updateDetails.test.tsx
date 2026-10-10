import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import type { UpdateCandidate } from "../lib/types";
import { blockedDetail } from "./updateDetails";

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
