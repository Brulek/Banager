import { afterEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { SHOWN_FOR_MS } from "../lib/clipboard";
import { LINK } from "./ui/controls";
import i18n from "../i18n";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";
import type { HomebrewFacts, InstalledArtifact, Measured, Sizes } from "../lib/types";
import { NO_FACTS, NO_SIZES } from "../lib/types";
import {
  HomebrewCaveats,
  addressWithBreaks,
  REASON_KEYS,
  homebrewStatusChip,
  homepageFact,
  homepageHost,
  homebrewMarkLines,
  lifecycleSentence,
  shownDate,
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
      "它没有通过macOS的安全性检查。Homebrew自2026-09-01起停用它，以后不再提供更新。已安装的这一份不会被删除；不再需要时可以卸载它。",
    );
    expect(lifecycleSentence(enT, mark)).toBe(
      "It doesn't pass the macOS security check. Homebrew disabled it on 2026-09-01 and won't provide more updates. The installed copy isn't removed; uninstall it when you no longer need it.",
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
    expect(lifecycleSentence(zh, mark)).toBe("Homebrew已停用它，以后不再提供更新。已安装的这一份不会被删除；不再需要时可以卸载它。");
    const deprecated = { kind: "deprecated" as const, lifecycle: { date: null, reason: "unmaintained", replacement: null } };
    expect(lifecycleSentence(zh, deprecated)).toBe("它已无人维护。Homebrew已将它标为弃用，以后可能会停用。");
  });

  it("quotes a reason that happens to be the name of an object property, too", () => {
    for (const reason of ["constructor", "toString", "__proto__", "hasOwnProperty"]) {
      const mark = { kind: "disabled" as const, lifecycle: { date: null, reason, replacement: null } };
      expect(lifecycleSentence(enT, mark)).toBe(
        `Homebrew's reason: “${reason}”. Homebrew has disabled it and won't provide more updates. The installed copy isn't removed; uninstall it when you no longer need it.`,
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

  it("shows the site's host as a link that opens the whole address in the default browser, and still copies it with Copy Link", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(undefined);
    const fact = homepageFact(enT, "https://code.claude.com/docs/en/setup", false);
    expect(fact?.term).toBe("Homepage");
    const { container } = renderWithProviders(<>{fact?.value}</>);
    const shown = container.querySelector("[data-homepage]") as HTMLElement;
    expect(shown.textContent).toBe("code.claude.com");
    expect(shown).toHaveAttribute("title", "https://code.claude.com/docs/en/setup");
    // A link, in the accent as a link in a Mac window's text, that opens
    // the page through Banager's own command -- never an <a href> the web
    // view would follow, or offer to open in a window of its own.
    const link = screen.getByRole("link", { name: "code.claude.com" });
    expect(link).toBe(shown);
    expect(link).not.toHaveAttribute("href");
    expect(link.className.split(" ")).toEqual(expect.arrayContaining(LINK.split(" ")));
    fireEvent.click(link);
    expect(invoke).toHaveBeenCalledWith("open_homepage", { address: "https://code.claude.com/docs/en/setup" });
    const button = screen.getByRole("button", { name: "Copy Link" });
    fireEvent.click(button);
    expect(writeText).toHaveBeenCalledWith("https://code.claude.com/docs/en/setup");
    // Its word beside it, as every button in a pane says it -- not in the
    // window's toolbar, which speaks for a row's ⋯ menu.
    const status = button.parentElement?.querySelector('[role="status"]') as HTMLElement;
    await waitFor(() => expect(status).toHaveTextContent(/^Copied$/));
    expect(status.parentElement).toBe(button.parentElement);
  });

  it("sends the address trimmed, as the backend compares it with what the source listed", () => {
    // `homepage::listed_homepage` matches the homepage trimmed, exactly:
    // an address sent with the source's spaces or newline around it would
    // be refused as one no tool lists.
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(undefined);
    renderWithProviders(<>{homepageFact(enT, "  https://jqlang.github.io/jq/\n", false)?.value}</>);
    const link = screen.getByRole("link", { name: "jqlang.github.io" });
    expect(link).toHaveAttribute("title", "https://jqlang.github.io/jq/");
    fireEvent.click(link);
    expect(invoke).toHaveBeenCalledWith("open_homepage", { address: "https://jqlang.github.io/jq/" });
  });

  it("offers no link for a plain http homepage: its host as text, to copy", () => {
    // Decision S9 allows a source's https homepage only; the backend
    // refuses any other (`not_web`), so the page offers none.
    vi.mocked(invoke).mockReset();
    const { container } = renderWithProviders(<>{homepageFact(enT, "http://www.lua.org/", false)?.value}</>);
    expect(screen.queryByRole("link")).toBeNull();
    const shown = container.querySelector("[data-homepage]") as HTMLElement;
    expect(shown.tagName).toBe("SPAN");
    expect(shown.textContent).toBe("lua.org");
    expect(shown).toHaveAttribute("title", "http://www.lua.org/");
    expect(screen.getByRole("button", { name: "Copy Link" })).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("offers no link while the first check's list is shown: the host as text, to copy", () => {
    // The backend opens only a homepage its committed snapshot lists, and
    // the list the page shows meanwhile is not committed: a link would
    // only say 「无法打开」 until the check is done.
    vi.mocked(invoke).mockReset();
    const { container } = renderWithProviders(<>{homepageFact(enT, "https://iterm2.com/", true)?.value}</>);
    expect(screen.queryByRole("link")).toBeNull();
    const shown = container.querySelector("[data-homepage]") as HTMLElement;
    expect(shown.tagName).toBe("SPAN");
    expect(shown.textContent).toBe("iterm2.com");
    expect(shown).toHaveAttribute("title", "https://iterm2.com/");
    expect(shown.className.split(" ")).not.toEqual(expect.arrayContaining(LINK.split(" ")));
    fireEvent.click(shown);
    expect(screen.getByRole("button", { name: "Copy Link" })).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("names a host without its www., and shows whole what is not a web address", () => {
    expect(homepageHost("https://www.python.org/")).toBe("python.org");
    expect(homepageHost("http://jqlang.github.io/jq/")).toBe("jqlang.github.io");
    expect(homepageHost("http://localhost:8080/x")).toBe("localhost:8080");
    expect(homepageHost("https://example.com:443/")).toBe("example.com");
    expect(homepageHost("ftp://example.com/x")).toBeNull();
    expect(homepageHost("not an address")).toBeNull();
  });

  it("says when the browser could not open it, beside the link, for a moment", async () => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockRejectedValue('{"kind":"not_listed"}');
    vi.useFakeTimers();
    try {
      renderWithProviders(<>{homepageFact(enT, "https://jqlang.github.io/jq/", false)?.value}</>);
      const link = screen.getByRole("link", { name: "jqlang.github.io" });
      fireEvent.click(link);
      await act(() => vi.advanceTimersByTimeAsync(0));
      const status = link.parentElement?.querySelector('[role="status"]') as HTMLElement;
      expect(status).toHaveTextContent(/^Couldn't open$/);
      await act(() => vi.advanceTimersByTimeAsync(SHOWN_FOR_MS));
      expect(status).toHaveTextContent(/^$/);
    } finally {
      vi.useRealTimers();
    }
  });

  it("offers no link for a homepage that is no web address: it is shown whole, to copy", () => {
    vi.mocked(invoke).mockReset();
    const { container } = renderWithProviders(<>{homepageFact(enT, "ftp://ftp.gnu.org/gnu/wget/", false)?.value}</>);
    expect(screen.queryByRole("link")).toBeNull();
    const shown = container.querySelector("[data-homepage]") as HTMLElement;
    expect(shown.tagName).toBe("SPAN");
    expect(shown.textContent).toBe("ftp://ftp.gnu.org/gnu/wget/");
    expect(screen.getByRole("button", { name: "Copy Link" })).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("says when the clipboard refused, beside the button", async () => {
    renderWithProviders(<>{homepageFact(enT, "https://jqlang.github.io/jq/", false)?.value}</>);
    const button = screen.getByRole("button", { name: "Copy Link" });
    fireEvent.click(button);
    const status = button.parentElement?.querySelector('[role="status"]') as HTMLElement;
    await waitFor(() => expect(status).toHaveTextContent(/^Couldn't copy$/));
  });

  it("keeps a host on one line, dots and all, where a whole address is shown", () => {
    const { container } = renderWithProviders(<>{addressWithBreaks("https://youtube-dl.org/?q=a&b=c#top")}</>);
    const shown = container;
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
    expect(homepageFact(enT, null, false)).toBeNull();
    expect(homepageFact(enT, "  ", false)).toBeNull();
  });
});

describe("homebrewMarkLines and HomebrewCaveats", () => {
  const Mark = ({ target, language }: { target: InstalledArtifact; language?: string }) => (
    <>{homebrewMarkLines(enT, target, language ?? "en")}</>
  );

  it("says the suggested name with no button to install it, the date as the pane's other dates read", async () => {
    const { container } = renderWithProviders(<Mark target={deprecatedFormula} />);
    expect(container.querySelector("[data-homebrew-mark]")?.textContent).toBe(
      `Homebrew's reason: “${FREE_TEXT}”. Homebrew deprecated it on Jun 15, 2026 and may disable it later.`,
    );
    // The date is never broken ("Jun 15," / "2026").
    expect(screen.getByText("Jun 15, 2026")).toHaveClass("whitespace-nowrap");
    expect(document.querySelector("[data-homebrew-replacement]")?.textContent).toBe("Homebrew suggests “newtool” instead.");
    // The name is never broken across lines ("yt-" / "dlp").
    expect(screen.getByText("newtool")).toHaveClass("whitespace-nowrap");
    // Said once, as the facts' 「其他版本」 row (`otherVersionsFact`), and the caveats come last, apart.
    expect(container.textContent).not.toContain("1.9");
    expect(screen.queryAllByRole("button")).toEqual([]);
    expect(screen.queryByRole("link")).toBeNull();
  });

  it("writes Homebrew's date in Chinese as the install date reads, 「2026年6月15日」", () => {
    const { container } = renderWithProviders(<>{homebrewMarkLines(zh, deprecatedFormula, "zh-CN")}</>);
    expect(container.querySelector("[data-homebrew-mark]")?.textContent).toBe(
      `Homebrew给出的原因：“${FREE_TEXT}”。Homebrew自2026年6月15日起将它标为弃用，以后可能会停用。`,
    );
    expect(shownDate("2026-06-15", "zh-CN")).toBe("2026年6月15日");
    // A date of another shape, or no language, as Homebrew wrote it.
    expect(shownDate("2026-06-15", undefined)).toBe("2026-06-15");
    expect(shownDate("June 2026", "en")).toBe("June 2026");
  });

  it("keeps the caveats closed until asked, then shows them verbatim, still with no Copy", () => {
    const { container } = renderWithProviders(<HomebrewCaveats artifact={deprecatedFormula} />);
    const disclosure = screen.getByRole("button", { name: "Homebrew's notes" });
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
      <Mark
        target={artifact("both", {
          ...EMPTY,
          deprecated: { date: "2026-01-01", reason: null, replacement: "newtool" },
          disabled: { date: "2026-09-01", reason: null, replacement: null },
        })}
      />,
    );
    expect(document.querySelector("[data-homebrew-replacement]")?.textContent).toBe("Homebrew suggests “newtool” instead.");
  });

  it("says nothing for a package Homebrew has nothing to say about", () => {
    expect(homebrewMarkLines(enT, artifact("jq", null), "en")).toEqual([]);
    const { container } = renderWithProviders(<HomebrewCaveats artifact={artifact("jq", null)} />);
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
    expect(textOf(fact?.value)).toBe("3.6.3 " + "约120 MB");
    // Several: 、 between them, and the size is theirs together.
    const two = artifact("openssl@3", { ...EMPTY, other_versions: ["3.6.2", "3.6.3"] });
    expect(textOf(otherVersionsFact(zh, two, sizesOf(two, about(240_000_000)))?.value)).toBe("3.6.2、3.6.3 " + "共约240 MB");
    expect(textOf(otherVersionsFact(enT, two, sizesOf(two, about(240_000_000, { partial: true })))?.value)).toBe(
      "3.6.2, 3.6.3 " + "240 MB or more in all",
    );
  });

  it("says what other versions are behind an ⓘ, without blaming anyone", () => {
    const openssl = artifact("openssl@3", { ...EMPTY, other_versions: ["3.6.3"] });
    renderWithProviders(<>{otherVersionsFact(enT, openssl, undefined)?.value}</>);
    fireEvent.click(screen.getByRole("button", { name: "Details: Other versions" }));
    expect(
      screen.getByText(
        "Other versions Homebrew still keeps, besides the one listed above. Updating or uninstalling it usually removes them too, as its confirmation says. They can't be cleaned up on their own here yet.",
      ),
    ).toBeInTheDocument();
    // Since U9 an update or an uninstall deletes them: not 「目前不能在这里清理它们」.
    expect(zh("clarity.otherVersionsDetail")).toBe(
      "Homebrew还留着的其他版本，不是上面列出的这一版。更新或卸载它时通常会一并删除，确认窗口里会写明；目前不能单独清理。",
    );
  });

  it("names the versions alone while they are measured, or when no size came", () => {
    const openssl = artifact("openssl@3", { ...EMPTY, other_versions: ["3.6.3"] });
    expect(textOf(otherVersionsFact(enT, openssl, undefined)?.value)).toBe("3.6.3 ");
    expect(textOf(otherVersionsFact(enT, openssl, sizesOf(openssl, null, null))?.value)).toBe("3.6.3 ");
    expect(textOf(otherVersionsFact(enT, openssl, sizesOf(openssl, null))?.value)).toBe("3.6.3 ");
  });

  it("names the versions alone when what they take measured 0, never 「约0 B」", () => {
    const openssl = artifact("openssl@3", { ...EMPTY, other_versions: ["3.6.3"] });
    expect(textOf(otherVersionsFact(zh, openssl, sizesOf(openssl, about(0)))?.value)).toBe("3.6.3 ");
  });
});
