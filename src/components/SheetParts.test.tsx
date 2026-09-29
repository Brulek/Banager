import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { SheetText, sheetMeta } from "./SheetParts";

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
