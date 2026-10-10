import { Profiler, type MouseEvent, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, getByText, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { DESCRIPTION_MIN_CHARACTERS, MIDDLE_CUT_FROM, ROW_FIT_WIDTHS, RowAction, rowFitFor, ToolRow, type ToolRowContentProps } from "./ToolRow";
import { StatusChip } from "./StatusChip";
import { BUTTON } from "./ui/controls";
import { Menu } from "./ui/Menu";
import { ListWidthProvider, StatusColumnProvider, VirtualList } from "./VirtualList";
import { RovingRowProvider, useRovingRow } from "./rovingRows";
import type { ArtifactKey } from "../lib/types";
import { readFileSync } from "node:fs";

// jsdom has no canvas to measure text with: a font 7 wide a character,
// used only where a line has a width (a test that lays one out).
vi.mock("../lib/middleCut", async (original) => ({
  ...(await original<typeof import("../lib/middleCut")>()),
  textMeasurer: () => (text: string) => text.length * 7,
}));
// The real hook, watched: a row calls it once each time it is drawn, and
// nothing else here calls it, so its calls count a row's draws.
vi.mock("./rovingRows", async (original) => {
  const actual = await original<typeof import("./rovingRows")>();
  return { ...actual, useRovingRow: vi.fn(actual.useRovingRow) };
});
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

  it("is named with its tool's name after its words, where the page says it", () => {
    const { getByRole } = renderWithProviders(
      <RowAction onClick={() => {}} ariaLabel="Update git">
        Update
      </RowAction>,
    );
    expect(getByRole("button", { name: "Update git" })).toHaveTextContent("Update");
  });
});

describe("ToolRow", () => {
  it("shows the tool's avatar with its source's mark, the name, and one line about it: no pill, the source in the avatar's tooltip and for a screen reader", () => {
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
    // No logo of its own in this test's empty pack: the program tile, not
    // the source's avatar (I8), 32px, the size a row's avatar is, with the
    // source's mark -- here its initial -- on its corner.
    expect(avatar?.querySelector("[data-program-tile]")?.className).toContain("h-8");
    const badge = avatar?.querySelector("[data-source-badge]");
    expect(badge).toHaveTextContent("H");
    expect(badge?.firstElementChild?.className).toContain("bg-source-homebrew");
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
    expect(atRest(source.className)).toEqual(expect.arrayContaining(["text-small", "text-muted"]));
    expect(source.className).not.toContain("sr-only");
    expect(source.className).not.toMatch(/rounded|border|bg-/);
    // Right after the name, on its line.
    expect(source.previousElementSibling).toBe(getByText("black"));
  });

  it("keeps the name whole before its source's words: they give way first, cut short, whole in their tooltip and to a screen reader", () => {
    // 「w…」 beside 「pip（/opt/homebrew/bin/python3.11）」 whole, as a pip
    // package two Pythons list was drawn: the name is what the row is.
    const where = "pip（/opt/homebrew/bin/python3.11）";
    const { getByText } = renderWithProviders(
      <ToolRow adapterId="pip" sourceLabel={where} name="wheel" showSource description="Python package" />,
    );
    const name = getByText("wheel");
    // The name takes the room it needs and gives none of it up -- up to
    // the whole line, where it is cut at its end only when it alone is
    // wider than that.
    expect(atRest(name.className)).toEqual(expect.arrayContaining(["shrink-0", "max-w-full", "truncate"]));
    expect(atRest(name.className)).not.toContain("min-w-0");
    // The source's words take what is left, cut short at their end.
    const source = getByText(where, { selector: "span" });
    expect(atRest(source.className)).toEqual(expect.arrayContaining(["min-w-0", "truncate"]));
    expect(atRest(source.className)).not.toContain("shrink-0");
    expect(source).toHaveAttribute("title", where);
    // Their whole text is there for a screen reader to read.
    expect(source.textContent).toBe(where);
  });

  it("cuts the source's words in their middle, keeping their end, where two copies of one source differ", () => {
    // 「pip（/opt/homebrew/bin/python3.…」 and 「pip（/opt/homebrew/bin/python3）」
    // look alike; the end is what tells the two Pythons apart. A line 200
    // wide, the name 35 (`wheel`, 5 characters of 7), so 165 for the words:
    // 23 of their characters, the last 12 of them kept.
    const where = "pip（/opt/homebrew/bin/python3.11）";
    const box = vi
      .spyOn(HTMLElement.prototype, "getBoundingClientRect")
      .mockImplementation(function (this: HTMLElement) {
        const width = this.tagName === "DIV" ? 200 : this.tagName === "P" ? 35 : 0;
        return new DOMRect(0, 0, width, 16);
      });
    try {
      const { container } = renderWithProviders(
        <ToolRow adapterId="pip" sourceLabel={where} name="wheel" showSource description="Python package" />,
      );
      const note = container.querySelector("[data-row-note]") as HTMLElement;
      expect(note).toHaveAttribute("title", where);
      const [shown, spoken] = [...note.children] as HTMLElement[];
      expect(shown.textContent).toBe("pip（/opt/h…/python3.11）");
      expect(shown).toHaveAttribute("aria-hidden", "true");
      expect(spoken.textContent).toBe(where);
      expect(spoken.className).toBe("sr-only");
    } finally {
      box.mockRestore();
    }
    // Less room than a start, … and those 12 take (a list beside the
    // inspector in the narrowest window): the end alone, after a … -- and
    // nothing at all where not even that fits, rather than a lone 「p…」.
    for (const [line, shown] of [
      [120, "…python3.11）"],
      [77, "…3.11）"],
      [48, ""],
    ] as const) {
      const narrow = vi
        .spyOn(HTMLElement.prototype, "getBoundingClientRect")
        .mockImplementation(function (this: HTMLElement) {
          const width = this.tagName === "DIV" ? line : this.tagName === "P" ? 35 : 0;
          return new DOMRect(0, 0, width, 16);
        });
      try {
        const { container, unmount } = renderWithProviders(
          <ToolRow adapterId="pip" sourceLabel={where} name="wheel" showSource description="Python package" />,
        );
        const note = container.querySelector("[data-row-note]") as HTMLElement;
        expect(note.querySelector("[aria-hidden]")?.textContent).toBe(shown);
        expect(note.querySelector(".sr-only")?.textContent).toBe(where);
        unmount();
      } finally {
        narrow.mockRestore();
      }
    }
    // Nothing measured (a line not laid out): whole, for its box to cut.
    const { container } = renderWithProviders(
      <ToolRow adapterId="pip" sourceLabel={where} name="wheel" showSource description="Python package" />,
    );
    const note = container.querySelector("[data-row-note]") as HTMLElement;
    expect(note.textContent).toBe(where);
    expect(note.childElementCount).toBe(0);
  });

  it("hides a source's words too short to cut where they do not fit, rather than leave a fragment of them", () => {
    // 「ansible Ho…」, 「ansible-lint u」: a one-word source -- Homebrew, uv,
    // npm -- has no middle to cut and no end that says more than its
    // start, so where it does not fit it is not shown at all, whole in its
    // tooltip and to a screen reader. A line `line` wide, the name drawn
    // 49 (`ansible`, 7 characters of 7).
    for (const [where, line, shown] of [
      ["Homebrew", 60, ""],
      ["uv", 60, ""],
      ["Homebrew", 200, null],
      ["uv", 63, null],
    ] as const) {
      const box = vi
        .spyOn(HTMLElement.prototype, "getBoundingClientRect")
        .mockImplementation(function (this: HTMLElement) {
          const width = this.tagName === "DIV" ? line : this.tagName === "P" ? 49 : 0;
          return new DOMRect(0, 0, width, 16);
        });
      try {
        const { container, unmount } = renderWithProviders(
          <ToolRow adapterId="brew" sourceLabel={where} name="ansible" showSource description="IT automation" />,
        );
        const note = container.querySelector("[data-row-note]") as HTMLElement;
        expect(note).toHaveAttribute("title", where);
        if (shown === null) {
          // Room for it: whole, nothing hidden.
          expect(note.textContent).toBe(where);
          expect(note.childElementCount).toBe(0);
        } else {
          expect(note.querySelector("[aria-hidden]")?.textContent).toBe(shown);
          expect(note.querySelector(".sr-only")?.textContent).toBe(where);
        }
        unmount();
      } finally {
        box.mockRestore();
      }
    }
  });

  it("fits a very long name to its whole line, not to what the source's words leave it", () => {
    const long = "@modelcontextprotocol/server-filesystem-extended";
    expect(long.length).toBeGreaterThan(MIDDLE_CUT_FROM);
    // A line 300 wide, the source's words drawn 100 wide on it, and a font
    // 7 wide a character (`textMeasurer`, mocked): the name is cut to the
    // line's 300, and the words beside it get what is left.
    const box = vi
      .spyOn(HTMLElement.prototype, "getBoundingClientRect")
      .mockImplementation(function (this: HTMLElement) {
        const width = this.tagName === "DIV" ? 300 : this.textContent === "npm（~/.npm-global）" ? 100 : 0;
        return new DOMRect(0, 0, width, 16);
      });
    try {
      const { container } = renderWithProviders(
        <ToolRow adapterId="npm" sourceLabel="npm（~/.npm-global）" name={long} showSource description="npm package" />,
      );
      const shown = container.querySelector("p[title] [aria-hidden]") as HTMLElement;
      expect(shown.textContent).toMatch(/…/);
      expect(shown.textContent!.length * 7).toBeLessThanOrEqual(300);
      expect(shown.textContent!.length * 7).toBeGreaterThan(300 - 7 * 2);
    } finally {
      box.mockRestore();
    }
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
    expect(atRest(uncut.className)).toEqual(expect.arrayContaining(["max-w-full", "truncate"]));
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
      expect(atRest(name.className)).toEqual(expect.arrayContaining(["max-w-full", "truncate"]));
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

  it("puts a note after its description, on its line, that gives way to the description and never selects with it", () => {
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
    // The note is cut short first; the path keeps up to 70% of the line,
    // whole where it fits, its whole text in its tooltip (walk-3 W3-8: the
    // path, what the row is about, went first, all of it at 800 wide).
    expect(path).toHaveAttribute("title", "/usr/local/bin/docker");
    expect(atRest(path.className)).toEqual(expect.arrayContaining(["max-w-[70%]", "shrink-0", "truncate"]));
    expect(atRest(note.className)).toEqual(expect.arrayContaining(["min-w-0", "truncate"]));
    expect(atRest(note.className)).not.toContain("shrink-0");
    // Cut short, its whole words are its tooltip, as the path's are (walk-3 review 3.2).
    expect(note).toHaveAttribute("title", "Points into Docker.app");
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

  it("gives up its empty status column in a list with no word on any row, but never a word of its own", () => {
    const row = (status?: ReactNode) => (
      <ToolRow
        adapterId="brew"
        sourceLabel="Homebrew"
        name="glib"
        description="Core application library for C"
        status={status}
        version="2.88.3 → 2.90.0"
      />
    );
    const { container, rerender } = renderWithProviders(
      <StatusColumnProvider value={false}>{row()}</StatusColumnProvider>,
    );
    // No word anywhere on the list: no column, the name and description
    // take its room.
    expect(container.querySelector("[data-status-column]")).toBeNull();
    expect(container.querySelector("[data-version]")).not.toBeNull();

    // A row with a word keeps its column whatever the list says, so the
    // word is never lost.
    rerender(<StatusColumnProvider value={false}>{row(<span>Pinned</span>)}</StatusColumnProvider>);
    expect(container.querySelector("[data-status-column]")).toHaveTextContent("Pinned");

    // A list with a word on some row: the empty column on this one too.
    rerender(<StatusColumnProvider value>{row()}</StatusColumnProvider>);
    expect(container.querySelector("[data-status-column]")?.childElementCount).toBe(0);
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

    it("says the whole change all the same where the row has room for it with its name whole", () => {
      // The name's block `block` wide and the version column 63 ("→
      // 2.1.290", 9 characters, 7 each: `textMeasurer`, mocked). The whole
      // change takes 119 and the name 78 (77 and a pixel), so the block
      // and the column together need 197.
      const layout = (block: number) =>
        vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
          const width = this.hasAttribute("data-version")
            ? (this.querySelector("[aria-hidden]")?.textContent ?? this.textContent ?? "").length * 7
            : this.className.includes("ml-3 min-w-0 flex-1")
              ? block
              : 0;
          return new DOMRect(0, 0, width, 16);
        });
      for (const width of [ROW_FIT_WIDTHS.compact, 592]) {
        let box = layout(134);
        try {
          const { container, unmount } = renderWithProviders(row(width));
          const version = container.querySelector("[data-version]") as HTMLElement;
          // Room for both: "2.1.282 → 2.1.290", nothing hidden from sight.
          expect(version.textContent).toBe("2.1.282 → 2.1.290");
          expect(version.querySelector(".sr-only")).toBeNull();
          unmount();
        } finally {
          box.mockRestore();
        }
        box = layout(133);
        try {
          const { container, unmount } = renderWithProviders(row(width));
          // A pixel short: the new version alone, after its arrow.
          expect(container.querySelector("[data-version] [aria-hidden]")?.textContent).toBe("→ 2.1.290");
          unmount();
        } finally {
          box.mockRestore();
        }
      }
    });

    it("counts the source's words whole beside the name, by their text, though their box gives way", () => {
      // As above, with 「Homebrew」 after the name (8 characters, 56): the
      // name and the words need 134, so the block 190 -- whatever width the
      // words' box has been left, here none, as a row that cut them leaves.
      const layout = (block: number) =>
        vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
          const width = this.hasAttribute("data-version")
            ? (this.querySelector("[aria-hidden]")?.textContent ?? this.textContent ?? "").length * 7
            : this.className.includes("ml-3 min-w-0 flex-1")
              ? block
              : 0;
          return new DOMRect(0, 0, width, 16);
        });
      const twice = (
        <ListWidthProvider value={ROW_FIT_WIDTHS.compact}>
          <ToolRow
            adapterId="brew"
            sourceLabel="Homebrew"
            name="claude-code"
            showSource
            description="Anthropic's coding assistant"
            status={<StatusChip label="Updates itself" />}
            version="2.1.282 → 2.1.290"
            newVersion="2.1.290"
            action={<button type="button">Update</button>}
          />
        </ListWidthProvider>
      );
      let box = layout(190);
      try {
        const { container, unmount } = renderWithProviders(twice);
        expect(container.querySelector("[data-version]")?.textContent).toBe("2.1.282 → 2.1.290");
        unmount();
      } finally {
        box.mockRestore();
      }
      box = layout(189);
      try {
        const { container, unmount } = renderWithProviders(twice);
        expect(container.querySelector("[data-version] [aria-hidden]")?.textContent).toBe("→ 2.1.290");
        unmount();
      } finally {
        box.mockRestore();
      }
    });

    it("counts only what the source's words keep when cut, so two copies of one package say their versions alike", () => {
      // wheel from two Pythons, 0.45.1 → 0.46.1 on both: the longer words,
      // 「pip（/opt/homebrew/bin/python3.11）」, counted whole, dropped the
      // version it is now from on one row and not the other, beside it. The
      // words give way first (`RowNote`): what they keep at the least -- a
      // "…" and their last 12 characters, 13 of 7 -- is what is counted,
      // whichever Python. The name 36, so 127 with the words; the column
      // at its widest 105 ("0.45.1 → 0.46.1"), 56 as "→ 0.46.1": a block
      // of 176 has room.
      const layout = (block: number) =>
        vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
          const width = this.hasAttribute("data-version")
            ? (this.querySelector("[aria-hidden]")?.textContent ?? this.textContent ?? "").length * 7
            : this.className.includes("ml-3 min-w-0 flex-1")
              ? block
              : 0;
          return new DOMRect(0, 0, width, 16);
        });
      const copy = (where: string) => (
        <ListWidthProvider value={ROW_FIT_WIDTHS.compact}>
          <ToolRow
            adapterId="pip"
            sourceLabel={where}
            name="wheel"
            showSource
            description="Python package"
            version="0.45.1 → 0.46.1"
            newVersion="0.46.1"
            action={<button type="button">Update</button>}
          />
        </ListWidthProvider>
      );
      for (const [block, shown] of [
        [300, "0.45.1 → 0.46.1"],
        [176, "0.45.1 → 0.46.1"],
        [175, "→ 0.46.1"],
      ] as const) {
        for (const where of ["pip（/opt/homebrew/bin/python3.11）", "pip（/opt/homebrew/bin/python3）"]) {
          const box = layout(block);
          try {
            const { container, unmount } = renderWithProviders(copy(where));
            const version = container.querySelector("[data-version]") as HTMLElement;
            expect([where, block, version.querySelector("[aria-hidden]")?.textContent ?? version.textContent]).toEqual([
              where,
              block,
              shown,
            ]);
            unmount();
          } finally {
            box.mockRestore();
          }
        }
      }
    });

    it("measures no change where the fit shows it whole or not at all, nor a version that is no update", () => {
      const box = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect");
      try {
        const { unmount } = renderWithProviders(row(752));
        unmount();
        renderWithProviders(
          <ListWidthProvider value={ROW_FIT_WIDTHS.compact}>
            <ToolRow adapterId="brew" sourceLabel="Homebrew" name="jq" description="JSON processor" version="1.8.2" />
          </ListWidthProvider>,
        );
        expect(box).not.toHaveBeenCalled();
      } finally {
        box.mockRestore();
      }
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

    it("beside the inspector in the narrowest window, keeps the button, the ⋯ and the status word, and gives up only the versions", () => {
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
      // The versions have gone before it, from the line and their column
      // alike: the inspector says them. The status word stays, at the
      // start of the description's line (「状态词 · 描述」), whole: the
      // description gives way to it.
      const blurb = getByText("Anthropic's coding assistant");
      const status = getByText("Updates itself").closest("[data-status]") as HTMLElement;
      expect(status.parentElement).toBe(blurb.parentElement);
      expect(status.className).toContain("shrink-0");
      expect(blurb.parentElement?.textContent).toBe("Updates itself · Anthropic's coding assistant");
      expect(blurb.parentElement?.querySelectorAll("[data-line-dot]")).toHaveLength(1);
      expect(container.querySelectorAll("[data-status]")).toHaveLength(1);
      expect(container.querySelector("[data-status-column]")).toBeNull();
      expect(container.querySelector("[data-version]")).toBeNull();
      expect(container.textContent).not.toContain("2.1.290");
      expect(blurb.className).toContain("truncate");
      expect(blurb.className).not.toContain("sr-only");
    });

    it("beside the inspector in the narrowest window, drops a description the status word leaves too little room, and keeps the word", () => {
      // The description's line 110 wide, as it is at 800 in English; the
      // status word 90 of it, a dot and each character 7 (`textMeasurer`,
      // mocked): 「 · Anthropi」 needs 77, and 20 is left.
      const box = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
        const width =
          this.querySelector(":scope > [data-description]") !== null
            ? 110
            : this.hasAttribute("data-status")
              ? 90
              : this.hasAttribute("data-line-dot")
                ? (this.textContent?.length ?? 0) * 7
                : 0;
        return new DOMRect(0, 0, width, 16);
      });
      try {
        const { getByText } = renderWithProviders(row(332));
        const blurb = getByText("Anthropic's coding assistant");
        expect(blurb.className).toBe("sr-only");
        expect(getByText("Updates itself").closest("[data-status]")?.parentElement).toBe(blurb.parentElement);
        expect(blurb.parentElement?.querySelector("[data-line-dot]")).toBeNull();
      } finally {
        box.mockRestore();
      }
    });

    // walk-4 W4-2: at 800 beside the inspector, "Updates when run ⓘ" was
    // wider than its line and ran on under the Uninstall… button after it.
    it("beside the inspector, cuts short a status word wider than its line once the description has gone, rather than run under the button", () => {
      const box = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
        const width =
          this.querySelector(":scope > [data-description]") !== null
            ? 93
            : this.hasAttribute("data-status")
              ? 112
              : this.hasAttribute("data-line-dot")
                ? (this.textContent?.length ?? 0) * 7
                : 0;
        return new DOMRect(0, 0, width, 16);
      });
      try {
        const { getByText, getByRole } = renderWithProviders(
          <ListWidthProvider value={332}>
            <ToolRow
              adapterId="antigravity"
              sourceLabel="Antigravity CLI"
              name="Antigravity CLI"
              description="Google's coding agent"
              status={<StatusChip label="Updates when run" detail={<p>Why.</p>} />}
              action={<button type="button">Uninstall…</button>}
            />
          </ListWidthProvider>,
        );
        expect(getByText("Google's coding agent").className).toBe("sr-only");
        const status = getByText("Updates when run").closest("[data-status]") as HTMLElement;
        // Free to be narrower than the word, and so is what it holds.
        expect(status.className.split(" ")).toEqual(expect.arrayContaining(["min-w-0", "[&>*]:min-w-0"]));
        expect(status.className).not.toContain("shrink-0");
        // Alone on the line, it may run 15 into the 16 before the button's
        // column, as the words that fitted before did.
        expect(status.className.split(" ")).toContain("-mr-[15px]");
        expect(getByText("Updates when run").className.split(" ")).toEqual(expect.arrayContaining(["min-w-0", "truncate"]));
        // The button keeps its column.
        expect(getByRole("button", { name: "Uninstall…" }).parentElement?.className).toContain("shrink-0");
      } finally {
        box.mockRestore();
      }
    });

    it("in a list too narrow for the button, gives it up as well and keeps the ⋯ and the status word", () => {
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
      expect(blurb.parentElement?.textContent).toBe("Updates itself · Anthropic's coding assistant");
      expect(container.querySelector("[data-version]")).toBeNull();
      expect(container.querySelectorAll("[data-status]")).toHaveLength(1);
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

  describe("its accessible name, where it takes the focus", () => {
    const rovingRow = (props: Partial<ToolRowContentProps>) =>
      renderWithProviders(
        <RovingRowProvider value={{ tabIndex: 0, onFocus: vi.fn() }}>
          <ToolRow adapterId="brew" sourceLabel="Homebrew" name="git" description="Distributed revision control system" {...props} />
        </RovingRowProvider>,
      );

    it("is a group named by the tool, and the change of version an update brings", () => {
      const { getByRole } = rovingRow({ version: "2.55.0 → 2.55.1", newVersion: "2.55.1" });
      const row = getByRole("group", { name: "git, 2.55.0 → 2.55.1" });
      expect(row).toHaveAttribute("data-tool-row");
      expect(row).toHaveAttribute("tabindex", "0");
    });

    it("says its status word after the name, and no version that is not a change", () => {
      const { getByRole } = rovingRow({
        name: "gh",
        status: <StatusChip label="Skipped 2.102.0" />,
        statusText: "Skipped 2.102.0",
        version: "2.101.0",
      });
      expect(getByRole("group", { name: "gh, Skipped 2.102.0" })).toHaveAttribute("data-tool-row");
    });

    it("is the tool's name alone with nothing else to say", () => {
      const { getByRole } = rovingRow({ version: "2.55.0" });
      expect(getByRole("group", { name: "git" })).toHaveAttribute("data-tool-row");
    });

    it("is named in the window's language where its words are", () => {
      const { getByRole } = rovingRow({ name: "gh", statusText: "已跳过2.102.0", status: <StatusChip label="已跳过2.102.0" /> });
      expect(getByRole("group", { name: "gh, 已跳过2.102.0" })).toHaveAttribute("data-tool-row");
    });

    it("says it is the one selected on the row that takes the focus, not only on its pointer's button", () => {
      const { container, rerender } = rovingRow({ onOpen: vi.fn(), openLabel: "Details: git", selected: true });
      const row = container.querySelector("[data-tool-row]") as HTMLElement;
      expect(row).toHaveAttribute("aria-current", "true");
      rerender(
        <RovingRowProvider value={{ tabIndex: 0, onFocus: vi.fn() }}>
          <ToolRow adapterId="brew" sourceLabel="Homebrew" name="git" description="Distributed revision control system" onOpen={vi.fn()} openLabel="Details: git" />
        </RovingRowProvider>,
      );
      expect(row).not.toHaveAttribute("aria-current");
    });

    it("is no group, and has no name of its own, in a list without arrow keys", () => {
      const { container, queryByRole } = renderWithProviders(
        <ToolRow adapterId="brew" sourceLabel="Homebrew" name="git" description="VCS" version="2.55.0 → 2.55.1" newVersion="2.55.1" />,
      );
      expect(queryByRole("group")).toBeNull();
      expect(container.querySelector("[data-tool-row]")).not.toHaveAttribute("aria-label");
    });
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
      // Every word on it in the accent's words, white -- black on a light
      // accent (decision I21b, src/test/increaseContrast.test.ts) -- but a
      // panel's opened from it, which keeps the window's colours.
      expect(css).toContain(
        "[data-list]:focus-within [data-tool-row][data-selected] :is(.text-foreground, .text-muted, .text-tertiary, .text-glyph-rest):not([data-popup-open] > :not(button), [data-popup-open] > :not(button) *) { color: var(--color-accent-foreground); }",
      );
      // No hairline under it; none over it either -- the row before's,
      // whose slot the page marks as a run's end (InstalledPage's tests).
      expect(css).toContain("[data-tool-row][data-selected] [data-row-separator] { display: none; }");
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
      // Its words are the accent's with the rest of the row's
      // (`.text-foreground`): white, or black on a light accent; one that
      // is off, at half strength.
      expect(css).toContain(
        "[data-list]:focus-within [data-tool-row][data-selected] button:disabled:is(.text-foreground) { color: color-mix(in srgb, var(--color-accent-foreground) 50%, transparent); }",
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

    it("opens with Space a row with no checkbox among rows that have one, the Installed page's", () => {
      const onOpen = vi.fn();
      const { container } = renderWithProviders(
        <RovingRowProvider value={{ tabIndex: 0, onFocus: vi.fn() }}>
          <ToolRow
            adapterId="brew"
            sourceLabel="Homebrew"
            name="jq"
            description="JSON processor"
            selectable={null}
            onOpen={onOpen}
            openLabel="Details: jq"
          />
        </RovingRowProvider>,
      );
      const row = container.querySelector("[data-tool-row]") as HTMLElement;
      expect(fireEvent.keyDown(row, { key: " " })).toBe(false);
      expect(onOpen).toHaveBeenCalledTimes(1);
    });

    it("does what the page says on Enter, where it says: the Installed page's details", () => {
      const onOpen = vi.fn();
      const onEnter = vi.fn();
      const { container } = renderWithProviders(
        <RovingRowProvider value={{ tabIndex: 0, onFocus: vi.fn() }}>
          <ToolRow adapterId="brew" sourceLabel="Homebrew" name="jq" description="JSON processor" onOpen={onOpen} openLabel="Details: jq" onEnter={onEnter} />
        </RovingRowProvider>,
      );
      const row = container.querySelector("[data-tool-row]") as HTMLElement;
      expect(fireEvent.keyDown(row, { key: "Enter" })).toBe(false);
      expect(onEnter).toHaveBeenCalledTimes(1);
      // Not what pressing the row does, which closes a row already open.
      expect(onOpen).not.toHaveBeenCalled();
    });

    it("keeps its button in the Tab order in a list without arrow keys", () => {
      const { getByRole } = renderWithProviders(selectedRow(false));
      expect(getByRole("button", { name: "Details: jq" })).not.toHaveAttribute("tabindex");
    });
  });

  it("has no hairline under a list's last row, nor under the last of a run a line of another kind follows", () => {
    // index.css: the rule that hides it -- under a list's last slot, and
    // under one its list marks as a run's end (`VirtualList`'s
    // `data-run-end`, from its items).
    const css = readFileSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../index.css"), "utf-8").replace(/\/\*[\s\S]*?\*\//g, "");
    const rule = /([^{}]*data-row-separator[^{}]*)\{\s*display:\s*none;?\s*\}/.exec(css);
    expect(rule).not.toBeNull();
    const selectors = (rule?.[1] ?? "").split(",").map((selector) => selector.trim());
    expect(selectors).toEqual([
      "[data-list-slot]:last-child [data-row-separator]",
      "[data-list-slot][data-run-end] [data-row-separator]",
    ]);
    // Nothing in the stylesheet reads a slot's next sibling (`:has(+ …)`,
    // `:has(~ …)`): such a rule has the browser restyle every slot in
    // sight at every step of a scroll.
    expect(css).not.toMatch(/:has\(\s*[+~]/);
    // The row's hairline is inside its slot, where the rule reaches it.
    const { container } = renderWithProviders(
      <div data-list-slot="" data-run-end="">
        <ToolRow adapterId="brew" sourceLabel="Homebrew" name="glib" description="C library" />
      </div>,
    );
    expect(container.querySelector("[data-list-slot][data-run-end] [data-row-separator]")).not.toBeNull();
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

  it("is not drawn again for a new width of its list that fits the same columns, but a long name is fitted again", () => {
    const draws = vi.mocked(useRovingRow);
    draws.mockClear();
    // The same row each time: only what reads the list can draw it again.
    const { rerender, container } = renderWithProviders(<ListWidthProvider value={null}>{fullRow}</ListWidthProvider>);
    expect(draws).toHaveBeenCalledTimes(1);
    // The list laid out as it mounts, then a window resized: all of it fits still.
    rerender(<ListWidthProvider value={752}>{fullRow}</ListWidthProvider>);
    rerender(<ListWidthProvider value={ROW_FIT_WIDTHS.full}>{fullRow}</ListWidthProvider>);
    expect(draws).toHaveBeenCalledTimes(1);
    // Narrower than that: drawn again, the version column saying only the new version.
    rerender(<ListWidthProvider value={ROW_FIT_WIDTHS.full - 1}>{fullRow}</ListWidthProvider>);
    expect(draws).toHaveBeenCalledTimes(2);
    expect(container.querySelector("[data-version]")?.textContent).toBe("→ 2.1.2902.1.282 → 2.1.290");

    // A name long enough to be cut in its middle is fitted to every new width.
    const box = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect");
    try {
      const long = <ToolRow adapterId="npm" sourceLabel="npm" name={LONG} description="MCP server" />;
      const { rerender: again } = renderWithProviders(<ListWidthProvider value={752}>{long}</ListWidthProvider>);
      box.mockClear();
      again(<ListWidthProvider value={760}>{long}</ListWidthProvider>);
      expect(box).toHaveBeenCalled();
    } finally {
      box.mockRestore();
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

  it("shows a cask's own app icon once it arrives, and the program tile until then", async () => {
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

    // Asked, and not here yet: the program tile stands in (I8), the
    // source's letter on its corner.
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("artifact_icon", { key: iterm }));
    expect(avatarOf(container).querySelector("[data-program-tile]")).not.toBeNull();
    expect(avatarOf(container).querySelector("[data-source-badge]")).toHaveTextContent("H");

    await act(async () => answer(ICON));

    await waitFor(() => expect(avatarOf(container).querySelector("img[data-app-icon]")).not.toBeNull());
    const icon = avatarOf(container).querySelector("img[data-app-icon]") as Element;
    expect(icon).toHaveAttribute("src", ICON);
    expect(icon).toHaveAttribute("alt", "");
    // A row avatar's size, rounded like an app icon, with nothing coloured behind it.
    expect(icon.className).toContain("h-8");
    expect(icon.className).toContain("rounded-[7px]");
    expect(icon.className).not.toMatch(/\bbg-/);
    // The tile gives way to it; the source's letter stays on the corner, 14px.
    expect(avatarOf(container).querySelector("[data-program-tile]")).toBeNull();
    const badge = avatarOf(container).querySelector("[data-source-badge]");
    expect(badge).toHaveTextContent("H");
    expect(badge?.firstElementChild?.className).toContain("h-3.5");
  });

  it("keeps the program tile for a cask that has no icon", async () => {
    mockInvoke.mockImplementation((cmd: string) => Promise.resolve(cmd === "artifact_icon" ? null : undefined));
    const { container, queryClient } = renderWithProviders(
      <ToolRow adapterId="brew" sourceLabel="Homebrew" iconKey={iterm} name="iTerm2" description="Terminal emulator" />,
    );

    await waitFor(() => expect(queryClient.getQueryState(["artifactIcon", "brew:/opt/homebrew", "Cask", "iterm2"])?.status).toBe("success"));
    expect(avatarOf(container).tagName).toBe("SPAN");
    expect(avatarOf(container).querySelector("[data-program-tile]")).not.toBeNull();
    expect(avatarOf(container).querySelector("[data-app-icon]")).toBeNull();
  });

  it("asks nothing for a row that is not a cask, or that names no tool", () => {
    mockInvoke.mockResolvedValue(ICON);
    const formula = renderWithProviders(
      <ToolRow adapterId="brew" sourceLabel="Homebrew" iconKey={jq} name="jq" description="JSON processor" />,
    );
    expect(avatarOf(formula.container).querySelector("[data-program-tile]")).not.toBeNull();
    formula.unmount();
    renderWithProviders(<ToolRow adapterId="brew" sourceLabel="Homebrew" name="jq" description="JSON processor" />);

    expect(mockInvoke).not.toHaveBeenCalled();
  });
});
