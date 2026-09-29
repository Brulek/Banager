import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { SheetText, sheetMeta, TOOLS_DRAWN_FIRST, useToolsInTurn } from "./SheetParts";

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
  /** A dialog's list of `count` tools, saying how many it draws each time it is drawn. */
  function List({ count, batch, drawn }: { count: number; batch: number | null; drawn: number[] }) {
    const shown = useToolsInTurn(count, batch);
    drawn.push(shown);
    return <p data-testid="shown">{shown}</p>;
  }

  it("draws the first few of a long list with the dialog and the rest just after, starting over for a new batch", async () => {
    // As many as a 320 high list shows at 36 a tool, and no fewer.
    expect(TOOLS_DRAWN_FIRST * 36).toBeGreaterThanOrEqual(320);
    expect((TOOLS_DRAWN_FIRST - 1) * 36).toBeLessThan(320);
    const drawn: number[] = [];
    const { rerender } = render(<List count={121} batch={1} drawn={drawn} />);
    expect(drawn[0]).toBe(TOOLS_DRAWN_FIRST);
    await waitFor(() => expect(screen.getByTestId("shown")).toHaveTextContent("121"));

    // Update all again: a new batch, from the first few.
    drawn.length = 0;
    rerender(<List count={121} batch={2} drawn={drawn} />);
    expect(drawn[0]).toBe(TOOLS_DRAWN_FIRST);
    await waitFor(() => expect(screen.getByTestId("shown")).toHaveTextContent("121"));

    // A list that fits is drawn whole at once, and none while there is no batch.
    drawn.length = 0;
    rerender(<List count={5} batch={3} drawn={drawn} />);
    expect(new Set(drawn)).toEqual(new Set([5]));
    rerender(<List count={0} batch={null} drawn={drawn} />);
    expect(screen.getByTestId("shown")).toHaveTextContent("0");
  });
});
