import type { MouseEvent } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, getByText, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { MIDDLE_CUT_FROM, ROW_FIT_WIDTHS, RowAction, rowFitFor, ToolRow } from "./ToolRow";
import { StatusChip } from "./StatusChip";
import { Menu } from "./ui/Menu";
import { ListWidthProvider } from "./VirtualList";
import { RovingRowProvider } from "./rovingRows";
import type { ArtifactKey } from "../lib/types";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

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
  it("shows the source's avatar, the name, and one line about it: no pill, the source in the avatar's tooltip and for a screen reader", () => {
    const { container, getByText } = renderWithProviders(
      <ToolRow
        adapterId="brew"
        sourceLabel="Homebrew"
        name="jq"
        description="Lightweight and flexible command-line JSON processor"
      />,
    );

    const row = container.querySelector("[data-tool-row]") as HTMLElement;
    const avatar = row.querySelector('[aria-hidden="true"]');
    expect(avatar).toHaveTextContent("H");
    expect(avatar?.className).toContain("bg-source-homebrew");
    // 32px: the size a row's avatar is.
    expect(avatar?.className).toContain("h-8");
    // The source's name over the avatar, on a layer above the row.
    expect(avatar?.parentElement).toHaveAttribute("title", "Homebrew");
    expect(avatar?.parentElement?.className).toContain("z-10");
    // The name: 13, semibold.
    const name = getByText("jq");
    expect(name.tagName).toBe("P");
    expect(atRest(name.className)).toEqual(expect.arrayContaining(["text-name", "font-semibold", "truncate"]));
    // The source said after it, to a screen reader only: no pill, no outline.
    const source = getByText("Homebrew", { selector: "span" });
    expect(source.className).toBe("sr-only");
    expect(row.querySelector('[class*="rounded-full"], [class*="border"]')).toBeNull();
    // One line, 11 muted, cut off at the end, with the whole sentence on hover.
    const blurb = getByText("Lightweight and flexible command-line JSON processor");
    expect(blurb.className).toContain("truncate");
    expect(blurb.parentElement?.className).toContain("text-small");
    expect(blurb.parentElement?.className).toContain("text-muted");
    expect(blurb).toHaveAttribute("title", "Lightweight and flexible command-line JSON processor");
  });

  it("says the source after the name, small and muted, where the list has the name under two sources (R3)", () => {
    const { getByText } = renderWithProviders(
      <ToolRow adapterId="pipx" sourceLabel="pipx" name="black" showSource description="Python code formatter" />,
    );
    const source = getByText("pipx", { selector: "span" });
    expect(atRest(source.className)).toEqual(expect.arrayContaining(["text-small", "text-muted", "shrink-0"]));
    expect(source.className).not.toContain("sr-only");
    expect(source.className).not.toMatch(/rounded|border|bg-/);
    // Right after the name, on its line.
    expect(source.previousElementSibling).toBe(getByText("black"));
  });

  it("says nothing more for a tool that is its own source", () => {
    const { container } = renderWithProviders(
      <ToolRow adapterId="standalone-claude" sourceLabel="Claude Code" name="Claude Code" showSource description="Anthropic's coding assistant" />,
    );
    expect(container.querySelectorAll(".sr-only")).toHaveLength(0);
    expect(container.textContent).toBe("CClaude CodeAnthropic's coding assistant");
  });

  it("is laid out as a Mac list's row: 52 high, 20 in, the avatar 12 after the box, the text 12 after that, the columns 16 apart", () => {
    const { container, getByRole } = renderWithProviders(
      <ToolRow
        adapterId="brew"
        sourceLabel="Homebrew"
        name="glib"
        description="Core application library for C"
        selectable={{ checked: false, onToggle: () => {}, ariaLabel: "Select glib for update" }}
        status={<StatusChip label="Pinned" />}
        version="2.88.3 → 2.90.0"
        action={<button type="button">Update</button>}
        menu={<button type="button">More</button>}
      />,
    );
    const row = container.querySelector("[data-tool-row]") as HTMLElement;
    const rest = atRest(row.className);
    expect(rest).toEqual(expect.arrayContaining(["h-13", "px-5", "items-center"]));
    // No corners, no fill, nothing under the pointer.
    expect(row.className).not.toMatch(/rounded|\bbg-|hover:/);
    expect(getByRole("checkbox").parentElement?.className).toContain("mr-3");
    expect(container.querySelector('[title="Homebrew"]')?.nextElementSibling?.className).toContain("ml-3");
    // The status, version, action and ⋯ columns, 16 apart; the action 80
    // wide and the ⋯ 24.
    const status = row.querySelector("[data-status]") as HTMLElement;
    expect(status.className).toContain("ml-4");
    const version = container.querySelector(".tabular-nums") as HTMLElement;
    expect(atRest(version.className)).toEqual(expect.arrayContaining(["ml-4", "min-w-16", "text-body", "text-muted"]));
    expect(getByRole("button", { name: "Update" }).parentElement?.className).toMatch(/\bml-4\b.*\bmin-w-20\b.*\bjustify-end\b/);
    expect(getByRole("button", { name: "More" }).parentElement?.className).toMatch(/\bml-4\b.*\bw-6\b/);
    // The hairline from where the text starts (20 + 16 + 12 + 32 + 12 =
    // 92) to 20 from the right; 64 in without a checkbox.
    const hairline = row.querySelector("[data-row-separator]") as HTMLElement;
    expect(atRest(hairline.className)).toEqual(expect.arrayContaining(["left-[5.75rem]", "right-5", "h-px", "bg-separator"]));
  });

  it("keeps a checkbox's room for a row with none, so the avatars stay in one column", () => {
    const { container } = renderWithProviders(
      <ToolRow adapterId="brew" sourceLabel="Homebrew" name="glib" description="C library" selectable={null} />,
    );
    const row = container.querySelector("[data-tool-row]") as HTMLElement;
    expect(row.querySelector("input")).toBeNull();
    const slot = row.querySelector(".w-4") as HTMLElement;
    expect(slot).not.toBeNull();
    expect(slot.childElementCount).toBe(0);
    expect(row.querySelector("[data-row-separator]")?.className).toContain("left-[5.75rem]");
  });

  it("cuts a very long name in its middle, keeping its end, and says it whole in its tooltip", () => {
    const long = "modelscope.cn/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M";
    expect(long.length).toBeGreaterThan(MIDDLE_CUT_FROM);
    const { container } = renderWithProviders(
      <ToolRow adapterId="ollama" sourceLabel="Ollama" name={long} description="Ollama model" />,
    );
    const name = container.querySelector("p[title]") as HTMLElement;
    expect(name).toHaveAttribute("title", long);
    // The two halves, one after the other, read as the whole name.
    expect(name.textContent).toBe(long);
    const [head, tail] = [...name.children] as HTMLElement[];
    // The head gives way, cut at its end; the last 12 characters stay whole.
    expect(atRest(head.className)).toEqual(expect.arrayContaining(["min-w-0", "truncate"]));
    expect(tail.textContent).toBe("-GGUF:Q4_K_M");
    expect(atRest(tail.className)).toContain("shrink-0");

    // A shorter name is one piece, cut at its end.
    const short = renderWithProviders(
      <ToolRow adapterId="brew" sourceLabel="Homebrew" name="Microsoft Visual Studio Code" description="Editor" />,
    );
    const whole = short.container.querySelector("p[title]") as HTMLElement;
    expect(whole.childElementCount).toBe(0);
    expect(whole.className).toContain("truncate");
  });

  it("marks a name in Latin letters as English, so a Chinese window cuts it with the system font's …", () => {
    const { container } = renderWithProviders(
      <>
        <ToolRow adapterId="brew" sourceLabel="Homebrew" name="Android SDK Platform-Tools" description="SDK" />
        <ToolRow adapterId="brew" sourceLabel="Homebrew" name="modelscope.cn/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF" description="Model" />
        <ToolRow avatar={<span />} name="微信开发者工具" description="WeChat DevTools" />
      </>,
    );
    const names = [...container.querySelectorAll("p[title]")];
    expect(names.map((name) => name.getAttribute("lang"))).toEqual(["en", "en", null]);
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
    expect(note.parentElement).toBe(path.parentElement);
    expect(path.compareDocumentPosition(note) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // The path is cut short first, its whole text in its tooltip; the note
    // does not shrink, short of a row too narrow for it alone.
    expect(path).toHaveAttribute("title", "/usr/local/bin/docker");
    expect(atRest(path.className)).toEqual(expect.arrayContaining(["min-w-0", "truncate"]));
    expect(atRest(note.className)).toEqual(expect.arrayContaining(["shrink-0", "max-w-full", "truncate"]));
    // Only the path selects; space sets the note apart, with no dot between them.
    expect([...container.querySelectorAll(".select-text")]).toEqual([path]);
    expect(note.previousElementSibling).toBe(path);
    expect(path.parentElement?.textContent).not.toContain("·");
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

    // The avatar sits over the row's button, for its tooltip, and opens
    // the row all the same, the focus on the row's button.
    open.blur();
    fireEvent.click(getByRole("button", { name: "Details: jq" }).parentElement?.querySelector('[title="Homebrew"]') as HTMLElement);
    expect(onOpen).toHaveBeenCalledTimes(2);
    expect(document.activeElement).toBe(open);
  });

  describe("in a narrow window (R9)", () => {
    const row = (width: number | null) => (
      <ListWidthProvider value={width}>
        <ToolRow
          adapterId="brew"
          sourceLabel="Homebrew"
          name="claude-code"
          description="Anthropic's coding assistant"
          status={<StatusChip label="Updates itself" />}
          version="2.1.282 → 2.1.290"
          newVersion="2.1.290"
          action={<button type="button">Update</button>}
        />
      </ListWidthProvider>
    );

    it("gives way column by column as the list narrows: the version first, then the status word's column", () => {
      expect(rowFitFor(null)).toBe("full");
      expect(rowFitFor(752)).toBe("full");
      expect(rowFitFor(ROW_FIT_WIDTHS.full)).toBe("full");
      expect(rowFitFor(ROW_FIT_WIDTHS.full - 1)).toBe("compact");
      expect(rowFitFor(ROW_FIT_WIDTHS.compact)).toBe("compact");
      expect(rowFitFor(ROW_FIT_WIDTHS.compact - 1)).toBe("narrow");
      expect(rowFitFor(592)).toBe("narrow");
      expect(rowFitFor(ROW_FIT_WIDTHS.narrow)).toBe("narrow");
      // Beside the Installed page's inspector in a window at its default 960.
      expect(rowFitFor(ROW_FIT_WIDTHS.narrow - 1)).toBe("minimal");
      expect(rowFitFor(451)).toBe("minimal");
    });

    it("shows the whole version change, and the status word in its column, with room for everything", () => {
      const { container, getByText } = renderWithProviders(row(752));
      expect(container.querySelector(".tabular-nums")?.textContent).toBe("2.1.282 → 2.1.290");
      const status = getByText("Updates itself").closest("[data-status]") as HTMLElement;
      expect(status.className).toContain("ml-4");
    });

    it("says only the new version first, the whole change still to a screen reader", () => {
      const { container, getByText } = renderWithProviders(row(ROW_FIT_WIDTHS.compact));
      const version = container.querySelector(".tabular-nums") as HTMLElement;
      const [shown, spoken] = [...version.children] as HTMLElement[];
      expect(shown.textContent).toBe("2.1.290");
      expect(shown).toHaveAttribute("aria-hidden", "true");
      expect(spoken.textContent).toBe("2.1.282 → 2.1.290");
      expect(spoken.className).toBe("sr-only");
      // The status word keeps its column still.
      expect(getByText("Updates itself").closest("[data-status]")?.className).toContain("ml-4");
    });

    it("then moves the status word to the start of the description's line; the name and the button stay whole", () => {
      const { container, getByText, getByRole } = renderWithProviders(row(592));
      const status = getByText("Updates itself").closest("[data-status]") as HTMLElement;
      const blurb = getByText("Anthropic's coding assistant");
      expect(status.parentElement).toBe(blurb.parentElement);
      expect(status.nextElementSibling).toBe(blurb);
      expect(container.querySelectorAll("[data-status]")).toHaveLength(1);
      // The description gives way; the name and the button never shrink
      // below themselves (the button's column is fixed, the name's line
      // loses its description first).
      expect(blurb.className).toContain("truncate");
      expect(getByRole("button", { name: "Update" }).parentElement?.className).toContain("shrink-0");
      expect(container.querySelector(".tabular-nums")?.textContent).toBe("2.1.2902.1.282 → 2.1.290");
    });

    it("beside the inspector, gives up the version column: an update's change goes after the status word, a plain version goes", () => {
      const { container, getByText, getByRole } = renderWithProviders(row(451));
      const blurb = getByText("Anthropic's coding assistant");
      const version = container.querySelector("[data-version]") as HTMLElement;
      expect(container.querySelectorAll("[data-version]")).toHaveLength(1);
      // On the description's line: the word, the change, the description.
      expect(version.parentElement).toBe(blurb.parentElement);
      expect(version.previousElementSibling).toHaveAttribute("data-status");
      expect(version.nextElementSibling).toBe(blurb);
      expect(version.textContent).toBe("2.1.282 → 2.1.290");
      expect(version.className.split(" ")).toEqual(expect.arrayContaining(["shrink-0", "whitespace-nowrap", "tabular-nums"]));
      expect(getByRole("button", { name: "Update" })).toBeInTheDocument();

      // No update: no version anywhere on the row -- the inspector says it.
      const plain = renderWithProviders(
        <ListWidthProvider value={451}>
          <ToolRow adapterId="brew" sourceLabel="Homebrew" name="jq" description="JSON processor" version="1.8.2" />
        </ListWidthProvider>,
      );
      expect(plain.container.querySelector("[data-version]")).toBeNull();
      expect(plain.container.textContent).not.toContain("1.8.2");
    });
  });

  it("opens its ⋯ menu at the pointer on a right-click anywhere on it", () => {
    const { container, getByRole, queryByRole } = renderWithProviders(
      <ToolRow
        adapterId="brew"
        sourceLabel="Homebrew"
        name="glib"
        description="Core application library for C"
        menu={<Menu label="More actions for glib" items={[{ id: "skip", label: "Skip This Version", onSelect: vi.fn() }]} />}
      />,
    );
    const row = container.querySelector("[data-tool-row]") as HTMLElement;
    const trigger = getByRole("button", { name: "More actions for glib" });
    // The ⋯ is always there, quiet: tertiary at rest, muted under the
    // pointer or the focus, never a fill.
    expect(atRest(trigger.className)).toEqual(expect.arrayContaining(["text-tertiary", "h-6", "w-6"]));
    expect(trigger.className).toMatch(/group-hover\/row:text-muted/);
    expect(trigger.className).toMatch(/group-focus-within\/row:text-muted/);
    expect(trigger.className).not.toMatch(/\bbg-/);
    expect(row.className).toContain("group/row");

    const wrapper = trigger.parentElement as HTMLElement;
    vi.spyOn(wrapper, "getBoundingClientRect").mockReturnValue(new DOMRect(700, 300, 24, 24));
    const event = fireEvent.contextMenu(getByText(container, "Core application library for C"), { clientX: 240, clientY: 310 });
    // The web view's own menu does not show.
    expect(event).toBe(false);

    const menu = getByRole("menu", { name: "More actions for glib" });
    // Its corner at the pointer: measured from the ⋯ button's box.
    expect(menu.style.left).toBe("-460px");
    expect(menu.style.top).toBe("10px");
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    // No item picked until the pointer or an arrow key picks one.
    expect(document.activeElement).toBe(menu);
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement).toBe(getByRole("menuitem", { name: "Skip This Version" }));
    fireEvent.keyDown(document.activeElement as HTMLElement, { key: "Escape" });
    expect(queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(trigger);

    // The ⋯ itself still opens it under the button.
    fireEvent.click(trigger);
    expect(getByRole("menu").style.left).toBe("");
  });

  it("leaves a right-click alone on a row with no menu", () => {
    const { container } = renderWithProviders(
      <ToolRow adapterId="brew" sourceLabel="Homebrew" name="glib" description="Core application library for C" />,
    );
    const event = fireEvent.contextMenu(container.querySelector("[data-tool-row]") as HTMLElement);
    expect(event).toBe(true);
  });

  it("takes the focus itself in a list with arrow keys: Space ticks its box, Enter does nothing", () => {
    const onToggle = vi.fn();
    const onFocus = vi.fn();
    const onUpdate = vi.fn();
    const { container, getByRole } = renderWithProviders(
      <RovingRowProvider value={{ tabIndex: 0, onFocus }}>
        <ToolRow
          adapterId="brew"
          sourceLabel="Homebrew"
          name="glib"
          description="Core application library for C"
          selectable={{ checked: false, onToggle, ariaLabel: "Select glib for update" }}
          action={
            <button type="button" onClick={onUpdate}>
              Update
            </button>
          }
        />
      </RovingRowProvider>,
    );
    const row = container.querySelector("[data-tool-row]") as HTMLElement;
    expect(row).toHaveAttribute("tabindex", "0");
    expect(row).toHaveAttribute("data-row-focus");
    // Its focus ring just inside it, where the list's edge cannot clip it.
    expect(row.className).toContain("-outline-offset-3");

    row.focus();
    expect(onFocus).toHaveBeenCalled();
    expect(fireEvent.keyDown(row, { key: " " })).toBe(false);
    expect(onToggle).toHaveBeenCalledTimes(1);
    expect(fireEvent.keyDown(row, { key: "Enter" })).toBe(false);
    expect(onUpdate).not.toHaveBeenCalled();
    expect(onToggle).toHaveBeenCalledTimes(1);

    // Space on its own checkbox is the checkbox's, not the row's.
    fireEvent.keyDown(getByRole("checkbox"), { key: " " });
    expect(onToggle).toHaveBeenCalledTimes(1);
  });

  describe("selected (the Installed page's inspector, R11)", () => {
    const selectedRow = (selected: boolean, roving = false) => {
      const content = (
        <ToolRow
          adapterId="brew"
          sourceLabel="Homebrew"
          name="jq"
          description="JSON processor"
          onOpen={vi.fn()}
          openLabel="Details: jq"
          selected={selected}
        />
      );
      return roving ? <RovingRowProvider value={{ tabIndex: 0, onFocus: vi.fn() }}>{content}</RovingRowProvider> : content;
    };

    it("fills the row 10 in from either side with a control's corners, and says it is pressed", () => {
      const { container, getByRole } = renderWithProviders(selectedRow(true));
      const row = container.querySelector("[data-tool-row]") as HTMLElement;
      expect(row).toHaveAttribute("data-selected");
      const fill = row.querySelector("[data-row-selection]") as HTMLElement;
      expect(fill).toHaveAttribute("aria-hidden", "true");
      expect(fill.className.split(" ")).toEqual(
        expect.arrayContaining(["absolute", "inset-y-0", "left-2.5", "right-2.5", "rounded-control", "pointer-events-none"]),
      );
      // Under the row's words, not over them: the row is its own stacking
      // context, and the fill sits at its bottom.
      expect(fill.className).toContain("-z-10");
      expect(row.className.split(" ")).toContain("isolate");
      // Under everything else on the row.
      expect(row.firstElementChild).toBe(fill);
      expect(getByRole("button", { name: "Details: jq" })).toHaveAttribute("aria-pressed", "true");
    });

    it("has no fill while it is not selected", () => {
      const { container, getByRole } = renderWithProviders(selectedRow(false));
      expect(container.querySelector("[data-tool-row]")).not.toHaveAttribute("data-selected");
      expect(container.querySelector("[data-row-selection]")).toBeNull();
      expect(getByRole("button", { name: "Details: jq" })).toHaveAttribute("aria-pressed", "false");
    });

    it("is filled in the accent with white words while its list has the focus, grey while it has not, with no hairline beside it", () => {
      const css = readFileSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../index.css"), "utf-8")
        .replace(/\/\*[\s\S]*?\*\//g, "")
        .replace(/\s+/g, " ");
      expect(css).toContain("[data-row-selection] { background-color: var(--color-row-selected); }");
      expect(css).toContain("[data-list]:focus-within [data-row-selection] { background-color: var(--color-accent); }");
      // Every word on it white -- but a panel's opened from it, which
      // keeps the window's colours.
      expect(css).toContain(
        "[data-list]:focus-within [data-tool-row][data-selected] :is(.text-foreground, .text-muted, .text-tertiary):not([data-popup-open] > :not(button), [data-popup-open] > :not(button) *) { color: #fff; }",
      );
      expect(css).toContain(
        "[data-tool-row][data-selected] [data-row-separator], [data-list-slot]:has(+ [data-list-slot] [data-tool-row][data-selected]) [data-row-separator] { display: none; }",
      );
      expect(css).not.toMatch(/:has\([^)]*:has\(/);
    });

    it("in a list with arrow keys, leaves Tab to the row and opens it with Space, having no checkbox", () => {
      const onOpen = vi.fn();
      const { container, getByRole } = renderWithProviders(
        <RovingRowProvider value={{ tabIndex: 0, onFocus: vi.fn() }}>
          <ToolRow adapterId="brew" sourceLabel="Homebrew" name="jq" description="JSON processor" onOpen={onOpen} openLabel="Details: jq" />
        </RovingRowProvider>,
      );
      const row = container.querySelector("[data-tool-row]") as HTMLElement;
      // The pointer's button is out of the Tab order: the row is what Tab and ↑ ↓ reach.
      expect(getByRole("button", { name: "Details: jq" })).toHaveAttribute("tabindex", "-1");
      expect(row).toHaveAttribute("tabindex", "0");
      expect(fireEvent.keyDown(row, { key: " " })).toBe(false);
      expect(onOpen).toHaveBeenCalledTimes(1);
      fireEvent.keyDown(row, { key: "Enter" });
      expect(onOpen).toHaveBeenCalledTimes(1);
    });

    it("keeps its button in the Tab order in a list without arrow keys", () => {
      const { getByRole } = renderWithProviders(selectedRow(false));
      expect(getByRole("button", { name: "Details: jq" })).not.toHaveAttribute("tabindex");
    });
  });

  it("has no hairline under a list's last row, nor under the last of a run a line of another kind follows", () => {
    // index.css: the rule that hides it, as one rule the browser keeps --
    // a `:has()` inside a `:has()` would have it dropped whole.
    const css = readFileSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../index.css"), "utf-8").replace(/\/\*[\s\S]*?\*\//g, "");
    const rule = /([^{}]*data-row-separator[^{}]*)\{\s*display:\s*none;?\s*\}/.exec(css);
    expect(rule).not.toBeNull();
    const selectors = (rule?.[1] ?? "").split(",").map((selector) => selector.trim());
    expect(selectors).toEqual([
      "[data-list-slot]:last-child [data-row-separator]",
      "[data-list-slot]:has(+ [data-list-slot] > :not([data-tool-row])) [data-row-separator]",
    ]);
    for (const selector of selectors) expect(selector).not.toMatch(/:has\([^)]*:has\(/);
    // A row is its slot's first child, which is what the rule reads.
    const { container } = renderWithProviders(
      <div data-list-slot="">
        <ToolRow adapterId="brew" sourceLabel="Homebrew" name="glib" description="C library" />
      </div>,
    );
    expect((container.querySelector("[data-list-slot]") as HTMLElement).firstElementChild).toHaveAttribute("data-tool-row");
  });

  it("takes no part in a list without arrow keys: not focusable itself", () => {
    const { container } = renderWithProviders(
      <ToolRow adapterId="brew" sourceLabel="Homebrew" name="glib" description="Core application library for C" />,
    );
    const row = container.querySelector("[data-tool-row]") as HTMLElement;
    expect(row).not.toHaveAttribute("tabindex");
    expect(row).not.toHaveAttribute("data-row-focus");
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
