import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { createRoot, type Root } from "react-dom/client";
import { renderWithProviders } from "../test/setup";
import {
  Refusal,
  SheetLines,
  SheetText,
  SheetTool,
  SheetToolList,
  sheetMeta,
  TOOLS_DRAWN_FIRST,
  useToolsInTurn,
  type ToolsInTurn,
} from "./SheetParts";

describe("sheetMeta", () => {
  it("says the source and the version, each on its own, apart by a middle dot", () => {
    const { container } = render(<p>{sheetMeta("jq", "Homebrew", "1.8.1")}</p>);
    expect(container.textContent).toBe("Homebrew · 1.8.1");
    expect([...container.querySelectorAll("span")].map((span) => span.textContent)).toEqual(["Homebrew", "1.8.1"]);
  });

  it("leaves out the source of a tool that is its own source, and a version it does not have", () => {
    const own = render(<p>{sheetMeta("rustup", "rustup", "1.29.1")}</p>);
    expect(own.container.textContent).toBe("1.29.1");
    own.unmount();
    const model = render(<p>{sheetMeta("qwen3:8b", "Ollama", null)}</p>);
    expect(model.container.textContent).toBe("Ollama");
    model.unmount();
    expect(sheetMeta("rustup", "rustup", null)).toBeNull();
  });
});

describe("SheetText", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  /** Lays its paragraph out `lines` lines of `lineHeight` high, as a browser would. */
  function layOut(lines: number) {
    const real = window.getComputedStyle;
    vi.spyOn(window, "getComputedStyle").mockImplementation((element) => {
      const style = real(element);
      if (!(element instanceof HTMLParagraphElement)) return style;
      const lineHeight = element.className.includes("text-body-long") ? 18 : 16;
      return { ...style, lineHeight: `${lineHeight}px` } as CSSStyleDeclaration;
    });
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
      return this.className.includes("text-body-long") ? lines * 18 : lines * 16;
    });
  }

  it("sets two lines 13/16, in the label colour", () => {
    layOut(2);
    render(<SheetText>Deletes only this installed version of jq and the links to it.</SheetText>);
    const text = screen.getByText(/Deletes only/);
    expect(text).toHaveClass("text-body", "text-foreground");
    expect(text).not.toHaveClass("text-body-long");
  });

  it("sets three lines or more 13/18, where Chinese at 16 is cramped", () => {
    layOut(3);
    render(<SheetText>删除Homebrew为“Microsoft Word”放置的文件，并执行它记下的卸载步骤；其他文件不删除。</SheetText>);
    const text = screen.getByText(/删除Homebrew/);
    expect(text).toHaveClass("text-body-long");
    expect(text).not.toHaveClass("text-body");
  });
});

describe("useToolsInTurn", () => {
  /** A dialog's list of `count` tools, saying what it draws each time it is drawn. */
  function List({ count, batch, stage, seen }: { count: number; batch: number | null; stage: string; seen: ToolsInTurn[] }) {
    const turn = useToolsInTurn(count, batch, stage);
    seen.push(turn);
    return <p data-testid="shown">{`${turn.drawn} ${turn.held}`}</p>;
  }
  const drawnCounts = (seen: ToolsInTurn[]) => [...new Set(seen.map((turn) => turn.drawn))];

  it("draws the first few of a long list with the dialog and the rest a turn at a time, starting over for a new batch", async () => {
    // As many as a 320 high list shows at 36 a tool, and no fewer.
    expect(TOOLS_DRAWN_FIRST * 36).toBeGreaterThanOrEqual(320);
    expect((TOOLS_DRAWN_FIRST - 1) * 36).toBeLessThan(320);
    const seen: ToolsInTurn[] = [];
    const { rerender } = render(<List count={200} batch={1} stage="planning" seen={seen} />);
    expect(seen[0]).toEqual({ drawn: TOOLS_DRAWN_FIRST, held: 0 });
    await waitFor(() => expect(screen.getByTestId("shown")).toHaveTextContent("200 0"));
    expect(drawnCounts(seen)).toEqual([9, 69, 129, 189, 200]);
    expect(seen.every((turn) => turn.held === 0)).toBe(true);

    // Update all again: a new batch, from the first few, holding nothing.
    seen.length = 0;
    rerender(<List count={200} batch={2} stage="planning" seen={seen} />);
    expect(seen[0]).toEqual({ drawn: TOOLS_DRAWN_FIRST, held: 0 });
    await waitFor(() => expect(screen.getByTestId("shown")).toHaveTextContent("200 0"));

    // A list that fits is drawn whole at once, and none while there is no batch.
    seen.length = 0;
    rerender(<List count={5} batch={3} stage="planning" seen={seen} />);
    expect(seen).toEqual(seen.map(() => ({ drawn: 5, held: 0 })));
    rerender(<List count={0} batch={null} stage="planning" seen={seen} />);
    expect(screen.getByTestId("shown")).toHaveTextContent("0 0");
  });

  it("draws a batch's next stage a turn at a time too, holding what the first drew until a turn reaches it", async () => {
    const seen: ToolsInTurn[] = [];
    const { rerender } = render(<List count={200} batch={1} stage="planning" seen={seen} />);
    await waitFor(() => expect(screen.getByTestId("shown")).toHaveTextContent("200 0"));

    // Its plans back: the first few as they are now at once, and all 200
    // the first stage drew held until then.
    seen.length = 0;
    rerender(<List count={200} batch={1} stage="planned" seen={seen} />);
    expect(seen[0]).toEqual({ drawn: TOOLS_DRAWN_FIRST, held: 200 });
    await waitFor(() => expect(screen.getByTestId("shown")).toHaveTextContent("200 0"));
    expect(drawnCounts(seen)).toEqual([9, 69, 129, 189, 200]);
    expect(seen.filter((turn) => turn.drawn < 200).every((turn) => turn.held === 200)).toBe(true);

    // Another batch's first stage holds nothing of this one's.
    seen.length = 0;
    rerender(<List count={200} batch={2} stage="planning" seen={seen} />);
    expect(seen[0]).toEqual({ drawn: TOOLS_DRAWN_FIRST, held: 0 });
  });

  // React as the window runs it: not under `act`, which draws every
  // transition at once, so that a turn takes time and a drawing can come
  // between two turns.
  describe("outside act", () => {
    let root: Root;
    let element: HTMLDivElement;
    const flag = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean };
    beforeEach(() => {
      flag.IS_REACT_ACT_ENVIRONMENT = false;
      element = document.createElement("div");
      document.body.append(element);
      root = createRoot(element);
    });
    afterEach(() => {
      root.unmount();
      element.remove();
      flag.IS_REACT_ACT_ENVIRONMENT = true;
    });

    /** A list whose every drawing takes 12 ms, as 754 tools do on a slow Mac. */
    function Slow({ stage, seen }: { stage: string; seen: ToolsInTurn[] }) {
      const turn = useToolsInTurn(754, 1, stage);
      seen.push(turn);
      const started = performance.now();
      while (performance.now() - started < 12) {
        // a drawing that takes time
      }
      return <p>{`${turn.drawn} ${turn.held}`}</p>;
    }

    it("draws every tool when the plans come back while the first stage is still drawing", async () => {
      const seen: ToolsInTurn[] = [];
      root.render(<Slow stage="planning" seen={seen} />);
      await new Promise((resolve) => setTimeout(resolve, 50));
      root.render(<Slow stage="planned" seen={seen} />);
      await waitFor(() => expect(element.textContent).toBe("754 0"), { timeout: 5000 });
      // It did come mid-way: tools held as the first stage drew them, not all 754.
      expect(seen.some((turn) => turn.held > 0 && turn.held < 754 && turn.drawn < 754)).toBe(true);
    });
  });
});

describe("SheetLines", () => {
  it("keeps a line's ⓘ on one line with its last word, so it never wraps alone", () => {
    const english = "This can't be cancelled once it starts. Don't quit Banager or shut down your Mac until it finishes.";
    const chinese = "开始后无法取消。完成前请不要退出Banager或关机。";
    const { container } = renderWithProviders(
      <SheetLines
        lines={[
          { text: english, detail: "Wait for the result.", caution: true },
          { text: chinese, detail: "等结果出来。", caution: false },
          { text: "Nothing more to say.", detail: null, caution: false },
        ]}
      />,
    );
    const tails = [...container.querySelectorAll<HTMLElement>("[data-info-tail]")];
    expect(tails).toHaveLength(2);
    for (const tail of tails) {
      expect(tail.className.split(" ")).toContain("whitespace-nowrap");
      expect(tail.querySelector("button")).not.toBeNull();
    }
    // The last word with its full stop; in Chinese, which wraps between
    // any two characters, the last character with its 。.
    expect(tails[0].textContent?.trim()).toBe("finishes.");
    expect(tails[1].textContent?.trim()).toBe("机。");
    // The whole sentence is still the line's, in order.
    const lines = [...container.querySelectorAll("li")].map((line) => line.textContent?.trim());
    expect(lines).toEqual([english, chinese, "Nothing more to say."]);
    // A line with no why has no ⓘ to hold.
    expect(container.querySelectorAll("li")[2].querySelector("[data-info-tail]")).toBeNull();
  });

  it("keeps a refusal's ⓘ with its last word too", () => {
    const { container } = renderWithProviders(
      <Refusal text="Couldn't prepare the update." detail="brew said no" detailTitle="Couldn't prepare the update." />,
    );
    const tail = container.querySelector<HTMLElement>("[data-info-tail]");
    expect(tail?.textContent?.trim()).toBe("update.");
    expect(tail?.className.split(" ")).toContain("whitespace-nowrap");
    expect(screen.getByRole("alert").textContent?.trim()).toBe("Couldn't prepare the update.");
  });
});

describe("SheetToolList", () => {
  const listLines = [
    "[&>*+*]:relative",
    "[&>*+*]:before:absolute",
    "[&>*+*]:before:left-10.5",
    "[&>*+*]:before:right-2.5",
    "[&>*+*]:before:top-0",
    "[&>*+*]:before:h-px",
    "[&>*+*]:before:bg-group-separator",
  ];
  const rowLines = listLines.map((line) => line.slice("[&>*+*]:".length));
  const tools = (separated: boolean) =>
    ["glib", "jq", "wget"].map((name, index) => (
      <SheetTool key={name} adapterId="brew" sourceLabel="Homebrew" name={name} separated={separated && index > 0} />
    ));

  it("parts its tools itself with a hairline over every one after the first, unless they draw their own", () => {
    const { container, unmount } = renderWithProviders(<SheetToolList label="Tools">{tools(false)}</SheetToolList>);
    const list = container.querySelector("[data-sheet-tools]")!;
    expect([...list.classList]).toEqual(expect.arrayContaining(listLines));
    for (const row of list.querySelectorAll("li")) {
      expect(rowLines.filter((line) => row.classList.contains(line))).toEqual([]);
    }
    unmount();

    // `rowsSeparate`: the same lines, drawn by the tools after the first.
    const own = renderWithProviders(
      <SheetToolList label="Tools" rowsSeparate>
        {tools(true)}
      </SheetToolList>,
    );
    const ownList = own.container.querySelector("[data-sheet-tools]")!;
    expect(listLines.filter((line) => ownList.classList.contains(line))).toEqual([]);
    expect([...ownList.querySelectorAll("li")].map((row) => rowLines.every((line) => row.classList.contains(line)))).toEqual([
      false,
      true,
      true,
    ]);
  });
});

describe("SheetTool", () => {
  it("shows the name the list shows where it is given, the whole name its tooltip and for a screen reader", () => {
    const whole = "modelscope.cn/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M";
    const { container } = renderWithProviders(
      <ul>
        <SheetTool adapterId="ollama" sourceLabel="Ollama" name={whole} shownName="Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M" />
      </ul>,
    );
    const name = container.querySelector("[data-sheet-name]") as HTMLElement;
    expect(name).toHaveAttribute("title", whole);
    const [shown, spoken] = [...name.children] as HTMLElement[];
    expect(shown.textContent).toBe("Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M");
    expect(shown).toHaveAttribute("aria-hidden", "true");
    expect(spoken.textContent).toBe(whole);
    expect(spoken.className).toBe("sr-only");
  });
});
