import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import type { SourceNoticeSpec } from "../lib/sources";
import type { Snapshot } from "../lib/types";
import { CheckAgain } from "./PageHeader";
import { SourceNotices, useNoticeFold } from "./SourceNotices";

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockResolvedValue(undefined);
});

// Notices as `sourceNoticesFor` and the Installed page write them.
const brewUpdating: SourceNoticeSpec = {
  id: "brew:index-updating",
  variant: "info",
  titleKey: "sourceNotice.indexUpdating.title",
  descriptionKey: "sourceNotice.indexUpdating.description",
};
const brewStale: SourceNoticeSpec = {
  id: "brew:index-may-be-stale",
  variant: "warning",
  titleKey: "sourceNotice.indexMayBeStale.title",
  descriptionKey: "sourceNotice.indexMayBeStale.description",
  action: { id: "checkAgain", labelKey: "header.checkAgain" },
};
const uvSilent: SourceNoticeSpec = {
  id: "uv:unreachable",
  variant: "warning",
  titleKey: "sourceNotice.unreachable.title",
  descriptionKey: "sourceNotice.unreachable.descriptionWithRows",
  values: { source: "uv" },
};
const ollamaStopped: SourceNoticeSpec = {
  id: "ollama:not-running",
  variant: "warning",
  titleKey: "sourceNotice.notRunning.title",
  descriptionKey: "sourceNotice.notRunning.description",
  values: { source: "Ollama" },
  action: { id: "openOllama", labelKey: "sourceNotice.openOllama" },
};
const claudeUntested: SourceNoticeSpec = {
  id: "standalone-claude:untested",
  variant: "info",
  titleKey: "installed.unverifiedVersion",
  descriptionKey: "installed.unverifiedVersionDetail",
  values: { source: "Claude Code", version: "2.1.290" },
};
const npmUntested: SourceNoticeSpec = {
  ...claudeUntested,
  id: "npm:untested",
  values: { source: "npm", version: "12.1.0" },
};

/** A page's notice lines under its fold, as the Updates and Installed pages draw them. */
function Folded({ notices }: { notices: SourceNoticeSpec[] }) {
  const fold = useNoticeFold(notices.length);
  return <SourceNotices notices={notices} layout="line" fold={fold} />;
}

/** The lines on screen, top to bottom, by the notice each "Details" belongs to. */
function linesShown(): string[] {
  return screen
    .getAllByRole("button", { name: /^Details: / })
    .map((button) => (button.getAttribute("aria-label") ?? "").replace(/^Details: /, ""));
}

describe("SourceNotices, folded", () => {
  it("shows one line while two or more are folded, with how many more at its end", () => {
    renderWithProviders(<Folded notices={[brewUpdating, claudeUntested, npmUntested]} />);

    // No warning among them: the first line.
    expect(linesShown()).toEqual(["Homebrew is updating its software list"]);
    expect(screen.queryByText("Claude Code 2.1.290 not tested")).toBeNull();
    expect(screen.queryByText("npm 12.1.0 not tested")).toBeNull();
    const more = screen.getByRole("button", { name: "2 More Issues" });
    // Last in that line, after its own "Details".
    expect(screen.getByText("Homebrew is updating its software list").parentElement?.lastElementChild).toBe(more);
  });

  it("shows the first warning, ahead of information that comes before it", () => {
    renderWithProviders(<Folded notices={[brewUpdating, uvSilent, ollamaStopped, claudeUntested]} />);

    expect(linesShown()).toEqual(["uv isn't responding"]);
    expect(screen.getByRole("button", { name: "3 More Issues" })).toBeInTheDocument();
  });

  it("shows every line in its order when pressed, and folds them again with Show fewer after the last", () => {
    renderWithProviders(<Folded notices={[brewUpdating, uvSilent, claudeUntested]} />);

    fireEvent.click(screen.getByRole("button", { name: "2 More Issues" }));

    // In their order: the warning shown folded goes back to its place.
    expect(linesShown()).toEqual([
      "Homebrew is updating its software list",
      "uv isn't responding",
      "Claude Code 2.1.290 not tested",
    ]);
    expect(screen.queryByRole("button", { name: "2 More Issues" })).toBeNull();
    const fewer = screen.getByRole("button", { name: "Show Fewer" });
    expect(
      screen.getByText("Claude Code 2.1.290 not tested").compareDocumentPosition(fewer) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    fireEvent.click(fewer);

    expect(linesShown()).toEqual(["uv isn't responding"]);
    expect(screen.queryByRole("button", { name: "Show Fewer" })).toBeNull();
    expect(screen.getByRole("button", { name: "2 More Issues" })).toBeInTheDocument();
  });

  it("folds and unfolds with a real button that says whether the lines are shown, and which", () => {
    renderWithProviders(<Folded notices={[brewUpdating, uvSilent]} />);

    const more = screen.getByRole("button", { name: "1 More Issue" });
    expect(more.tagName).toBe("BUTTON");
    expect(more).toHaveAttribute("type", "button");
    expect(more).toHaveAttribute("aria-expanded", "false");
    const controls = more.getAttribute("aria-controls");
    expect(document.getElementById(controls ?? "")).toContainElement(screen.getByText("uv isn't responding"));

    fireEvent.click(more);

    const fewer = screen.getByRole("button", { name: "Show Fewer" });
    expect(fewer).toHaveAttribute("type", "button");
    expect(fewer).toHaveAttribute("aria-expanded", "true");
    expect(fewer).toHaveAttribute("aria-controls", controls);
    const lines = document.getElementById(controls ?? "");
    expect(lines).toContainElement(screen.getByText("Homebrew is updating its software list"));
    expect(lines).toContainElement(screen.getByText("uv isn't responding"));
  });

  it("looks like a disclosure, not like the line's Details: muted words, then a 10pt triangle that turns down once the lines show", () => {
    renderWithProviders(<Folded notices={[brewUpdating, uvSilent]} />);

    expect(screen.getByRole("button", { name: "Details: uv isn't responding" })).toHaveClass("text-accent-text");
    const more = screen.getByRole("button", { name: "1 More Issue" });
    expect(more).toHaveClass("text-muted", "text-body");
    expect(more).not.toHaveClass("text-accent-text");
    // The words, then the triangle (spec §3.8): filled, 10 wide, pointing right.
    const triangle = more.lastElementChild as SVGElement;
    expect(triangle.tagName.toLowerCase()).toBe("svg");
    expect(triangle.getAttribute("width")).toBe("10");
    expect(triangle.querySelector("path")?.getAttribute("fill")).toBe("currentColor");
    expect(triangle).not.toHaveClass("rotate-90");

    fireEvent.click(more);

    const fewer = screen.getByRole("button", { name: "Show Fewer" });
    expect(fewer).toHaveClass("text-muted");
    expect(fewer).not.toHaveClass("text-accent-text");
    expect(fewer.querySelector("svg")).toHaveClass("rotate-90");
    // A line of its own as high as a notice's.
    expect(fewer.parentElement).toHaveClass("h-8");
  });

  it("gives the focus to the button that now says the other thing, so Enter folds straight back", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Folded notices={[brewUpdating, uvSilent, claudeUntested]} />);

    screen.getByRole("button", { name: "2 More Issues" }).focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "Show Fewer" })));

    await user.keyboard("{Enter}");
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "2 More Issues" })));
  });

  it("folds again when the number of lines changes, and only then", () => {
    const { rerender } = renderWithProviders(<Folded notices={[brewUpdating, uvSilent]} />);
    fireEvent.click(screen.getByRole("button", { name: "1 More Issue" }));

    // As many lines, other ones: still unfolded.
    rerender(<Folded notices={[brewUpdating, ollamaStopped]} />);
    expect(linesShown()).toEqual(["Homebrew is updating its software list", "Ollama isn't running"]);
    expect(screen.getByRole("button", { name: "Show Fewer" })).toBeInTheDocument();

    // One more: folded.
    rerender(<Folded notices={[brewUpdating, ollamaStopped, uvSilent]} />);
    expect(linesShown()).toEqual(["Ollama isn't running"]);
    expect(screen.getByRole("button", { name: "2 More Issues" })).toBeInTheDocument();

    // And back to two, still folded: the fold went for good.
    rerender(<Folded notices={[brewUpdating, ollamaStopped]} />);
    expect(linesShown()).toEqual(["Ollama isn't running"]);
    expect(screen.getByRole("button", { name: "1 More Issue" })).toBeInTheDocument();
  });

  it("keeps each line's Details and its own button working, folded or not", async () => {
    renderWithProviders(<Folded notices={[claudeUntested, ollamaStopped, brewStale]} />);

    // Folded: Ollama's line, the first warning.
    const ollamaDetails = screen.getByRole("button", { name: "Details: Ollama isn't running" });
    fireEvent.click(ollamaDetails);
    expect(document.getElementById(ollamaDetails.getAttribute("aria-controls") ?? "")).toHaveTextContent(
      "Open Ollama to see what it has and check for updates.",
    );
    fireEvent.click(screen.getByRole("button", { name: "Open Ollama" }));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("open_ollama_app"));

    fireEvent.click(screen.getByRole("button", { name: "2 More Issues" }));

    // Unfolded: the lines that were folded away, and Ollama's still.
    const claudeDetails = screen.getByRole("button", { name: "Details: Claude Code 2.1.290 not tested" });
    fireEvent.click(claudeDetails);
    expect(document.getElementById(claudeDetails.getAttribute("aria-controls") ?? "")).toHaveTextContent(
      "Not tested with this version of Claude Code yet.",
    );
    mockInvoke.mockClear();
    fireEvent.click(screen.getByRole("button", { name: "Check Again" }));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
    mockInvoke.mockClear();
    fireEvent.click(screen.getByRole("button", { name: "Open Ollama" }));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("open_ollama_app"));
  });

  it("draws one notice exactly as it would with no fold", () => {
    const folded = renderWithProviders(<Folded notices={[ollamaStopped]} />);
    const withFold = folded.container.innerHTML;
    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual(["Details", "Open Ollama"]);
    folded.unmount();

    const plain = renderWithProviders(<SourceNotices notices={[ollamaStopped]} layout="line" />);
    expect(plain.container.innerHTML).toBe(withFold);
  });

  it("says 还有N个问题 and 收起 in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(<Folded notices={[brewUpdating, uvSilent, claudeUntested]} />);

      fireEvent.click(screen.getByRole("button", { name: "还有2个问题" }));
      expect(screen.getByRole("button", { name: "收起" })).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});

describe("SourceNotices' Check again", () => {
  /** A snapshot of `generation`, checked a minute ago. */
  function snapshotAt(generation: number): Snapshot {
    return {
      generation,
      round: generation,
      detect: "Found",
      instances: [],
      artifacts: [],
      updates: [],
      refreshed_at: Math.floor(Date.now() / 1000) - 60,
      stale: false,
      errors: [],
    };
  }

  it("is the header's: off while a check runs, whoever started it, so none queues behind it", async () => {
    // Homebrew's "may be out of date" line sits on the Overview just under
    // the header, both buttons called Check again. Pressed while the
    // header's check ran, this one queued a second full check after it.
    let finish: (snapshot: Snapshot) => void = () => {};
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(snapshotAt(4));
      if (cmd === "refresh") {
        return new Promise<Snapshot>((resolve) => {
          finish = resolve;
        });
      }
      return Promise.resolve(undefined);
    });
    const refreshes = () => mockInvoke.mock.calls.filter(([cmd]) => cmd === "refresh").length;
    renderWithProviders(
      <>
        <CheckAgain />
        <SourceNotices notices={[brewStale]} layout="line" />
      </>,
    );
    const [header, notice] = screen.getAllByRole("button", { name: "Check Again" });
    await waitFor(() => expect(header.getAttribute("title")).toMatch(/Checked /));
    expect(notice).toBeEnabled();

    fireEvent.click(header);

    await waitFor(() => expect(notice).toBeDisabled());
    fireEvent.click(notice);
    await act(async () => {
      finish(snapshotAt(5));
    });
    await waitFor(() => expect(notice).toBeEnabled());
    expect(refreshes()).toBe(1);

    // Pressed itself, it runs the one check, the header's off with it.
    fireEvent.click(notice);
    await waitFor(() => expect(header).toBeDisabled());
    expect(notice).toBeDisabled();
    await act(async () => {
      finish(snapshotAt(6));
    });
    await waitFor(() => expect(notice).toBeEnabled());
    expect(refreshes()).toBe(2);
  });
});
