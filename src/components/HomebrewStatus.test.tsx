import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";
import type { HomebrewFacts, InstalledArtifact, Measured, Sizes } from "../lib/types";
import { NO_FACTS, NO_SIZES } from "../lib/types";
import {
  HomebrewNotes,
  REASON_KEYS,
  homebrewStatusChip,
  homepageFact,
  lifecycleSentence,
  otherVersionsFact,
} from "./HomebrewStatus";

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

  it("quotes a reason that happens to be the name of an object property, too", () => {
    for (const reason of ["constructor", "toString", "__proto__", "hasOwnProperty"]) {
      const mark = { kind: "disabled" as const, lifecycle: { date: null, reason, replacement: null } };
      expect(lifecycleSentence(enT, mark)).toBe(
        `Homebrew's reason: “${reason}”. Homebrew has disabled it, so no more updates will come. The copy already installed is not removed.`,
      );
    }
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
  afterEach(() => {
    Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
  });

  it("shows the address as text and copies it with Copy Link, opening nothing", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const fact = homepageFact(enT, "https://jqlang.github.io/jq/");
    expect(fact?.term).toBe("Homepage");
    const { container } = renderWithProviders(<>{fact?.value}</>);
    const shown = container.querySelector("[data-homepage]") as HTMLElement;
    expect(shown.textContent).toBe("https://jqlang.github.io/jq/");
    expect(shown.tagName).toBe("SPAN");
    // A line may break after // and before each /, never inside a name
    // nor at its dots: "https://" | "jqlang.github.io" | "/jq" | "/".
    expect(shown.querySelectorAll("wbr")).toHaveLength(3);
    expect(shown).toHaveClass("break-words");
    expect(screen.queryByRole("link")).toBeNull();
    const button = screen.getByRole("button", { name: "Copy Link" });
    fireEvent.click(button);
    expect(writeText).toHaveBeenCalledWith("https://jqlang.github.io/jq/");
    // Its word beside it, as every button in a pane says it -- not in the
    // window's toolbar, which speaks for a row's ⋯ menu.
    const status = await screen.findByRole("status");
    await waitFor(() => expect(status).toHaveTextContent(/^Copied$/));
    expect(status.parentElement).toBe(button.parentElement);
  });

  it("says when the clipboard refused, beside the button", async () => {
    renderWithProviders(<>{homepageFact(enT, "https://jqlang.github.io/jq/")?.value}</>);
    fireEvent.click(screen.getByRole("button", { name: "Copy Link" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/^Couldn't copy$/));
  });

  it("keeps a host on one line, dots and all", () => {
    const { container } = renderWithProviders(
      <>{homepageFact(enT, "https://youtube-dl.org/?q=a&b=c#top")?.value}</>,
    );
    const shown = container.querySelector("[data-homepage]") as HTMLElement;
    expect(shown.textContent).toBe("https://youtube-dl.org/?q=a&b=c#top");
    // Each piece between two <wbr> is what may stand at a line's start:
    // "youtube-dl.org" whole, never ".org" -- and as one inline block,
    // never "dl.org" after the browser's break at its hyphen.
    const pieces = Array.from(shown.childNodes)
      .filter((node) => node.nodeName !== "WBR")
      .map((node) => node.textContent);
    expect(pieces).toEqual(["https://", "youtube-dl.org", "/", "?q", "=a", "&b", "=c", "#top"]);
    const host = shown.querySelector("[data-host]");
    expect(host?.textContent).toBe("youtube-dl.org");
    expect(host).toHaveClass("inline-block", "max-w-full");
  });

  it("is nothing where the source gave no address", () => {
    expect(homepageFact(enT, null)).toBeNull();
    expect(homepageFact(enT, "  ")).toBeNull();
  });
});

describe("HomebrewNotes", () => {
  it("says the suggested name with no button to install it, and leaves the other versions to the facts", async () => {
    const { container } = renderWithProviders(<HomebrewNotes artifact={deprecatedFormula} />);
    expect(container.querySelector("[data-homebrew-mark]")?.textContent).toBe(
      `Homebrew's reason: “${FREE_TEXT}”. Homebrew deprecated it on 2026-06-15 and may disable it later.`,
    );
    // The date is never broken at its hyphens ("2026-" / "06-15").
    expect(screen.getByText("2026-06-15")).toHaveClass("whitespace-nowrap");
    expect(document.querySelector("[data-homebrew-replacement]")?.textContent).toBe("Homebrew suggests “newtool” instead.");
    // The name is never broken across lines ("yt-" / "dlp").
    expect(screen.getByText("newtool")).toHaveClass("whitespace-nowrap");
    // Said once, as the facts' 「其他版本」 row (`otherVersionsFact`).
    expect(container.textContent).not.toContain("1.9");
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

  it("says the replacement a disabled package named only when Homebrew deprecated it", () => {
    renderWithProviders(
      <HomebrewNotes
        artifact={artifact("both", {
          ...EMPTY,
          deprecated: { date: "2026-01-01", reason: null, replacement: "newtool" },
          disabled: { date: "2026-09-01", reason: null, replacement: null },
        })}
      />,
    );
    expect(document.querySelector("[data-homebrew-replacement]")?.textContent).toBe("Homebrew suggests “newtool” instead.");
  });

  it("says nothing for a package Homebrew has nothing to say about", () => {
    const { container } = renderWithProviders(<HomebrewNotes artifact={artifact("jq", null)} />);
    expect(container.querySelector("[data-homebrew-notes]")).toBeNull();
  });
});

describe("otherVersionsFact", () => {
  const about = (bytes: number, more: Partial<Measured> = {}): Measured => ({
    bytes,
    partial: false,
    at_least: false,
    ...more,
  });
  const sizesOf = (target: InstalledArtifact, other: Measured | null, measured: Measured | null = about(1)): Sizes => ({
    ...NO_SIZES,
    round: 2,
    done: measured !== null,
    artifacts: [{ key: target.key, version: target.version, measured, old_versions: other }],
  });
  const textOf = (value: unknown) => render(<>{value}</>).container.textContent;

  it("is nothing for a package with no other versions", () => {
    expect(otherVersionsFact(enT, artifact("jq", EMPTY), undefined)).toBeNull();
    expect(otherVersionsFact(enT, artifact("jq", null), undefined)).toBeNull();
    // A size with no versions to put it under is not said on its own.
    const jq = artifact("jq", EMPTY);
    expect(otherVersionsFact(enT, jq, sizesOf(jq, about(5_000_000)))).toBeNull();
  });

  it("names the versions, and once measured what they take, in one row: 其他版本", () => {
    const openssl = artifact("openssl@3", { ...EMPTY, other_versions: ["3.6.3"] });
    const fact = otherVersionsFact(zh, openssl, sizesOf(openssl, about(120_000_000)));
    expect(fact?.term).toBe("其他版本");
    expect(fact?.selectable).toBe(true);
    expect(textOf(fact?.value)).toBe("3.6.3" + "约120 MB");
    // Several: 、 between them, and the size is theirs together.
    const two = artifact("openssl@3", { ...EMPTY, other_versions: ["3.6.2", "3.6.3"] });
    expect(textOf(otherVersionsFact(zh, two, sizesOf(two, about(240_000_000)))?.value)).toBe("3.6.2、3.6.3" + "共约240 MB");
    expect(textOf(otherVersionsFact(enT, two, sizesOf(two, about(240_000_000, { partial: true })))?.value)).toBe(
      "3.6.2, 3.6.3" + "At least about\u00a0240 MB in all",
    );
  });

  it("names the versions alone while they are measured, or when no size came", () => {
    const openssl = artifact("openssl@3", { ...EMPTY, other_versions: ["3.6.3"] });
    expect(textOf(otherVersionsFact(enT, openssl, undefined)?.value)).toBe("3.6.3");
    expect(textOf(otherVersionsFact(enT, openssl, sizesOf(openssl, null, null))?.value)).toBe("3.6.3");
    expect(textOf(otherVersionsFact(enT, openssl, sizesOf(openssl, null))?.value)).toBe("3.6.3");
  });
});
