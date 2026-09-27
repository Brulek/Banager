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
        title="Canager found nothing installed"
        description="Tools you install with Homebrew, npm and the like show up here."
        detail={{
          label: "Details",
          ariaLabel: "Details: Canager found nothing installed",
          content: "Canager works with Homebrew, npm, pipx, uv, pip, Cargo and Ollama.",
        }}
      />,
    );

    expect(screen.queryByText("Canager works with Homebrew, npm, pipx, uv, pip, Cargo and Ollama.")).toBeNull();
    const details = screen.getByRole("button", { name: "Details: Canager found nothing installed" });
    expect(details).toHaveTextContent("Details");
    fireEvent.click(details);
    expect(screen.getByText("Canager works with Homebrew, npm, pipx, uv, pip, Cargo and Ollama.")).toBeInTheDocument();
  });

  it("renders as a status banner when variant is 'banner'", () => {
    renderWithProviders(
      <EmptyState
        title="Some data might be out of date"
        description="The last refresh couldn't finish for 1 source, so what you see below may be stale."
        variant="banner"
      />,
    );

    expect(screen.getByRole("status")).toBeInTheDocument();
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
