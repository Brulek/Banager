import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";
import type { PlanAction, Settings } from "../lib/types";
import { CommandPreview } from "./CommandPreview";

const brewUpgrade: PlanAction = {
  Command: { program: "/opt/homebrew/bin/brew", args: ["upgrade", "--cask", "onyx"], env: [] },
};

let settings: Settings;

beforeEach(() => {
  settings = {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    skipped_versions: [],
    include_self_updating: false,
    auto_check: false,
    notify_updates: false,
  };
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => (cmd === "get_settings" ? settings : undefined));
});

describe("CommandPreview", () => {
  it("keeps the command behind one press, closed to begin with", async () => {
    renderWithProviders(<CommandPreview plans={[{ id: "1", action: brewUpgrade }]} />);

    const disclosure = screen.getByRole("button", { name: "Show Command" });
    // Settled: the setting has arrived, and it is off.
    await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith("get_settings"));
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("/opt/homebrew/bin/brew upgrade --cask onyx")).toBeNull();

    await userEvent.setup().click(disclosure);

    expect(disclosure).toHaveAttribute("aria-expanded", "true");
    const command = screen.getByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    expect(command.tagName).toBe("CODE");
    // The button names what it opens.
    expect(document.getElementById(disclosure.getAttribute("aria-controls") ?? "")).toContainElement(command);
  });

  it("is a disclosure row: a 10 triangle and 13 muted words, the triangle turned down while open", async () => {
    renderWithProviders(<CommandPreview plans={[{ id: "1", action: brewUpgrade }]} />);

    const disclosure = screen.getByRole("button", { name: "Show Command" });
    expect(disclosure).toHaveClass("text-body", "text-muted");
    // Not a link: no accent.
    expect(disclosure.className).not.toMatch(/accent/);
    const triangle = disclosure.querySelector("svg") as SVGElement;
    expect(triangle).toHaveAttribute("width", "10");
    expect(triangle.getAttribute("class")).not.toMatch(/rotate-90/);

    await userEvent.setup().click(disclosure);
    expect(triangle.getAttribute("class")).toMatch(/rotate-90/);
  });

  it("opens and closes from the keyboard", async () => {
    const user = userEvent.setup();
    renderWithProviders(<CommandPreview plans={[{ id: "1", action: brewUpgrade }]} />);

    await user.tab();
    const disclosure = screen.getByRole("button", { name: "Show Command" });
    expect(document.activeElement).toBe(disclosure);

    await user.keyboard("{Enter}");
    expect(disclosure).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("/opt/homebrew/bin/brew upgrade --cask onyx")).toBeInTheDocument();

    await user.keyboard(" ");
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("/opt/homebrew/bin/brew upgrade --cask onyx")).toBeNull();
  });

  it("is open from the start with Show technical details on, and still closes", async () => {
    settings.show_technical_details = true;
    renderWithProviders(<CommandPreview plans={[{ id: "1", action: brewUpgrade }]} />);

    expect(await screen.findByText("/opt/homebrew/bin/brew upgrade --cask onyx")).toBeInTheDocument();
    const disclosure = screen.getByRole("button", { name: "Show Command" });
    expect(disclosure).toHaveAttribute("aria-expanded", "true");

    await userEvent.setup().click(disclosure);
    expect(screen.queryByText("/opt/homebrew/bin/brew upgrade --cask onyx")).toBeNull();
  });

  it("lets each command be selected, to be copied into Terminal, and not what it is for", async () => {
    settings.show_technical_details = true;
    renderWithProviders(
      <CommandPreview
        plans={[
          { id: "1", name: "OnyX", action: brewUpgrade },
          {
            id: "2",
            name: "rustup",
            action: { Command: { program: "/Users/you/.cargo/bin/rustup", args: ["self", "update"], env: [] } },
          },
        ]}
      />,
    );

    const onyx = await screen.findByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    expect(onyx).toHaveClass("select-text");
    expect(screen.getByText("/Users/you/.cargo/bin/rustup self update")).toHaveClass("select-text");
    expect(screen.getByText("OnyX")).not.toHaveClass("select-text");
    expect(screen.getByRole("button", { name: "Show Commands" })).not.toHaveClass("select-text");
  });

  it("quotes tokens that contain whitespace so argument boundaries stay visible", async () => {
    settings.show_technical_details = true;
    renderWithProviders(
      <CommandPreview
        plans={[
          {
            id: "1",
            action: {
              Command: { program: "/Users/Alice Smith/bin/brew", args: ["upgrade", "--cask", "onyx"], env: [] },
            },
          },
        ]}
      />,
    );

    expect(await screen.findByText("'/Users/Alice Smith/bin/brew' upgrade --cask onyx")).toBeInTheDocument();
  });

  it("shows the variables a plan sets before its program, so Homebrew's autoremove switch is on screen", async () => {
    // `BrewAdapter::ENV` goes on every brew command, and
    // `HOMEBREW_NO_AUTOREMOVE=1` is what keeps `brew uninstall` from also
    // uninstalling what nothing needs any more: pasted into Terminal
    // without it, the same argv does more. A value is quoted as a token is.
    settings.show_technical_details = true;
    renderWithProviders(
      <CommandPreview
        plans={[
          {
            id: "1",
            action: {
              Command: {
                program: "/opt/homebrew/bin/brew",
                args: ["uninstall", "--formula", "jq"],
                env: [
                  ["HOMEBREW_NO_AUTOREMOVE", "1"],
                  ["SUDO_ASKPASS", "/Users/Alice Smith/askpass"],
                ],
              },
            },
          },
        ]}
      />,
    );

    expect(
      await screen.findByText(
        "HOMEBREW_NO_AUTOREMOVE=1 SUDO_ASKPASS='/Users/Alice Smith/askpass' /opt/homebrew/bin/brew uninstall --formula jq",
      ),
    ).toBeInTheDocument();
  });

  it("lists several commands behind one press, each under what it is for", async () => {
    settings.show_technical_details = true;
    renderWithProviders(
      <CommandPreview
        plans={[
          { id: "1", name: "OnyX", action: brewUpgrade },
          {
            id: "2",
            name: "rustup",
            action: { Command: { program: "/Users/you/.cargo/bin/rustup", args: ["self", "update"], env: [] } },
          },
        ]}
      />,
    );

    expect(screen.getAllByRole("button")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Show Commands" })).toBeInTheDocument();
    const rustup = await screen.findByText("/Users/you/.cargo/bin/rustup self update");
    expect(rustup.previousElementSibling).toHaveTextContent("rustup");
    expect(screen.getByText("/opt/homebrew/bin/brew upgrade --cask onyx").previousElementSibling).toHaveTextContent(
      "OnyX",
    );
  });

  it("says only what is sure of a path-list uninstall, in the open, with no command to show", () => {
    // A `TrashPaths` plan runs no command: Banager moves the items itself.
    // T4 of the copy table: they go to the Trash and can be dragged back
    // out -- and no promise of Finder's Put Back, which works as often as
    // not (crates/banager-core/src/trash/mod.rs).
    const { container } = renderWithProviders(
      <CommandPreview
        plans={[
          {
            id: "1",
            action: {
              TrashPaths: {
                paths: [
                  "/Users/someone/.local/share/claude",
                  "/Users/someone/.claude/downloads",
                  "/Users/someone/.local/bin/claude",
                ],
              },
            },
          },
        ]}
      />,
    );

    expect(
      screen.getByText("These 3 items go to the Trash, where you can drag them back out."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button")).toBeNull();
    expect(container.querySelector("code")).toBeNull();
    expect(container.textContent).not.toMatch(/Put Back/);
  });

  it("uses the singular sentence for one item", () => {
    renderWithProviders(
      <CommandPreview plans={[{ id: "1", action: { TrashPaths: { paths: ["/Users/someone/.local/bin/claude"] } } }]} />,
    );

    expect(screen.getByText("This item goes to the Trash, where you can drag it back out.")).toBeInTheDocument();
  });

  it("promises nothing in either language but the Trash and dragging back out of it", () => {
    for (const sentence of [en.uninstall.trashPreview_one, en.uninstall.trashPreview_other]) {
      expect(sentence).not.toMatch(/Put Back|deleted|command/i);
    }
    expect(zhCN.uninstall.trashPreview_other).toBe("这{{count}}项会移到废纸篓，可以从那里拖回来。");
    expect(zhCN.uninstall.trashPreview_other).not.toMatch(/放回原处/);
    expect(zhCN.commandPreview.show_other).toBe("查看命令");
  });
});
