import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import type { SourceNoticeSpec } from "../lib/sources";
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
  action: { id: "retry", labelKey: "common.retry" },
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
    const more = screen.getByRole("button", { name: "2 more" });
    // Last in that line, after its own "Details".
    expect(screen.getByText("Homebrew is updating its software list").parentElement?.lastElementChild).toBe(more);
  });

  it("shows the first warning, ahead of information that comes before it", () => {
    renderWithProviders(<Folded notices={[brewUpdating, uvSilent, ollamaStopped, claudeUntested]} />);

    expect(linesShown()).toEqual(["uv isn't responding"]);
    expect(screen.getByRole("button", { name: "3 more" })).toBeInTheDocument();
  });

  it("shows every line in its order when pressed, and folds them again with Show fewer after the last", () => {
    renderWithProviders(<Folded notices={[brewUpdating, uvSilent, claudeUntested]} />);

    fireEvent.click(screen.getByRole("button", { name: "2 more" }));

    // In their order: the warning shown folded goes back to its place.
    expect(linesShown()).toEqual([
      "Homebrew is updating its software list",
      "uv isn't responding",
      "Claude Code 2.1.290 not tested",
    ]);
    expect(screen.queryByRole("button", { name: "2 more" })).toBeNull();
    const fewer = screen.getByRole("button", { name: "Show fewer" });
    expect(
      screen.getByText("Claude Code 2.1.290 not tested").compareDocumentPosition(fewer) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    fireEvent.click(fewer);

    expect(linesShown()).toEqual(["uv isn't responding"]);
    expect(screen.queryByRole("button", { name: "Show fewer" })).toBeNull();
    expect(screen.getByRole("button", { name: "2 more" })).toBeInTheDocument();
  });

  it("folds and unfolds with a real button that says whether the lines are shown, and which", () => {
    renderWithProviders(<Folded notices={[brewUpdating, uvSilent]} />);

    const more = screen.getByRole("button", { name: "1 more" });
    expect(more.tagName).toBe("BUTTON");
    expect(more).toHaveAttribute("type", "button");
    expect(more).toHaveAttribute("aria-expanded", "false");
    const controls = more.getAttribute("aria-controls");
    expect(document.getElementById(controls ?? "")).toContainElement(screen.getByText("uv isn't responding"));

    fireEvent.click(more);

    const fewer = screen.getByRole("button", { name: "Show fewer" });
    expect(fewer).toHaveAttribute("type", "button");
    expect(fewer).toHaveAttribute("aria-expanded", "true");
    expect(fewer).toHaveAttribute("aria-controls", controls);
    const lines = document.getElementById(controls ?? "");
    expect(lines).toContainElement(screen.getByText("Homebrew is updating its software list"));
    expect(lines).toContainElement(screen.getByText("uv isn't responding"));
  });

  it("gives the focus to the button that now says the other thing, so Enter folds straight back", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Folded notices={[brewUpdating, uvSilent, claudeUntested]} />);

    screen.getByRole("button", { name: "2 more" }).focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "Show fewer" })));

    await user.keyboard("{Enter}");
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "2 more" })));
  });

  it("folds again when the number of lines changes, and only then", () => {
    const { rerender } = renderWithProviders(<Folded notices={[brewUpdating, uvSilent]} />);
    fireEvent.click(screen.getByRole("button", { name: "1 more" }));

    // As many lines, other ones: still unfolded.
    rerender(<Folded notices={[brewUpdating, ollamaStopped]} />);
    expect(linesShown()).toEqual(["Homebrew is updating its software list", "Ollama isn't running"]);
    expect(screen.getByRole("button", { name: "Show fewer" })).toBeInTheDocument();

    // One more: folded.
    rerender(<Folded notices={[brewUpdating, ollamaStopped, uvSilent]} />);
    expect(linesShown()).toEqual(["Ollama isn't running"]);
    expect(screen.getByRole("button", { name: "2 more" })).toBeInTheDocument();

    // And back to two, still folded: the fold went for good.
    rerender(<Folded notices={[brewUpdating, ollamaStopped]} />);
    expect(linesShown()).toEqual(["Ollama isn't running"]);
    expect(screen.getByRole("button", { name: "1 more" })).toBeInTheDocument();
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

    fireEvent.click(screen.getByRole("button", { name: "2 more" }));

    // Unfolded: the lines that were folded away, and Ollama's still.
    const claudeDetails = screen.getByRole("button", { name: "Details: Claude Code 2.1.290 not tested" });
    fireEvent.click(claudeDetails);
    expect(document.getElementById(claudeDetails.getAttribute("aria-controls") ?? "")).toHaveTextContent(
      "Canager hasn't tested this version.",
    );
    mockInvoke.mockClear();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
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

  it("says 还有 N 条 and 收起 in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(<Folded notices={[brewUpdating, uvSilent, claudeUntested]} />);

      fireEvent.click(screen.getByRole("button", { name: "还有 2 条" }));
      expect(screen.getByRole("button", { name: "收起" })).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});
