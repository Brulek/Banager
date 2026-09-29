import type { MouseEvent } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { RowAction, ToolRow } from "./ToolRow";
import type { ArtifactKey } from "../lib/types";

/** The classes a class list holds with no state prefix: how it looks at rest. */
function atRest(className: string): string[] {
  return className.split(/\s+/).filter((name) => name !== "" && !name.includes(":"));
}

describe("RowAction", () => {
  it("is a regular grey button, whichever action it is: nothing in accent, nothing red", () => {
    // What the page gets: the button itself, to give the focus back to.
    const pressed: Array<EventTarget | null> = [];
    const onClick = vi.fn((event: MouseEvent<HTMLButtonElement>) => pressed.push(event.currentTarget));
    const { getByRole } = renderWithProviders(
      <>
        <RowAction onClick={() => {}}>Update</RowAction>
        <RowAction onClick={onClick}>Uninstall</RowAction>
      </>,
    );

    for (const name of ["Update", "Uninstall"]) {
      const button = getByRole("button", { name });
      // 24 high, the fill and the label colour at rest, a darker fill
      // while pressed.
      const rest = atRest(button.className);
      expect(rest).toEqual(expect.arrayContaining(["h-6", "rounded-control", "bg-fill", "text-foreground"]));
      expect(button.className).toMatch(/\bactive:bg-fill-pressed\b/);
      // No accent and no red, at rest or in any state; nothing under the
      // pointer at all.
      expect(button.className).not.toMatch(/danger|accent/);
      expect(button.className).not.toMatch(/\bhover:/);
    }

    const uninstall = getByRole("button", { name: "Uninstall" });
    fireEvent.click(uninstall);
    expect(onClick).toHaveBeenCalledTimes(1);
    expect(pressed).toEqual([uninstall]);
  });
});

describe("ToolRow", () => {
  it("shows the source's avatar, the name with its source chip, and one line about it", () => {
    const { container, getByText } = renderWithProviders(
      <ToolRow
        adapterId="brew"
        sourceLabel="Homebrew"
        name="jq"
        nameChip="Homebrew"
        description="Lightweight and flexible command-line JSON processor"
      />,
    );

    const row = container.querySelector("[data-tool-row]") as HTMLElement;
    const avatar = row.querySelector('[aria-hidden="true"]');
    expect(avatar).toHaveTextContent("H");
    expect(avatar?.className).toContain("bg-source-homebrew");
    // 32px: the size a row's avatar is.
    expect(avatar?.className).toContain("h-8");
    expect(getByText("jq").tagName).toBe("P");
    expect(getByText("Homebrew", { selector: "span" })).toBeInTheDocument();
    // One line, cut off at the end, with the whole sentence on hover.
    const blurb = getByText("Lightweight and flexible command-line JSON processor");
    expect(blurb.className).toContain("truncate");
    expect(blurb).toHaveAttribute("title", "Lightweight and flexible command-line JSON processor");
  });

  it("lets its description be selected only where asked, for a path to copy", () => {
    const { container, getByText, rerender } = renderWithProviders(
      <ToolRow adapterId="brew" sourceLabel="Homebrew" name="jq" description="Lightweight JSON processor" />,
    );
    // A tool's own blurb, like its name, is the row's words.
    expect(container.querySelectorAll(".select-text")).toHaveLength(0);

    rerender(
      <ToolRow avatar={<span />} name="helper-cli" description="/usr/local/bin/helper-cli" selectableDescription />,
    );
    expect(getByText("/usr/local/bin/helper-cli")).toHaveClass("select-text");
    expect(getByText("helper-cli")).not.toHaveClass("select-text");
  });

  it("puts a note after its description, on its line, that the description gives way to and never selects with it", () => {
    const { container, getByText } = renderWithProviders(
      <ToolRow
        avatar={<span />}
        name="docker"
        description="/usr/local/bin/docker"
        selectableDescription
        descriptionNote="Points into Docker.app"
      />,
    );

    const path = getByText("/usr/local/bin/docker");
    const note = getByText("Points into Docker.app");
    expect(note.closest("p")).toBe(path.closest("p"));
    expect(path.compareDocumentPosition(note) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // The path is cut short first, its whole text in its tooltip; the note
    // does not shrink, short of a row too narrow for it alone.
    expect(path).toHaveAttribute("title", "/usr/local/bin/docker");
    expect(atRest(path.className)).toEqual(expect.arrayContaining(["min-w-0", "truncate"]));
    expect(atRest(note.className)).toEqual(expect.arrayContaining(["shrink-0", "max-w-full", "truncate"]));
    // Only the path selects; space sets the note apart, with no dot between them.
    expect([...container.querySelectorAll(".select-text")]).toEqual([path]);
    expect(note.previousElementSibling).toBe(path);
    expect(path.closest("p")?.textContent).not.toContain("·");
  });

  it("ticks its checkbox through onToggle, named for what it selects", () => {
    const onToggle = vi.fn();
    const { getByRole } = renderWithProviders(
      <ToolRow
        adapterId="brew"
        sourceLabel="Homebrew"
        name="glib"
        description="Core application library for C"
        selectable={{ checked: false, onToggle, ariaLabel: "Select glib for update" }}
      />,
    );

    const checkbox = getByRole("checkbox", { name: "Select glib for update" });
    expect(checkbox).not.toBeChecked();
    fireEvent.click(checkbox);
    expect(onToggle).toHaveBeenCalledTimes(1);
  });

  it("draws a column for every slot it is given, even an empty one, and none for a slot left out", () => {
    const { container, getByRole, getByText, rerender } = renderWithProviders(
      <ToolRow
        adapterId="brew"
        sourceLabel="Homebrew"
        name="glib"
        description="Core application library for C"
        status={<span>Pinned</span>}
        version="2.88.3 → 2.90.0"
        action={<button type="button">Update</button>}
        menu={<button type="button">More</button>}
      />,
    );
    const row = () => container.querySelector("[data-tool-row]") as HTMLElement;
    const columns = () => row().children.length;

    expect(getByText("Pinned")).toBeInTheDocument();
    expect(getByText("2.88.3 → 2.90.0").className).toContain("tabular-nums");
    expect(getByRole("button", { name: "Update" })).toBeInTheDocument();
    expect(getByRole("button", { name: "More" })).toBeInTheDocument();
    const withEverything = columns();

    // `null` keeps the action's column, so this row's version lines up
    // with the rows above that have a button there.
    rerender(
      <ToolRow
        adapterId="brew"
        sourceLabel="Homebrew"
        name="glib"
        description="Core application library for C"
        status={<span>Pinned</span>}
        version="2.88.3 → 2.90.0"
        action={null}
        menu={<button type="button">More</button>}
      />,
    );
    expect(columns()).toBe(withEverything);

    // Left out: no column at all.
    rerender(<ToolRow adapterId="brew" sourceLabel="Homebrew" name="glib" description="Core application library for C" />);
    expect(columns()).toBe(withEverything - 4);
  });

  it("is a button itself when it opens something, under its own controls, and takes the focus when pressed", () => {
    const onOpen = vi.fn();
    const onUninstall = vi.fn();
    const { getByRole } = renderWithProviders(
      <ToolRow
        adapterId="brew"
        sourceLabel="Homebrew"
        name="jq"
        description="Lightweight and flexible command-line JSON processor"
        status={<span>Pinned</span>}
        action={
          <button type="button" onClick={onUninstall}>
            Uninstall
          </button>
        }
        onOpen={onOpen}
        openLabel="Details: jq"
      />,
    );

    const open = getByRole("button", { name: "Details: jq" });
    // First in the row, so Tab reaches it before the row's own buttons,
    // and under them: they sit on a layer above it.
    expect(open.parentElement?.firstElementChild).toBe(open);
    expect(getByRole("button", { name: "Uninstall" }).parentElement?.className).toContain("z-10");
    fireEvent.click(open);
    expect(onOpen).toHaveBeenCalledTimes(1);
    // WebKit leaves a clicked button unfocused; this one takes the focus,
    // so what it opens can give it back.
    expect(document.activeElement).toBe(open);

    fireEvent.click(getByRole("button", { name: "Uninstall" }));
    expect(onUninstall).toHaveBeenCalledTimes(1);
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("lets the source's chip give way to the name on a narrow row, and still says it to a screen reader", () => {
    const { container, getByText } = renderWithProviders(
      <ToolRow adapterId="brew" sourceLabel="Homebrew" name="jq" nameChip="Homebrew" description="A JSON processor" />,
    );
    // A container query: the row measures itself, not the window.
    expect((container.querySelector("[data-tool-row]") as HTMLElement).className).toContain("@container");
    // Visually hidden, not removed: the avatar is aria-hidden, so the chip
    // is all a screen reader has of the source.
    expect(getByText("Homebrew", { selector: "span" }).className).toContain("@max-2xl:sr-only");
    expect(getByText("Homebrew", { selector: "span" }).className).not.toContain("hidden");
  });

  it("draws an avatar of its own in place of a source's, for a row that belongs to no source", () => {
    const { container } = renderWithProviders(
      <ToolRow avatar={<span data-testid="own-avatar" />} name="sync-photos" description="~/bin/sync-photos" />,
    );

    const row = container.querySelector("[data-tool-row]") as HTMLElement;
    expect(row.querySelector('[data-testid="own-avatar"]')).not.toBeNull();
    // No source's letter or colour beside it.
    expect(row.querySelector('[class*="bg-source-"]')).toBeNull();
  });
});

describe("ToolRow's app icon", () => {
  const iterm: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "iterm2" };
  const jq: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" };
  const ICON = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAwS2OUAAAAABJRU5ErkJggg==";
  const mockInvoke = vi.mocked(invoke);

  beforeEach(() => {
    mockInvoke.mockReset();
  });

  /** The row's avatar: the first thing in it hidden from a screen reader. */
  function avatarOf(container: HTMLElement): Element {
    return (container.querySelector("[data-tool-row]") as HTMLElement).querySelector('[aria-hidden="true"]') as Element;
  }

  it("shows a cask's own app icon once it arrives, and the source's letter until then", async () => {
    let answer: (icon: string | null) => void = () => {};
    mockInvoke.mockImplementation(
      (cmd: string) =>
        new Promise((resolve) => {
          if (cmd === "artifact_icon") answer = resolve;
        }),
    );
    const { container } = renderWithProviders(
      <ToolRow adapterId="brew" sourceLabel="Homebrew" iconKey={iterm} name="iTerm2" description="Terminal emulator" />,
    );

    // Asked, and not here yet: the coloured initial stands in.
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("artifact_icon", { key: iterm }));
    expect(avatarOf(container)).toHaveTextContent("H");
    expect(avatarOf(container).className).toContain("bg-source-homebrew");

    await act(async () => answer(ICON));

    await waitFor(() => expect(avatarOf(container).querySelector("img[data-app-icon]")).not.toBeNull());
    const icon = avatarOf(container).querySelector("img[data-app-icon]") as Element;
    expect(icon).toHaveAttribute("src", ICON);
    expect(icon).toHaveAttribute("alt", "");
    // A row avatar's size, rounded like an app icon, with nothing coloured behind it.
    expect(icon.className).toContain("h-8");
    expect(icon.className).toContain("rounded-[7px]");
    expect(icon.className).not.toMatch(/\bbg-/);
    // The source's letter moves to the icon's corner, 14px.
    const badge = avatarOf(container).querySelector("[data-source-badge]");
    expect(badge).toHaveTextContent("H");
    expect(badge?.firstElementChild?.className).toContain("h-3.5");
  });

  it("keeps the source's letter for a cask that has no icon", async () => {
    mockInvoke.mockImplementation((cmd: string) => Promise.resolve(cmd === "artifact_icon" ? null : undefined));
    const { container, queryClient } = renderWithProviders(
      <ToolRow adapterId="brew" sourceLabel="Homebrew" iconKey={iterm} name="iTerm2" description="Terminal emulator" />,
    );

    await waitFor(() => expect(queryClient.getQueryState(["artifactIcon", "brew:/opt/homebrew", "Cask", "iterm2"])?.status).toBe("success"));
    expect(avatarOf(container).tagName).toBe("SPAN");
    expect(avatarOf(container)).toHaveTextContent("H");
  });

  it("asks nothing for a row that is not a cask, or that names no tool", () => {
    mockInvoke.mockResolvedValue(ICON);
    const formula = renderWithProviders(
      <ToolRow adapterId="brew" sourceLabel="Homebrew" iconKey={jq} name="jq" description="JSON processor" />,
    );
    expect(avatarOf(formula.container)).toHaveTextContent("H");
    formula.unmount();
    renderWithProviders(<ToolRow adapterId="brew" sourceLabel="Homebrew" name="jq" description="JSON processor" />);

    expect(mockInvoke).not.toHaveBeenCalled();
  });
});
