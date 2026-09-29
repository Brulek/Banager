import { describe, expect, it, vi } from "vitest";
import { fireEvent, waitFor } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import { NOTICE_GRID, SourceNotice, SourceNoticeLine } from "./SourceNotice";
import { BUTTON } from "./ui/controls";

describe("SourceNotice", () => {
  it("renders a title and description with no action button when none is given", () => {
    const { getByText, queryByRole } = renderWithProviders(
      <SourceNotice variant="info" title="Read-only" description="Cannot be changed here." />,
    );
    expect(getByText("Read-only")).toBeInTheDocument();
    expect(getByText("Cannot be changed here.")).toBeInTheDocument();
    expect(queryByRole("button")).not.toBeInTheDocument();
  });

  it("renders and fires the action button when one is given", async () => {
    const onClick = vi.fn();
    const { getByRole } = renderWithProviders(
      <SourceNotice variant="warning" title="Not running" action={{ label: "Open it", onClick }} />,
    );
    fireEvent.click(getByRole("button", { name: "Open it" }));
    await waitFor(() => expect(onClick).toHaveBeenCalledTimes(1));
  });
});

describe("SourceNotice's look (spec §3.8)", () => {
  it("draws a warning's line 32 high in one kind of control: a 16pt filled orange ⚠︎, the title, a muted ⓘ, and one small grey button", () => {
    const { container, getByRole, getByText } = renderWithProviders(
      <SourceNoticeLine
        variant="warning"
        title="Ollama isn't running"
        description="Open Ollama to see what it has."
        action={{ label: "Open Ollama", onClick: () => {} }}
        detailsLabel="Details"
        detailsAriaLabel="Details: Ollama isn't running"
      />,
    );
    const line = container.querySelector("[data-notice-line]") as HTMLElement;
    expect(line).toHaveClass("h-8", "items-center", "text-body");
    const icon = line.querySelector("[data-notice-symbol] > svg") as SVGElement;
    expect(icon.getAttribute("width")).toBe("16");
    expect(icon.getAttribute("class")).toContain("text-warning");
    // Filled, with the mark cut out in white: not an outline.
    expect(icon.querySelector("path")?.getAttribute("fill")).toBe("currentColor");
    expect(getByText("Ollama isn't running")).toHaveClass("text-foreground", "truncate");
    // The description behind a muted ⓘ, named for its notice -- no words,
    // no accent: not a link.
    const details = getByRole("button", { name: "Details: Ollama isn't running" });
    expect(details.textContent).toBe("");
    expect(details.querySelector("svg")?.getAttribute("width")).toBe("12");
    expect(details).toHaveClass("text-muted");
    expect(container.innerHTML).not.toContain("text-accent-text");
    expect(details.className).not.toMatch(/underline|\bbg-/);
    fireEvent.click(details);
    expect(document.getElementById(details.getAttribute("aria-controls") ?? "")).toHaveTextContent(
      "Open Ollama to see what it has.",
    );
    // One push button, after the ⓘ.
    const buttons = [...line.querySelectorAll("button")];
    expect(buttons.map((button) => button.getAttribute("aria-label") ?? button.textContent)).toEqual([
      "Details: Ollama isn't running",
      "Open Ollama",
    ]);
    expect(getByRole("button", { name: "Open Ollama" }).className).toContain(BUTTON.small.grey);
    expect(line.querySelectorAll('[class*="bg-fill"]')).toHaveLength(1);
  });

  it("marks information with a 16pt muted ⓘ, not a warning's orange", () => {
    const { container } = renderWithProviders(
      <SourceNoticeLine variant="info" title="npm 12.1.0 not tested" detailsAriaLabel="Details" />,
    );
    const icon = container.querySelector("[data-notice-line] [data-notice-symbol] > svg") as SVGElement;
    expect(icon.getAttribute("width")).toBe("16");
    expect(icon.getAttribute("class")).toContain("text-muted");
    expect(container.innerHTML).not.toContain("text-warning");
  });

  it("lines up with its list's columns: the symbol centred on the avatars', the title where the names start", () => {
    // The Updates page: rows start with a 16 checkbox, 12, a 32 avatar, 12,
    // the name -- 72 past the line's left edge.
    const updates = renderWithProviders(
      <SourceNoticeLine variant="warning" title="uv isn't responding" detailsAriaLabel="Details" grid="checkbox" />,
    );
    const symbol = updates.container.querySelector("[data-notice-symbol]") as HTMLElement;
    // Past the checkbox's 16 and 12, in the avatars' 32: 28 + 32 + 12 = 72.
    expect(symbol.className.split(" ")).toEqual(expect.arrayContaining(["ml-7", "w-8", "justify-center", "shrink-0"]));
    expect(updates.getByText("uv isn't responding").className.split(" ")).toContain("ml-3");
    expect(NOTICE_GRID.checkbox).toEqual({ symbol: "ml-7 w-8", gap: "ml-3", inset: "pl-18", hairline: "left-18" });
    updates.unmount();

    // The Installed and Unknown pages: a 32 avatar, 12, the name -- 44 in.
    const installed = renderWithProviders(
      <SourceNoticeLine
        variant="warning"
        title="uv isn't responding"
        detailsAriaLabel="Details"
        error="Couldn't open Ollama."
      />,
    );
    expect((installed.container.querySelector("[data-notice-symbol]") as HTMLElement).className.split(" ")).toContain("w-8");
    expect(installed.getByText("uv isn't responding").className.split(" ")).toContain("ml-3");
    // A failed press is said under the title.
    expect(installed.getByRole("alert").className.split(" ")).toContain("pl-11");
    expect(NOTICE_GRID.avatar).toEqual({ symbol: "w-8", gap: "ml-3", inset: "pl-11", hairline: "left-11" });
  });

  it("draws a whole notice with no fill and no corners, its description quieter on the line under the title", () => {
    const { container, getByText } = renderWithProviders(
      <SourceNotice variant="warning" title="Not running" description="Open it to check for updates." />,
    );
    const notice = container.firstElementChild as HTMLElement;
    expect(notice.className).not.toMatch(/\bbg-|rounded/);
    expect(getByText("Open it to check for updates.")).toHaveClass("text-small", "text-muted");
    expect(getByText("Not running")).toHaveClass("text-foreground");
    expect(getByText("Not running").className).not.toContain("font-medium");
  });
});
