import { Profiler, type MouseEvent } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, getByText, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { DESCRIPTION_MIN_CHARACTERS, MIDDLE_CUT_FROM, ROW_FIT_WIDTHS, RowAction, rowFitFor, ToolRow } from "./ToolRow";
import { StatusChip } from "./StatusChip";
import { BUTTON } from "./ui/controls";
import { Menu } from "./ui/Menu";
import { ListWidthProvider, VirtualList } from "./VirtualList";
import { RovingRowProvider } from "./rovingRows";
import type { ArtifactKey } from "../lib/types";
import { readFileSync } from "node:fs";

// jsdom has no canvas to measure text with: a font 7 wide a character,
// used only where a line has a width (a test that lays one out).
vi.mock("../lib/middleCut", async (original) => ({
  ...(await original<typeof import("../lib/middleCut")>()),
  textMeasurer: () => (text: string) => text.length * 7,
}));
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
    // The status, version, action and ⋯ columns, 16 apart; the status
    // 120 wide with its word at its left, the version at least 120, the
    // action 80 and the ⋯ 24.
    const status = row.querySelector("[data-status]") as HTMLElement;
    expect(atRest(status.className)).toEqual(expect.arrayContaining(["ml-4", "min-w-30", "shrink-0", "justify-start"]));
    const version = container.querySelector(".tabular-nums") as HTMLElement;
    expect(atRest(version.className)).toEqual(expect.arrayContaining(["ml-4", "min-w-30", "text-right", "text-body", "text-muted"]));
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

  it("cuts a very long name in its middle as one string fitted to its line, keeping its end, and says it whole", () => {
    const long = "modelscope.cn/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M";
    expect(long.length).toBeGreaterThan(MIDDLE_CUT_FROM);
    // A line 300 wide, and a font 7 wide a character (`textMeasurer`, mocked).
    const box = vi
      .spyOn(HTMLElement.prototype, "getBoundingClientRect")
      .mockImplementation(function (this: HTMLElement) {
        return new DOMRect(0, 0, this.tagName === "DIV" ? 300 : 0, 16);
      });
    try {
      const { container } = renderWithProviders(
        <ToolRow adapterId="ollama" sourceLabel="Ollama" name={long} description="Ollama model" />,
      );
      const name = container.querySelector("p[title]") as HTMLElement;
      expect(name).toHaveAttribute("title", long);
      expect(name).toHaveAttribute("data-cut-middle");
      // One string -- the start, "…", the last 12 -- as wide as the line
      // takes, in one box: no second box to leave a hole before the end.
      const [shown, spoken] = [...name.children] as HTMLElement[];
      expect(shown.textContent).toBe(`${long.slice(0, 29)}…-GGUF:Q4_K_M`);
      expect(shown.textContent!.length * 7).toBeLessThanOrEqual(300);
      expect(shown).toHaveAttribute("aria-hidden", "true");
      expect(shown.childElementCount).toBe(0);
      // A screen reader hears it whole.
      expect(spoken.textContent).toBe(long);
      expect(spoken.className).toBe("sr-only");
    } finally {
      box.mockRestore();
    }

    // Nothing measured (a line not laid out): whole, cut at its end.
    const unmeasured = renderWithProviders(
      <ToolRow adapterId="ollama" sourceLabel="Ollama" name={long} description="Ollama model" />,
    );
    const uncut = unmeasured.container.querySelector("p[title]") as HTMLElement;
    expect(uncut.textContent).toBe(long);
    expect(uncut.childElementCount).toBe(0);
    expect(atRest(uncut.className)).toEqual(expect.arrayContaining(["min-w-0", "truncate"]));
    unmeasured.unmount();

    // A shorter name is one piece, cut at its end.
    const short = renderWithProviders(
      <ToolRow adapterId="brew" sourceLabel="Homebrew" name="Microsoft Visual Studio Code" description="Editor" />,
    );
    const whole = short.container.querySelector("p[title]") as HTMLElement;
    expect(whole.childElementCount).toBe(0);
    expect(whole.className).toContain("truncate");
  });

  it("names a model by its last path segment, cut at its end, with where it is from at the start of its line, and says it whole", () => {
    const long = "modelscope.cn/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M";
    // A line 300 wide and a font 7 wide a character, as above: the middle
    // cut would fit the whole path to it; a model's name is not cut so.
    const box = vi
      .spyOn(HTMLElement.prototype, "getBoundingClientRect")
      .mockImplementation(function (this: HTMLElement) {
        return new DOMRect(0, 0, this.tagName === "DIV" ? 300 : 0, 16);
      });
    try {
      const { container, getByText } = renderWithProviders(
        <ToolRow
          adapterId="ollama"
          sourceLabel="Ollama"
          name={long}
          namePath={{ name: "Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M", from: "modelscope.cn/Qwen" }}
          description="Ollama model"
        />,
      );
      const name = container.querySelector("p[title]") as HTMLElement;
      // The whole path in its tooltip and for a screen reader.
      expect(name).toHaveAttribute("title", long);
      expect(name).not.toHaveAttribute("data-cut-middle");
      const [shown, spoken] = [...name.children] as HTMLElement[];
      expect(shown.textContent).toBe("Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M");
      expect(shown).toHaveAttribute("aria-hidden", "true");
      expect(spoken.textContent).toBe(long);
      expect(spoken.className).toBe("sr-only");
      // Cut at its end if it still does not fit.
      expect(atRest(name.className)).toEqual(expect.arrayContaining(["min-w-0", "truncate"]));
      expect(name).toHaveAttribute("lang", "en");
      // Where it is from, first on the description's line, in sight only.
      const line = getByText("Ollama model", { exact: false }) as HTMLElement;
      expect(line.textContent).toBe("modelscope.cn/Qwen · Ollama model");
      expect(line).toHaveAttribute("title", "modelscope.cn/Qwen · Ollama model");
      const from = line.querySelector("[data-name-from]") as HTMLElement;
      expect(from.textContent).toBe("modelscope.cn/Qwen · ");
      expect(from).toHaveAttribute("aria-hidden", "true");
    } finally {
      box.mockRestore();
    }
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

    // Left out: no column at all -- but the status word's, which every
    // row has, empty or not, so the words line up down a list.
    rerender(<ToolRow adapterId="brew" sourceLabel="Homebrew" name="glib" description="Core application library for C" />);
    expect(columns()).toBe(withEverything - 3);
    const slot = row().querySelector("[data-status-column]") as HTMLElement;
    expect(slot.childElementCount).toBe(0);
    expect(slot.className).toContain("min-w-30");
    // Only a word is `data-status`, what finds a row's word.
    expect(slot).not.toHaveAttribute("data-status");
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

    it("gives way column by column as the list narrows: the version first, then the status word's column, the button last", () => {
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
      expect(rowFitFor(ROW_FIT_WIDTHS.minimal)).toBe("minimal");
      expect(ROW_FIT_WIDTHS.minimal).toBe(340);
      expect(rowFitFor(ROW_FIT_WIDTHS.minimal - 1)).toBe("slim");
      // Beside it in the narrowest window, 800: the list 332 wide, which
      // keeps the button (R9).
      expect(rowFitFor(332)).toBe("slim");
      expect(ROW_FIT_WIDTHS.slim).toBe(324);
      expect(rowFitFor(ROW_FIT_WIDTHS.slim)).toBe("slim");
      expect(rowFitFor(ROW_FIT_WIDTHS.slim - 1)).toBe("tiny");
      expect(rowFitFor(291)).toBe("tiny");
    });

    it("shows the whole version change, and the status word in its column, with room for everything", () => {
      const { container, getByText } = renderWithProviders(row(752));
      expect(container.querySelector(".tabular-nums")?.textContent).toBe("2.1.282 → 2.1.290");
      const status = getByText("Updates itself").closest("[data-status]") as HTMLElement;
      expect(status.className).toContain("ml-4");
    });

    it("says only the new version first, after its arrow, the whole change still to a screen reader", () => {
      const { container, getByText } = renderWithProviders(row(ROW_FIT_WIDTHS.compact));
      const version = container.querySelector(".tabular-nums") as HTMLElement;
      const [shown, spoken] = [...version.children] as HTMLElement[];
      // Not a bare "2.1.290", which would read as the version installed.
      expect(shown.textContent).toBe("→ 2.1.290");
      expect(version.className).toContain("min-w-20");
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
      // 「状态词 · 描述」: a dot, spaced, sets the word apart from the description.
      const dot = status.nextElementSibling as HTMLElement;
      expect(dot.textContent).toBe(" · ");
      expect(dot).toHaveAttribute("aria-hidden", "true");
      expect(dot.className).toContain("whitespace-pre");
      expect(dot.nextElementSibling).toBe(blurb);
      expect(blurb.parentElement?.textContent).toBe("Updates itself · Anthropic's coding assistant");
      expect(container.querySelectorAll("[data-status]")).toHaveLength(1);
      // Its column is gone with it.
      expect(container.querySelector("[data-status-column]")).toBeNull();
      // The description gives way; the name and the button never shrink
      // below themselves (the button's column is fixed, the name's line
      // loses its description first).
      expect(blurb.className).toContain("truncate");
      expect(getByRole("button", { name: "Update" }).parentElement?.className).toContain("shrink-0");
      expect(container.querySelector(".tabular-nums")?.textContent).toBe("→ 2.1.2902.1.282 → 2.1.290");
    });

    it("says no dot where the row has no status word", () => {
      const { getByText } = renderWithProviders(
        <ListWidthProvider value={592}>
          <ToolRow adapterId="brew" sourceLabel="Homebrew" name="jq" description="JSON processor" version="1.8.2" />
        </ListWidthProvider>,
      );
      expect(getByText("JSON processor").parentElement?.textContent).toBe("JSON processor");
    });

    it("beside the inspector, gives up the version column: an update's change goes after the status word, a plain version goes", () => {
      const { container, getByText, getByRole } = renderWithProviders(row(451));
      const blurb = getByText("Anthropic's coding assistant");
      const version = container.querySelector("[data-version]") as HTMLElement;
      expect(container.querySelectorAll("[data-version]")).toHaveLength(1);
      // On the description's line: the word, the change, the description,
      // a dot between each.
      expect(version.parentElement).toBe(blurb.parentElement);
      expect(version.previousElementSibling?.textContent).toBe(" · ");
      expect(version.previousElementSibling?.previousElementSibling).toHaveAttribute("data-status");
      expect(version.nextElementSibling?.textContent).toBe(" · ");
      expect(version.nextElementSibling?.nextElementSibling).toBe(blurb);
      expect(blurb.parentElement?.textContent).toBe("Updates itself · 2.1.282 → 2.1.290 · Anthropic's coding assistant");
      expect(version.textContent).toBe("2.1.282 → 2.1.290");
      // It gives way only once the description has: the description is
      // laid out from nothing (`flex-1`, a basis of 0) and takes what is left.
      expect(version.className.split(" ")).toEqual(
        expect.arrayContaining(["min-w-0", "truncate", "whitespace-nowrap", "tabular-nums"]),
      );
      expect(version.className).not.toMatch(/\bshrink/);
      expect(blurb.className.split(" ")).toEqual(expect.arrayContaining(["min-w-0", "flex-1", "truncate"]));
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

    it("beside the inspector in the narrowest window, keeps the button and the ⋯, and gives the description's line to the description alone", () => {
      const { container, getByText, getByRole } = renderWithProviders(
        <ListWidthProvider value={332}>
          <ToolRow
            adapterId="brew"
            sourceLabel="Homebrew"
            name="claude-code"
            description="Anthropic's coding assistant"
            status={<StatusChip label="Updates itself" />}
            version="2.1.282 → 2.1.290"
            newVersion="2.1.290"
            action={<button type="button">Uninstall…</button>}
            menu={<Menu label="More actions for claude-code" items={[{ id: "details", label: "Details", onSelect: vi.fn() }]} />}
          />
        </ListWidthProvider>,
      );
      // The button, in its column of at least 80, and the ⋯ after it.
      const button = getByRole("button", { name: "Uninstall…" });
      expect(button.parentElement?.className.split(" ")).toEqual(expect.arrayContaining(["min-w-20", "shrink-0"]));
      expect(getByRole("button", { name: "More actions for claude-code" })).toBeInTheDocument();
      // The status word and the versions have gone before it, from the
      // line and their columns alike: the inspector says them.
      const blurb = getByText("Anthropic's coding assistant");
      expect(blurb.parentElement?.textContent).toBe("Anthropic's coding assistant");
      expect(blurb.parentElement?.querySelector("[data-line-dot]")).toBeNull();
      expect(container.querySelector("[data-status]")).toBeNull();
      expect(container.querySelector("[data-status-column]")).toBeNull();
      expect(container.querySelector("[data-version]")).toBeNull();
      expect(container.textContent).not.toContain("Updates itself");
      expect(container.textContent).not.toContain("2.1.290");
      expect(blurb.className).toContain("truncate");
      expect(blurb.className).not.toContain("sr-only");
    });

    it("in a list too narrow for the button, gives it up as well and keeps the ⋯", () => {
      const { container, getByText, getByRole, queryByRole } = renderWithProviders(
        <ListWidthProvider value={291}>
          <ToolRow
            adapterId="brew"
            sourceLabel="Homebrew"
            name="claude-code"
            description="Anthropic's coding assistant"
            status={<StatusChip label="Updates itself" />}
            version="2.1.282 → 2.1.290"
            newVersion="2.1.290"
            action={<button type="button">Update</button>}
            menu={<Menu label="More actions for claude-code" items={[{ id: "details", label: "Details", onSelect: vi.fn() }]} />}
          />
        </ListWidthProvider>,
      );
      expect(queryByRole("button", { name: "Update" })).toBeNull();
      expect(getByRole("button", { name: "More actions for claude-code" })).toBeInTheDocument();
      const blurb = getByText("Anthropic's coding assistant");
      expect(blurb.parentElement?.textContent).toBe("Anthropic's coding assistant");
      expect(container.querySelector("[data-version]")).toBeNull();
      expect(container.querySelector("[data-status]")).toBeNull();
      expect(container.querySelector("[data-status-column]")).toBeNull();
    });

    it("beside the inspector, drops a description left room for fewer than 8 of its characters, rather than cut it to 「G…」", () => {
      expect(DESCRIPTION_MIN_CHARACTERS).toBe(8);
      // The description's line `lineWidth` wide; the status word 80; a dot
      // and each of the version's characters 7, as the font is
      // (`textMeasurer`, mocked). The words before the description take
      // 80 + 21 + 119 = 220.
      const laidOut = (lineWidth: number) =>
        vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
          const width =
            this.querySelector(":scope > [data-description]") !== null
              ? lineWidth
              : this.hasAttribute("data-status")
                ? 80
                : this.hasAttribute("data-line-dot") || this.hasAttribute("data-version")
                  ? (this.textContent?.length ?? 0) * 7
                  : 0;
          return new DOMRect(0, 0, width, 16);
        });
      const blurbAt = (lineWidth: number) => {
        const box = laidOut(lineWidth);
        try {
          const { container, unmount } = renderWithProviders(row(451));
          const blurb = getByText(container, "Anthropic's coding assistant");
          const line = blurb.parentElement as HTMLElement;
          const result = {
            className: blurb.className,
            text: line.textContent,
            dots: line.querySelectorAll("[data-line-dot]").length,
          };
          unmount();
          return result;
        } finally {
          box.mockRestore();
        }
      };

      // 「 · Anthropi」 is 11 characters, 77 wide: with 76 left it goes --
      // from sight, and its dot with it -- and the status word and the
      // change stay whole.
      const dropped = blurbAt(220 + 76);
      expect(dropped.className).toBe("sr-only");
      expect(dropped.dots).toBe(1);
      expect(dropped.text).toBe("Updates itself · 2.1.282 → 2.1.290Anthropic's coding assistant");
      // A screen reader still hears it; the inspector shows it whole.

      // With 77 left, it stays, cut short by its box.
      const kept = blurbAt(220 + 77);
      expect(kept.className.split(" ")).toEqual(expect.arrayContaining(["min-w-0", "flex-1", "truncate"]));
      expect(kept.dots).toBe(2);
      expect(kept.text).toBe("Updates itself · 2.1.282 → 2.1.290 · Anthropic's coding assistant");

      // Nothing laid out (a line 0 wide): it stays, for its box to cut.
      const unmeasured = blurbAt(0);
      expect(unmeasured.className).not.toBe("sr-only");
      expect(unmeasured.dots).toBe(2);

      // A row with nothing before its description keeps it whatever the room.
      const box = laidOut(10);
      try {
        const plain = renderWithProviders(
          <ListWidthProvider value={451}>
            <ToolRow adapterId="brew" sourceLabel="Homebrew" name="jq" description="JSON processor" version="1.8.2" />
          </ListWidthProvider>,
        );
        expect(getByText(plain.container, "JSON processor").className).not.toBe("sr-only");
      } finally {
        box.mockRestore();
      }
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
    // The ⋯ is always there, quiet: its own rest grey (3:1), muted under
    // the pointer or the focus, never a fill.
    expect(atRest(trigger.className)).toEqual(expect.arrayContaining(["text-glyph-rest", "h-6", "w-6"]));
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
    // Its focus ring is index.css's inset one (below), not an outline
    // round its box: no outline class of its own.
    expect(row.className).not.toMatch(/outline/);
    expect(row.className.split(" ")).toContain("relative");

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
        "[data-list]:focus-within [data-tool-row][data-selected] :is(.text-foreground, .text-muted, .text-tertiary, .text-glyph-rest):not([data-popup-open] > :not(button), [data-popup-open] > :not(button) *) { color: #fff; }",
      );
      expect(css).toContain(
        "[data-tool-row][data-selected] [data-row-separator], [data-list-slot]:has(+ [data-list-slot] [data-tool-row][data-selected]) [data-row-separator] { display: none; }",
      );
      expect(css).not.toMatch(/:has\([^)]*:has\(/);
    });

    it("keeps its own buttons seen on the selection: 14% black on the grey, 22% white with white words on the accent", () => {
      const css = readFileSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../index.css"), "utf-8")
        .replace(/\/\*[\s\S]*?\*\//g, "")
        .replace(/\s+/g, " ");
      // A grey button's fill is the fill token: the selection sets its own.
      expect(BUTTON.regular.grey.split(" ")).toEqual(expect.arrayContaining(["bg-fill", "enabled:active:bg-fill-pressed"]));
      expect(css).toContain(
        "[data-tool-row][data-selected] { --color-fill: rgb(0 0 0 / 0.14); --color-fill-pressed: rgb(0 0 0 / 0.22); }",
      );
      expect(css).toContain(
        "[data-list]:focus-within [data-tool-row][data-selected] { --color-fill: rgb(255 255 255 / 0.22); --color-fill-pressed: rgb(255 255 255 / 0.32); }",
      );
      // Its words are white with the rest of the row's (`.text-foreground`);
      // one that is off, half white.
      expect(css).toContain(
        "[data-list]:focus-within [data-tool-row][data-selected] button:disabled:is(.text-foreground) { color: rgb(255 255 255 / 0.5); }",
      );
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

  it("rings the keyboard's focus inset and rounded, as the selection is, never square round the row", () => {
    const css = readFileSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../index.css"), "utf-8")
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/\s+/g, " ");
    // No outline round the line's box, which lies flush with the window's edges.
    expect(css).toContain("[data-row-focus]:focus-visible, [data-row-open]:focus-visible { outline: none; }");
    // The ring: 10 in from either side, 2 from the top and bottom, corners
    // of 8, 3 wide in the focus colour, over the row's words and out of
    // the pointer's way; none round a selected row.
    expect(css).toContain(
      "[data-row-focus]:not([data-selected]):focus-visible::after, [data-tool-row]:not([data-selected]):has(> [data-row-open]:focus-visible)::after { content: \"\"; position: absolute; inset: 2px 10px; z-index: 20; border-radius: 8px; box-shadow: inset 0 0 0 3px var(--color-focus); pointer-events: none; }",
    );
    // The row is what the ring is placed in, and its open button a direct child.
    const { container } = renderWithProviders(
      <ToolRow adapterId="brew" sourceLabel="Homebrew" name="jq" description="JSON" onOpen={vi.fn()} openLabel="Details: jq" />,
    );
    const row = container.querySelector("[data-tool-row]") as HTMLElement;
    expect(row.className.split(" ")).toContain("relative");
    const open = row.querySelector("[data-row-open]") as HTMLElement;
    expect(open.parentElement).toBe(row);
    expect(open.className).not.toMatch(/outline/);
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

describe("ToolRow as a list mounts it", () => {
  // Long enough to be cut in its middle (`MIDDLE_CUT_FROM`).
  const LONG = "@modelcontextprotocol/server-filesystem";
  /** A row with every column: the status word, an update's versions, its button and its ⋯. */
  const fullRow = (
    <ToolRow
      adapterId="brew"
      sourceLabel="Homebrew"
      name="claude-code"
      description="Anthropic's coding assistant"
      status={<StatusChip label="Updates itself" />}
      version="2.1.282 → 2.1.290"
      newVersion="2.1.290"
      action={<RowAction onClick={() => {}}>Update</RowAction>}
      menu={<Menu label="More actions for claude-code" items={[{ id: "details", label: "Details", onSelect: () => {} }]} />}
      onOpen={() => {}}
      openLabel="Details: claude-code"
    />
  );

  it("draws once as it mounts, in a list of any width, setting nothing from its own box", () => {
    expect(LONG.length).toBeGreaterThan(MIDDLE_CUT_FROM);
    // A scroll mounts a dozen rows at a time: a row that drew itself again
    // for what it measured of itself drew them all two and three times.
    for (const width of [752, ROW_FIT_WIDTHS.compact, 592, 451, 332, 291, null]) {
      for (const row of [
        fullRow,
        <ToolRow key="long" adapterId="npm" sourceLabel="npm" name={LONG} description="MCP server" version="2025.8.21" />,
      ]) {
        const commits: string[] = [];
        const { unmount } = renderWithProviders(
          <ListWidthProvider value={width}>
            <Profiler id="row" onRender={(_id, phase) => commits.push(phase)}>
              {row}
            </Profiler>
          </ListWidthProvider>,
        );
        expect({ width, commits }).toEqual({ width, commits: ["mount"] });
        unmount();
      }
    }
  });

  it("reads nothing of the layout where nothing can crowd its text: a name that is never cut, a description with nothing before it", () => {
    const box = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect");
    try {
      const { unmount } = renderWithProviders(
        <ListWidthProvider value={752}>
          {fullRow}
          <ToolRow adapterId="brew" sourceLabel="Homebrew" name="jq" description="JSON processor" version="1.8.2" />
        </ListWidthProvider>,
      );
      expect(box).not.toHaveBeenCalled();
      unmount();

      // A name that may be cut in its middle is measured: its line, and
      // nothing else on the row.
      renderWithProviders(<ToolRow adapterId="npm" sourceLabel="npm" name={LONG} description="MCP server" />);
      expect(box).toHaveBeenCalled();
      for (const element of box.mock.contexts as HTMLElement[]) {
        expect(element.querySelector(":scope > [data-cut-middle]")).not.toBeNull();
      }
    } finally {
      box.mockRestore();
    }
  });

  it("observes no box of its own: a list is measured once, however many rows it has", () => {
    // Every observer made, and what each watches.
    const made: Element[][] = [];
    const Observer = globalThis.ResizeObserver;
    globalThis.ResizeObserver = class {
      private readonly targets: Element[] = [];
      constructor() {
        made.push(this.targets);
      }
      observe(target: Element) {
        this.targets.push(target);
      }
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
    // The virtualizer measures its box and its slots through offsetHeight,
    // which jsdom has as 0: a box 600 high, and slots 52.
    const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
      return this.getAttribute("data-index") === null ? 600 : 52;
    });
    try {
      // Rows drawn outside a list (the Unknown page's): no observer at all.
      const bare = renderWithProviders(
        <ListWidthProvider value={451}>
          {fullRow}
          <ToolRow adapterId="npm" sourceLabel="npm" name={LONG} description="MCP server" />
        </ListWidthProvider>,
      );
      expect(made).toHaveLength(0);
      bare.unmount();

      const observedFor = (count: number) => {
        made.length = 0;
        const items = Array.from({ length: count }, (_, index) => (index % 2 === 0 ? `${LONG}-${index}` : `tool-${index}`));
        const { container, unmount } = renderWithProviders(
          <VirtualList
            items={items}
            itemKey={(item) => item}
            estimateSize={() => 52}
            renderItem={(item) => <ToolRow adapterId="npm" sourceLabel="npm" name={item} description="A tool" />}
          />,
        );
        expect(container.querySelectorAll("[data-tool-row]")).toHaveLength(count);
        const targets = made.flat();
        unmount();
        return { observers: made.length, inRows: targets.filter((target) => target.closest("[data-tool-row]") !== null) };
      };
      const few = observedFor(2);
      const more = observedFor(8);
      // As many observers for eight rows as for two -- the list's own and
      // the virtualizer's -- and none of them watches anything in a row.
      expect(more.observers).toBe(few.observers);
      expect(few.inRows).toEqual([]);
      expect(more.inRows).toEqual([]);
    } finally {
      height.mockRestore();
      globalThis.ResizeObserver = Observer;
    }
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
