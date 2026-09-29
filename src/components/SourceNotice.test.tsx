import { describe, expect, it, vi } from "vitest";
import { fireEvent, waitFor } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import { SourceNotice, SourceNoticeLine } from "./SourceNotice";
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
  it("draws a warning's line 32 high: a 16pt filled orange ⚠︎, the title, Details as a link, and a small grey button", () => {
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
    const icon = line.firstElementChild as SVGElement;
    expect(icon.getAttribute("width")).toBe("16");
    expect(icon.getAttribute("class")).toContain("text-warning");
    // Filled, with the mark cut out in white: not an outline.
    expect(icon.querySelector("path")?.getAttribute("fill")).toBe("currentColor");
    expect(getByText("Ollama isn't running")).toHaveClass("text-foreground", "truncate");
    const details = getByRole("button", { name: "Details: Ollama isn't running" });
    expect(details).toHaveClass("text-accent-text");
    expect(details.className).not.toMatch(/underline|bg-/);
    expect(getByRole("button", { name: "Open Ollama" }).className).toContain(BUTTON.small.grey);
  });

  it("marks information with a 16pt muted ⓘ, not a warning's orange", () => {
    const { container } = renderWithProviders(
      <SourceNoticeLine variant="info" title="npm 12.1.0 not tested" detailsLabel="Details" detailsAriaLabel="Details" />,
    );
    const icon = container.querySelector("[data-notice-line] > svg") as SVGElement;
    expect(icon.getAttribute("width")).toBe("16");
    expect(icon.getAttribute("class")).toContain("text-muted");
    expect(container.innerHTML).not.toContain("text-warning");
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
