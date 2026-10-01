import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, waitFor, within } from "@testing-library/react";
import i18n from "../i18n";
import { renderWithProviders } from "../test/setup";
import { twinsByArtifact } from "../lib/commands";
import type { ArtifactKey, CommandFact, InstalledArtifact } from "../lib/types";
import { NO_FACTS } from "../lib/types";
import { CommandsGroup, twinChip } from "./CommandFacts";
import { BUTTON } from "./ui/controls";

const npmKey: ArtifactKey = { instance_id: "npm:/opt/homebrew", kind: "Package", name: "@anthropic-ai/claude-code" };
const nativeKey: ArtifactKey = { instance_id: "standalone-claude", kind: "Binary", name: "claude" };
const formulaKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "grok" };

const LABELS: Record<string, string> = {
  "npm:/opt/homebrew": "npm",
  "standalone-claude": "Claude Code",
  "brew:/opt/homebrew": "Homebrew",
};
const sourceLabelFor = (instanceId: string) => LABELS[instanceId] ?? instanceId;

function artifact(key: ArtifactKey, family: string | null, commands: CommandFact[], name = key.name): InstalledArtifact {
  return {
    key,
    display_name: name,
    version: "1.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: { ...NO_FACTS, family, commands },
  };
}

function group(subject: InstalledArtifact, others: InstalledArtifact[] = []) {
  return renderWithProviders(
    <CommandsGroup artifact={subject} artifacts={[subject, ...others]} sourceLabelFor={sourceLabelFor} />,
  );
}

/** Each line of the group: its commands, then what typing them runs (less the space before an ⓘ). */
function lines(container: HTMLElement): string[][] {
  return [...container.querySelectorAll("[data-command-line]")].map((line) =>
    [...line.querySelectorAll("p")].map((p) => (p.textContent ?? "").trim()),
  );
}

describe("CommandsGroup", () => {
  it("says nothing for a tool with no verdict about any of its commands", () => {
    const { container } = group(artifact(formulaKey, null, [{ name: "curl", state: null }]));
    expect(container).toBeEmptyDOMElement();
    const { container: none } = group(artifact(formulaKey, null, []));
    expect(none).toBeEmptyDOMElement();
  });

  it("titles the group with what the verdicts are judged against behind its ⓘ", () => {
    const { getByRole, getByText } = group(artifact(nativeKey, "claude-code", [{ name: "claude", state: "Runs" }]));
    expect(getByRole("heading", { name: /Typed in Terminal/ })).toBeInTheDocument();
    fireEvent.click(getByRole("button", { name: "Details: Typed in Terminal" }));
    expect(
      getByText(
        "Based on the Terminal settings read when this app opened. An alias, a new Terminal window or an editor's terminal may differ.",
      ),
    ).toBeInTheDocument();
  });

  it("says which copy runs: this one, the other copy of the tool, or another program", () => {
    const npm = artifact(npmKey, "claude-code", [{ name: "claude", state: "Runs" }]);
    const native = artifact(nativeKey, "claude-code", [
      { name: "agent", state: { ShadowedBy: { by: formulaKey } } },
      { name: "claude", state: { ShadowedBy: { by: npmKey } } },
      { name: "claw", state: { ShadowedBy: { by: null } } },
    ]);
    const formula = artifact(formulaKey, null, [{ name: "agent", state: "Runs" }]);
    const { container, getByRole } = group(native, [npm, formula]);
    expect(lines(container)).toEqual([
      ["agent", "Runs another program with this name, from Homebrew"],
      ["claude", "Runs the copy from npm"],
      ["claw", "Runs another program with this name"],
    ]);
    // Why behind an ⓘ: the other one is found first.
    fireEvent.click(getByRole("button", { name: "Details: claude" }));
    expect(container.ownerDocument.body).toHaveTextContent("Terminal finds that one first. This copy comes after it.");
    expect(within(container).queryByRole("button", { name: /Copy path/ })).toBeNull();
  });

  it("says this copy runs, once for commands with one verdict, counting the names past three", () => {
    const names = ["cargo", "cargo-clippy", "cargo-fmt", "rustc", "rustup"];
    const { container } = group(
      artifact(
        { instance_id: "standalone-rustup", kind: "Binary", name: "rustup" },
        null,
        names.map((name) => ({ name, state: "Runs" as const })),
      ),
    );
    expect(lines(container)).toEqual([["cargo, cargo-clippy, cargo-fmt and 2 more", "Runs this copy"]]);
  });

  describe("a folder Terminal does not search", () => {
    let writeText: ReturnType<typeof vi.fn>;

    beforeEach(() => {
      writeText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    });

    afterEach(() => {
      Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
    });

    it("names the folder and copies it as shown, with nothing else to press", async () => {
      const { container, getByRole } = group(
        artifact(nativeKey, "claude-code", [{ name: "claude", state: { NotOnPath: { dir: "~/.local/bin" } } }]),
      );
      expect(lines(container)).toEqual([
        ["claude", "Terminal can't find it: it's in ~/.local/bin, a folder Terminal doesn't search"],
      ]);
      const copy = getByRole("button", { name: "Copy path: ~/.local/bin" });
      expect(copy).toHaveTextContent(/^Copy Path$/);
      expect(copy.className).toBe(BUTTON.small.grey);
      expect(container.querySelectorAll("button")).toHaveLength(2);
      fireEvent.click(copy);
      expect(writeText).toHaveBeenCalledWith("~/.local/bin");
      await waitFor(() => expect(getByRole("status")).toHaveTextContent(/^Copied$/));
      // Beside the button it is about, as the homepage's Copy Link says it.
      expect(getByRole("status").parentElement).toBe(copy.parentElement);
    });

    it("says it in Chinese as Apple's strings do", async () => {
      await i18n.changeLanguage("zh-CN");
      try {
        const { container, getByRole } = group(
          artifact(nativeKey, "claude-code", [
            { name: "claude", state: { NotOnPath: { dir: "~/.local/bin" } } },
            { name: "claude-helper", state: "Runs" },
          ]),
        );
        expect(getByRole("heading", { name: /在终端里输入/ })).toBeInTheDocument();
        expect(lines(container)).toEqual([
          ["claude", "终端找不到它：它在~/.local/bin，这个文件夹不在终端的搜索路径里"],
          ["claude-helper", "运行的是这一份"],
        ]);
        expect(getByRole("button", { name: "拷贝路径：~/.local/bin" })).toHaveTextContent(/^拷贝路径$/);
      } finally {
        await i18n.changeLanguage("en");
      }
    });
  });
});

describe("twinChip", () => {
  const t = i18n.getFixedT("en");

  /** The chip's detail as text, a line each. */
  function detailText(detail: React.ReactNode): string[] {
    const { container } = render(<>{detail}</>);
    return [...container.querySelectorAll("p")].map((p) => p.textContent ?? "");
  }

  it("marks a tool another source installed too, and says which copy typing it runs", () => {
    const npm = artifact(npmKey, "claude-code", [{ name: "claude", state: "Runs" }], "@anthropic-ai/claude-code");
    const native = artifact(
      nativeKey,
      "claude-code",
      [{ name: "claude", state: { ShadowedBy: { by: npmKey } } }],
      "Claude Code",
    );
    const twins = twinsByArtifact([npm, native]);

    const onNative = twinChip(t, native, twins.get("standalone-claude|Binary|claude"), sourceLabelFor);
    expect(onNative?.label).toBe("Installed twice");
    expect(onNative?.ariaLabel).toBe("Installed twice: Claude Code");
    expect(onNative?.tone).toBe("neutral");
    expect(detailText(onNative?.detail)).toEqual([
      "npm has a copy too.",
      "Typing claude in Terminal runs the copy from npm.",
    ]);

    const onNpm = twinChip(t, npm, twins.get("npm:/opt/homebrew|Package|@anthropic-ai/claude-code"), sourceLabelFor);
    expect(detailText(onNpm?.detail)).toEqual([
      "Claude Code's own installer installed a copy too.",
      "Typing claude in Terminal runs this copy.",
    ]);
  });

  it("counts a third copy, and says only where the others are when nothing was judged", () => {
    const caskKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "claude-code" };
    const npm = artifact(npmKey, "claude-code", [{ name: "claude", state: null }]);
    const native = artifact(nativeKey, "claude-code", [{ name: "claude", state: null }], "Claude Code");
    const cask = artifact(caskKey, "claude-code", [{ name: "claude", state: null }]);
    const twins = twinsByArtifact([npm, native, cask]);
    const chip = twinChip(t, native, twins.get("standalone-claude|Binary|claude"), sourceLabelFor);
    expect(chip?.label).toBe("Installed 3 times");
    expect(detailText(chip?.detail)).toEqual(["npm and Homebrew each have a copy too."]);
  });

  it("says this copy cannot be found where its folder is off the search path", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      const zh = i18n.getFixedT("zh-CN");
      const npm = artifact(npmKey, "claude-code", [{ name: "claude", state: "Runs" }]);
      const native = artifact(
        nativeKey,
        "claude-code",
        [{ name: "claude", state: { NotOnPath: { dir: "~/.local/bin" } } }],
        "Claude Code",
      );
      const twins = twinsByArtifact([npm, native]);
      const chip = twinChip(zh, native, twins.get("standalone-claude|Binary|claude"), sourceLabelFor);
      expect(chip?.label).toBe("装了两份");
      expect(chip?.ariaLabel).toBe("装了两份：Claude Code");
      expect(detailText(chip?.detail)).toEqual(["npm也装了一份。", "在终端里输入“claude”，找不到这一份。"]);
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("counts copies, not sources, where one source has two", async () => {
    const otherNpmKey: ArtifactKey = { ...npmKey, instance_id: "npm:/Users/a/.npm-global" };
    const native = artifact(nativeKey, "claude-code", [{ name: "claude", state: "Runs" }], "Claude Code");
    const npm = artifact(npmKey, "claude-code", [{ name: "claude", state: null }]);
    const otherNpm = artifact(otherNpmKey, "claude-code", [{ name: "claude", state: null }]);
    const labels = (id: string) => (id.startsWith("npm:") ? "npm" : sourceLabelFor(id));
    const twins = twinsByArtifact([native, npm, otherNpm]);
    const chip = twinChip(t, native, twins.get("standalone-claude|Binary|claude"), labels);
    expect(chip?.label).toBe("Installed 3 times");
    expect(detailText(chip?.detail)).toEqual([
      "The other 2 copies were installed by npm.",
      "Typing claude in Terminal runs this copy.",
    ]);
    await i18n.changeLanguage("zh-CN");
    try {
      const zh = i18n.getFixedT("zh-CN");
      const zhChip = twinChip(zh, native, twins.get("standalone-claude|Binary|claude"), labels);
      expect(detailText(zhChip?.detail)).toEqual(["另外2份由npm安装。", "在终端里输入“claude”，运行的是这一份。"]);
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("names no command when the shared commands do not all run the same copy", () => {
    const rustupKey: ArtifactKey = { instance_id: "standalone-rustup", kind: "Binary", name: "rustup" };
    const rustKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "rust" };
    const rustup = artifact(rustupKey, "rust", [
      { name: "cargo", state: { ShadowedBy: { by: rustKey } } },
      { name: "rustc", state: "Runs" },
    ]);
    const rust = artifact(rustKey, "rust", [
      { name: "cargo", state: "Runs" },
      { name: "rustc", state: null },
    ]);
    const twins = twinsByArtifact([rustup, rust]);
    const chip = twinChip(t, rustup, twins.get("standalone-rustup|Binary|rustup"), sourceLabelFor);
    expect(detailText(chip?.detail)).toEqual(["Homebrew has a copy too."]);
  });

  it("is no word at all for a tool with no other copy", () => {
    const native = artifact(nativeKey, "claude-code", [{ name: "claude", state: "Runs" }]);
    expect(twinChip(t, native, undefined, sourceLabelFor)).toBeNull();
    expect(twinChip(t, native, [], sourceLabelFor)).toBeNull();
  });
});
