import { describe, expect, it } from "vitest";
import { renderWithProviders } from "../test/setup";
import { CommandPreview } from "./CommandPreview";

describe("CommandPreview", () => {
  it("renders the joined program and arguments under a label", () => {
    const { getByText } = renderWithProviders(
      <CommandPreview
        action={{
          Command: { program: "/opt/homebrew/bin/brew", args: ["upgrade", "--cask", "onyx"], env: [] },
        }}
      />,
    );

    expect(getByText("This will run:")).toBeInTheDocument();
    expect(getByText("/opt/homebrew/bin/brew upgrade --cask onyx")).toBeInTheDocument();
  });

  it("quotes tokens that contain whitespace so argument boundaries stay visible", () => {
    const { getByText } = renderWithProviders(
      <CommandPreview
        action={{
          Command: { program: "/Users/Alice Smith/bin/brew", args: ["upgrade", "--cask", "onyx"], env: [] },
        }}
      />,
    );

    expect(getByText("'/Users/Alice Smith/bin/brew' upgrade --cask onyx")).toBeInTheDocument();
  });

  it("says Canager moves the listed items itself, counted, when the plan runs no command", () => {
    // A path-list uninstall (spec §6.2): no argv exists, so the honest
    // preview is a sentence -- what Canager will do, that no command
    // runs, and that nothing is deleted -- under a label of its own, never
    // "This will run:". The items themselves are the dialog's `WillTrash`
    // list above it. No <code>: there is nothing to copy into a terminal.
    const { getByText, queryByText, container } = renderWithProviders(
      <CommandPreview
        action={{
          TrashPaths: {
            paths: [
              "/Users/someone/.local/share/claude",
              "/Users/someone/.claude/downloads",
              "/Users/someone/.local/bin/claude",
            ],
          },
        }}
      />,
    );

    expect(getByText("What Canager will do:")).toBeInTheDocument();
    expect(queryByText("This will run:")).toBeNull();
    expect(
      getByText(
        "Canager moves the 3 items listed above to the Trash itself — no command runs, and nothing is deleted: until you empty the Trash you can drag them back out, and Finder's Put Back will likely work too.",
      ),
    ).toBeInTheDocument();
    expect(container.querySelector("code")).toBeNull();
  });

  it("uses the singular sentence for one item", () => {
    const { getByText } = renderWithProviders(
      <CommandPreview action={{ TrashPaths: { paths: ["/Users/someone/.local/bin/claude"] } }} />,
    );

    expect(
      getByText(
        "Canager moves the 1 item listed above to the Trash itself — no command runs, and nothing is deleted: until you empty the Trash you can drag it back out, and Finder's Put Back will likely work too.",
      ),
    ).toBeInTheDocument();
  });
});
