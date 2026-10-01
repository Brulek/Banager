import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";
import type { HomebrewFacts, InstalledArtifact } from "../lib/types";
import { NO_FACTS } from "../lib/types";
import { HomebrewNotes, REASON_KEYS, homebrewStatusChip, homepageFact, lifecycleSentence } from "./HomebrewStatus";

const zh = i18n.getFixedT("zh-CN");
const enT = i18n.getFixedT("en");

/** A Homebrew artifact carrying `homebrew`. */
function artifact(name: string, homebrew: HomebrewFacts | null, kind: "Formula" | "Cask" = "Formula"): InstalledArtifact {
  return {
    key: { instance_id: "brew:/opt/homebrew", kind, name },
    display_name: name,
    version: "1.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: { ...NO_FACTS, homebrew },
  };
}

const EMPTY: HomebrewFacts = { deprecated: null, disabled: null, caveats: null, other_versions: [] };

// The two cases the build list names: a cask Homebrew disabled for failing
// Gatekeeper on 2026-09-01, and a formula it deprecated in favour of
// another, with a reason that is the maintainers' own sentence.
const disabledCask = artifact(
  "oldapp",
  { ...EMPTY, disabled: { date: "2026-09-01", reason: "fails_gatekeeper_check", replacement: null } },
  "Cask",
);
const FREE_TEXT = "the package is not compatible with Homebrew's installation parameters";
const deprecatedFormula = artifact("oldtool", {
  ...EMPTY,
  deprecated: { date: "2026-06-15", reason: FREE_TEXT, replacement: "newtool" },
  caveats: "To finish, run:\n  oldtool --init",
  other_versions: ["1.9"],
});

describe("lifecycleSentence", () => {
  it("says why, then that a disabled package gets no more updates and stays installed", () => {
    const mark = { kind: "disabled" as const, lifecycle: disabledCask.facts.homebrew!.disabled! };
    expect(lifecycleSentence(zh, mark)).toBe(
      "它没有通过macOS的安全检查。Homebrew自2026-09-01起停用它，以后不再提供更新。已经装好的这一份不会被删除。",
    );
    expect(lifecycleSentence(enT, mark)).toBe(
      "It doesn't pass macOS's security check. Homebrew disabled it on 2026-09-01, so no more updates will come. The copy already installed is not removed.",
    );
  });

  it("quotes a reason it does not know word for word, untranslated", () => {
    const mark = { kind: "deprecated" as const, lifecycle: deprecatedFormula.facts.homebrew!.deprecated! };
    expect(lifecycleSentence(zh, mark)).toBe(
      `Homebrew给出的原因：“${FREE_TEXT}”。Homebrew自2026-06-15起将它标为弃用，以后可能会停用。`,
    );
    expect(lifecycleSentence(enT, mark)).toBe(
      `Homebrew's reason: “${FREE_TEXT}”. Homebrew deprecated it on 2026-06-15 and may disable it later.`,
    );
  });

  it("leaves out what Homebrew did not say: no reason, no date", () => {
    const mark = { kind: "disabled" as const, lifecycle: { date: null, reason: null, replacement: null } };
    expect(lifecycleSentence(zh, mark)).toBe("Homebrew已停用它，以后不再提供更新。已经装好的这一份不会被删除。");
    const deprecated = { kind: "deprecated" as const, lifecycle: { date: null, reason: "unmaintained", replacement: null } };
    expect(lifecycleSentence(zh, deprecated)).toBe("它已经没人维护。Homebrew已将它标为弃用，以后可能会停用。");
  });

  it("has a sentence in both languages for every reason Homebrew 7.0.7 names", () => {
    // `deprecate_disable.rb`: ten formula reasons and seven cask ones, two
    // shared (`unmaintained`, `unreachable`).
    expect(Object.keys(REASON_KEYS)).toHaveLength(15);
    for (const [symbol, key] of Object.entries(REASON_KEYS)) {
      expect(key).toBe(`brewStatus.reason.${symbol}`);
      const reasons = { en: en.brewStatus.reason, zh: zhCN.brewStatus.reason } as Record<string, Record<string, string>>;
      expect(reasons.en[symbol], symbol).toMatch(/\.$/);
      expect(reasons.zh[symbol], symbol).toMatch(/。$/);
    }
  });
});

describe("homebrewStatusChip", () => {
  it("is a quiet word only for a package Homebrew marked, disabled before deprecated", () => {
    expect(homebrewStatusChip(enT, artifact("jq", null))).toBeNull();
    expect(homebrewStatusChip(enT, artifact("jq", { ...EMPTY, other_versions: ["1.7"] }))).toBeNull();
    const disabled = homebrewStatusChip(zh, disabledCask);
    expect(disabled).toMatchObject({ label: "已停用", ariaLabel: "已停用：oldapp", tone: "neutral" });
    expect(homebrewStatusChip(enT, deprecatedFormula)).toMatchObject({ label: "Deprecated", tone: "neutral" });
    const both = artifact("both", {
      ...EMPTY,
      deprecated: { date: "2026-01-01", reason: null, replacement: null },
      disabled: { date: "2026-09-01", reason: null, replacement: null },
    });
    expect(homebrewStatusChip(enT, both)?.label).toBe("Disabled");
  });
});

describe("homepageFact", () => {
  it("shows the address as text and copies it with Copy Link, opening nothing", () => {
    const onCopy = vi.fn();
    const fact = homepageFact(enT, "https://jqlang.github.io/jq/", onCopy);
    expect(fact?.term).toBe("Homepage");
    render(<>{fact?.value}</>);
    expect(screen.getByText("https://jqlang.github.io/jq/").tagName).toBe("SPAN");
    expect(screen.queryByRole("link")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Copy Link" }));
    expect(onCopy).toHaveBeenCalledWith("https://jqlang.github.io/jq/");
  });

  it("is nothing where the source gave no address", () => {
    expect(homepageFact(enT, null, vi.fn())).toBeNull();
    expect(homepageFact(enT, "  ", vi.fn())).toBeNull();
  });
});

describe("HomebrewNotes", () => {
  it("says the suggested name with no button to install it, and the other version with no cause", async () => {
    renderWithProviders(<HomebrewNotes artifact={deprecatedFormula} />);
    expect(screen.getByText(`Homebrew's reason: “${FREE_TEXT}”. Homebrew deprecated it on 2026-06-15 and may disable it later.`)).toBeInTheDocument();
    expect(screen.getByText("Homebrew suggests “newtool” instead.")).toBeInTheDocument();
    expect(screen.getByText("1 other version is also installed: 1.9")).toBeInTheDocument();
    // The one control is the caveats' disclosure: nothing installs, opens or copies.
    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual(["Homebrew's notes in English"]);
    expect(screen.queryByRole("link")).toBeNull();
  });

  it("keeps the caveats closed until asked, then shows them verbatim, still with no Copy", () => {
    const { container } = renderWithProviders(<HomebrewNotes artifact={deprecatedFormula} />);
    const disclosure = screen.getByRole("button", { name: "Homebrew's notes in English" });
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
    expect(container.querySelector("[data-caveats]")).toBeNull();
    fireEvent.click(disclosure);
    expect(disclosure).toHaveAttribute("aria-expanded", "true");
    const caveats = container.querySelector("[data-caveats]");
    expect(caveats?.textContent).toBe("To finish, run:\n  oldtool --init");
    expect(caveats).toHaveClass("whitespace-pre-wrap");
    expect(caveats).toHaveAttribute("lang", "en");
    expect(screen.getAllByRole("button")).toHaveLength(1);
  });

  it("says the other versions in Chinese, listed with 、", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(<HomebrewNotes artifact={artifact("openssl@3", { ...EMPTY, other_versions: ["3.6.2", "3.6.3"] })} />);
      expect(screen.getByText("另外还装着2个其他版本：3.6.2、3.6.3")).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says nothing for a package Homebrew has nothing to say about", () => {
    const { container } = renderWithProviders(<HomebrewNotes artifact={artifact("jq", null)} />);
    expect(container.querySelector("[data-homebrew-notes]")).toBeNull();
  });
});
