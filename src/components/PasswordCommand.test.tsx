import { describe, expect, it, beforeEach, vi } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { LogDrawer } from "./LogDrawer";
import { PasswordCommand, terminalCommand } from "./PasswordCommand";
import { useUiStore } from "../store/ui";
import i18n from "../i18n";
import type { OpSummary } from "../lib/types";

const mockInvoke = vi.mocked(invoke);

/** What Homebrew prints when a cask's step runs sudo with no terminal. */
const SUDO_LINES = [
  "Error: Failure while executing; `/usr/bin/sudo -u root -E LOGNAME=me USER=me USERNAME=me -- /bin/launchctl bootout system/com.example.helper` exited with 1. Here's the output:",
  "sudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper",
  "sudo: a password is required",
];

/** A cask upgrade that stopped where sudo wanted the Mac's password. */
const passwordOp: OpSummary = {
  id: 1,
  kind: "Upgrade",
  instance_id: "brew:/opt/homebrew",
  artifact_kind: "Cask",
  name: "example",
  status: "Done",
  outcome: { Failed: { exit_code: 1, summary: SUDO_LINES.join("\n") } },
  argv_preview: ["/opt/homebrew/bin/brew", "upgrade", "--cask", "example"],
  env_preview: [
    ["HOMEBREW_NO_AUTO_UPDATE", "1"],
    ["HOMEBREW_NO_AUTOREMOVE", "1"],
    ["HOMEBREW_NO_INSTALL_CLEANUP", "1"],
    ["SUDO_ASKPASS", "/Users/me/bin/ask pass"],
  ],
  cancel_policy: "KillThenReconcile",
};

const EXPECTED_COMMAND =
  "HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_AUTOREMOVE=1 HOMEBREW_NO_INSTALL_CLEANUP=1 /opt/homebrew/bin/brew upgrade --cask example";

let operations: OpSummary[];

beforeEach(() => {
  operations = [passwordOp];
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "list_operations") return Promise.resolve(operations);
    if (cmd === "get_snapshot") return new Promise(() => {});
    return Promise.resolve(undefined);
  });
  useUiStore.setState({ logs: [], drawerOpen: true, focusedOpId: 1 });
});

describe("terminalCommand", () => {
  it("is the command as the confirmation showed it, its variables kept, SUDO_ASKPASS left out", () => {
    expect(terminalCommand(passwordOp)).toBe(EXPECTED_COMMAND);
  });

  it("quotes a token with a space, as the confirmation does", () => {
    expect(
      terminalCommand({ ...passwordOp, argv_preview: ["/Users/Alice Smith/brew", "upgrade"], env_preview: [] }),
    ).toBe("'/Users/Alice Smith/brew' upgrade");
  });

  it("takes a summary without variables as one with none", () => {
    const { env_preview: _, ...withoutEnv } = passwordOp;
    expect(terminalCommand(withoutEnv)).toBe("/opt/homebrew/bin/brew upgrade --cask example");
  });

  it("is nothing for an operation that ran no command", () => {
    expect(terminalCommand({ ...passwordOp, argv_preview: [], env_preview: [] })).toBeNull();
  });
});

describe("PasswordCommand", () => {
  it("shows nothing for a failure with another cause, one still running, one that ran no command, or one not Homebrew's", () => {
    const network = renderWithProviders(
      <PasswordCommand
        op={{ ...passwordOp, outcome: { Failed: { exit_code: 1, summary: "curl: (6) Could not resolve host: ghcr.io" } } }}
      />,
    );
    expect(network.container).toBeEmptyDOMElement();
    network.unmount();

    const running = renderWithProviders(<PasswordCommand op={{ ...passwordOp, status: "Running" }} />);
    expect(running.container).toBeEmptyDOMElement();
    running.unmount();

    const noCommand = renderWithProviders(<PasswordCommand op={{ ...passwordOp, argv_preview: [], env_preview: [] }} />);
    expect(noCommand.container).toBeEmptyDOMElement();
    noCommand.unmount();

    // Only Homebrew's command is handed over: not another source's, whose
    // words these are not, even where its output carried sudo's lines.
    const standalone = renderWithProviders(
      <PasswordCommand
        op={{
          ...passwordOp,
          instance_id: "standalone:claude",
          argv_preview: ["/bin/bash", "-c", "curl -fsSL https://example.com/install.sh | bash"],
          env_preview: [],
        }}
      />,
    );
    expect(standalone.container).toBeEmptyDOMElement();
  });
});

describe("LogDrawer, where sudo wanted a password", () => {
  it("says so, says how to go on in Terminal, and hands over the exact command to copy", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    try {
      const { findByText, findByRole, getByRole } = renderWithProviders(<LogDrawer />);
      act(() => {
        for (const line of SUDO_LINES) useUiStore.getState().appendLog({ opId: 1, stream: "Stderr", line });
      });

      // The cause in the header, as the row and the bar say it; the next step under it.
      expect(await findByText("Needs your password")).toBeInTheDocument();
      expect(await findByText("This step needs your Mac login password, which can't be entered here.")).toBeInTheDocument();
      expect(
        await findByText(
          "You can run the command below in Terminal and type your password when asked. Nothing appears on screen as you type. That's normal.",
        ),
      ).toBeInTheDocument();
      // The command, whole, selectable in one click, and named for what it is.
      const code = getByRole("dialog").querySelector("code") as HTMLElement;
      expect(code.textContent).toBe(EXPECTED_COMMAND);
      expect(code.className).toContain("select-all");
      // The variables on a muted line of their own, the command on the
      // next; a line breaks only between two tokens, never inside one.
      const env = code.querySelector("[data-command-env]") as HTMLElement;
      const argv = code.querySelector("[data-command-argv]") as HTMLElement;
      expect(env).toHaveClass("text-muted");
      expect(argv.textContent).toMatch(/\/brew upgrade --cask /);
      expect(env.textContent?.trim()).toBe(EXPECTED_COMMAND.slice(0, EXPECTED_COMMAND.length - argv.textContent!.length).trim());
      for (const token of argv.querySelectorAll("span")) expect(token).toHaveClass("whitespace-nowrap");
      expect([...argv.querySelectorAll("span")].map((token) => token.textContent)).toContain("--cask");
      expect(getByRole("group", { name: "Command to run in Terminal" })).toContainElement(code);
      expect(await findByText("When it's done, come back here and press ⌘R to check again.")).toBeInTheDocument();

      // Copying it puts exactly that on the clipboard, and says so.
      const copyButton = await findByRole("button", { name: "Copy Command" });
      fireEvent.click(copyButton);
      await waitFor(() => expect(writeText).toHaveBeenCalledWith(EXPECTED_COMMAND));
      expect(within(copyButton.parentElement as HTMLElement).getByRole("status").textContent).toBe("Copied");
      // Copy Log stays in the foot, apart from it.
      const footer = getByRole("dialog").querySelector("[data-dialog-footer]") as HTMLElement;
      expect(within(footer).queryByRole("button", { name: "Copy Command" })).toBeNull();
      // Nothing was run: the only command the drawer sent is the list it reads.
      const reads = new Set(["list_operations", "get_snapshot", "get_settings"]);
      expect(mockInvoke.mock.calls.map(([cmd]) => cmd).filter((cmd) => !reads.has(cmd))).toEqual([]);
    } finally {
      Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
    }
  });

  it("words it all in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText, findByRole } = renderWithProviders(<LogDrawer />);
      expect(await findByText("需要输入密码")).toBeInTheDocument();
      expect(await findByText("这一步需要输入Mac的登录密码，无法在这里输入。")).toBeInTheDocument();
      expect(
        await findByText("可以在终端里运行下面这条命令，按提示输入密码。输入时屏幕上不显示任何字符，这是正常的。"),
      ).toBeInTheDocument();
      expect(await findByText("完成后回到这里，按⌘R重新检查。")).toBeInTheDocument();
      expect(await findByRole("button", { name: "拷贝命令" })).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("where a password window asked and got none, says so, not that it can't be entered here, and still hands over the command", async () => {
    operations = [
      {
        ...passwordOp,
        outcome: { Failed: { exit_code: 1, summary: "sudo: no password was provided\nsudo: a password is required" } },
      },
    ];
    const { findByText, getByRole, queryByText } = renderWithProviders(<LogDrawer />);
    expect(await findByText("Password not accepted")).toBeInTheDocument();
    expect(
      await findByText("Enter your Mac login password in the password window, then try again."),
    ).toBeInTheDocument();
    expect(queryByText(/can't be entered here/)).toBeNull();
    expect(getByRole("dialog").querySelector("code")?.textContent).toBe(EXPECTED_COMMAND);
  });

  it("shows no command for a failure with another cause", async () => {
    operations = [
      { ...passwordOp, outcome: { Failed: { exit_code: 1, summary: 'Error: Failed to download resource "example"' } } },
    ];
    const { findByText, getByRole, queryByRole } = renderWithProviders(<LogDrawer />);
    await findByText("Connection failed");
    expect(getByRole("dialog").querySelector("code")).toBeNull();
    expect(queryByRole("button", { name: "Copy Command" })).toBeNull();
  });
});
