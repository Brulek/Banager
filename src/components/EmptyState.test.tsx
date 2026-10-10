import { describe, expect, it, vi } from "vitest";
import { screen, fireEvent } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import { EmptyState } from "./EmptyState";

describe("EmptyState", () => {
  it("renders the title, the description and an optional action", () => {
    const onClick = vi.fn();
    renderWithProviders(
      <EmptyState
        title="Nothing installed yet"
        description="Once you install something with Homebrew, it will show up here."
        action={{ label: "Refresh", onClick }}
      />,
    );

    expect(screen.getByText("Nothing installed yet")).toBeInTheDocument();
    expect(
      screen.getByText("Once you install something with Homebrew, it will show up here."),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("keeps more than its one line behind Details, named for what it explains", () => {
    renderWithProviders(
      <EmptyState
        title="Banager found nothing installed"
        description="Tools you install with Homebrew, npm and the like show up here."
        detail={{
          label: "Details",
          ariaLabel: "Details: Banager found nothing installed",
          content: "Banager works with Homebrew, npm, pipx, uv, pip, Cargo and Ollama.",
        }}
      />,
    );

    expect(screen.queryByText("Banager works with Homebrew, npm, pipx, uv, pip, Cargo and Ollama.")).toBeNull();
    const details = screen.getByRole("button", { name: "Details: Banager found nothing installed" });
    expect(details).toHaveTextContent("Details");
    fireEvent.click(details);
    expect(screen.getByText("Banager works with Homebrew, npm, pipx, uv, pip, Cargo and Ollama.")).toBeInTheDocument();
  });

  it("is centred in the list's area: a 36 symbol, 24 to the title, 8 to its sentence, 16 to its one button", () => {
    renderWithProviders(
      <EmptyState
        symbol="check"
        title="Everything is up to date"
        description="Checked 3 min ago"
        action={{ label: "Check Again", onClick: () => {} }}
      />,
    );

    const title = screen.getByText("Everything is up to date");
    const empty = title.closest("[data-empty-state]") as HTMLElement;
    expect(empty).toHaveClass("h-full", "items-center", "justify-center", "text-center");
    const symbol = empty.querySelector("svg") as SVGElement;
    expect(symbol).toHaveAttribute("width", "36");
    expect(symbol.getAttribute("class")).toContain("text-tertiary");
    expect(title).toHaveClass("mt-6", "text-section", "text-muted");
    expect(screen.getByText("Checked 3 min ago")).toHaveClass("mt-2", "text-section", "font-normal", "max-w-90");
    expect(screen.getByRole("button", { name: "Check Again" })).toHaveClass("mt-4", "bg-fill");
  });

  it("draws an ⓘ in a circle unless told there is nothing to do, and never in green", () => {
    const { container, rerender } = renderWithProviders(<EmptyState title="No tools to manage" />);
    const info = container.querySelector("svg") as SVGElement;
    expect(info.querySelector("circle")).not.toBeNull();
    expect(info.innerHTML).toContain("M12 11v5.5");
    rerender(<EmptyState symbol="check" title="Everything is up to date" />);
    expect(container.querySelector("svg")?.innerHTML).toContain("M8 12.4");
    expect(container.innerHTML).not.toContain("text-success");
    // A source that did not answer: a ⚠︎, in the same quiet grey, not orange.
    rerender(<EmptyState symbol="warning" title="uv isn't responding" />);
    const warning = container.querySelector("svg") as SVGElement;
    expect(warning.innerHTML).toContain("M10.3 4.4");
    expect(warning.getAttribute("class")).toContain("text-tertiary");
    expect(container.innerHTML).not.toContain("text-warning");
  });
  it("gives the action a visible button style, not bare text", () => {
    // EmptyState's action is the Retry button of the refresh-failed states
    // (SnapshotStatus), the app's only recovery affordance when the first
    // refresh fails. A class-less <button> under Tailwind preflight is
    // indistinguishable from the sentence above it.
    renderWithProviders(
      <EmptyState title="Couldn't load" description="…" action={{ label: "Try again", onClick: () => {} }} />,
    );

    expect(screen.getByRole("button", { name: "Try again" }).className).not.toBe("");
  });
});
