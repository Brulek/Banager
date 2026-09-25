import { describe, expect, it } from "vitest";
import { warningArgs, warningKey, warningMessage, warningText, warningTexts } from "./warnings";
import type { Warning } from "./types";

/** A stub `t`: returns the key with its interpolations inlined, which is
 *  enough to prove `warningText` looked the right key up with the right
 *  values, without coupling this test to the actual English copy. */
function fakeT(key: string, options?: Record<string, unknown>): string {
  return options && Object.keys(options).length > 0 ? `${key}(${JSON.stringify(options)})` : key;
}

describe("warningKey", () => {
  it("gives each fixed warning its own key", () => {
    expect(warningKey("DependentsUnknown")).toBe("warnings.dependentsUnknown");
    expect(warningKey("CompilesLocally")).toBe("warnings.compilesLocally");
    expect(warningKey("NonRegistrySource")).toBe("warnings.nonRegistrySource");
    expect(warningKey({ WouldBreak: { names: ["python@3.13"] } })).toBe("warnings.wouldBreak");
    expect(warningKey({ ThirdPartyRegistry: { host: "modelscope.cn" } })).toBe(
      "warnings.thirdPartyRegistry",
    );
    // A path-list uninstall's items: the key is chosen by what the path
    // is, so each kind can have its own parenthesis.
    expect(warningKey({ WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } })).toBe(
      "warnings.willTrash.Launcher",
    );
    expect(warningKey({ WillTrash: { path: "~/.local/share/claude", what: "Program" } })).toBe(
      "warnings.willTrash.Program",
    );
    expect(warningKey({ WillTrash: { path: "~/.claude/downloads", what: "Cache" } })).toBe(
      "warnings.willTrash.Cache",
    );
    expect(warningKey({ WillKeep: { path: "~/.claude.json", what: "Settings" } })).toBe(
      "warnings.willKeep.Settings",
    );
    expect(warningKey({ WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } })).toBe(
      "warnings.willKeep.SettingsAndHistory",
    );
    expect(warningKey({ AlreadyGone: { path: "~/.local/share/claude" } })).toBe(
      "warnings.alreadyGone",
    );
  });

  it("gives each of rustup's uninstall warnings its key, and two of them a second key by payload", () => {
    // No toolchain names (the toolchains directory is missing or empty):
    // the sentence must not read "every toolchain ()" -- it drops the
    // parenthesis instead (spec §6.5). A startup-file line rustup will
    // not remove is "will print an error" only when it is one of the
    // sourcing forms rustup itself writes; any other mention is "may".
    expect(
      warningKey({ RemovesToolchains: { path: "~/.rustup", names: ["stable-aarch64-apple-darwin"] } }),
    ).toBe("warnings.removesToolchains");
    expect(warningKey({ RemovesToolchains: { path: "~/.rustup", names: [] } })).toBe(
      "warnings.removesToolchainsUnlisted",
    );
    expect(warningKey({ DeletesCargoHome: { path: "~/.cargo" } })).toBe("warnings.deletesCargoHome");
    expect(warningKey({ RemovesCargoInstalled: { names: ["hexyl"] } })).toBe(
      "warnings.removesCargoInstalled",
    );
    expect(warningKey("HomebrewRustupLosesToolchains")).toBe("warnings.homebrewRustupLosesToolchains");
    expect(warningKey("EditsShellConfig")).toBe("warnings.editsShellConfig");
    expect(warningKey({ LeavesShellConfigLine: { path: "~/.zshrc", certain: true } })).toBe(
      "warnings.leavesShellConfigLine",
    );
    expect(warningKey({ LeavesShellConfigLine: { path: "~/.zshrc", certain: false } })).toBe(
      "warnings.leavesShellConfigLineMaybe",
    );
  });

  it("has no key for a Message -- its text comes from the wire, not i18n", () => {
    expect(warningKey({ Message: "boom" })).toBeNull();
  });

  it("is null for Message and for nothing else", () => {
    // The runtime half of what `tsc` checks at compile time: every
    // variant of `Warning` is one of these fifteen, and the only one
    // without a `warnings.*` key is the raw-text catch-all. A variant this
    // list does not name is a `never` in `warningKey`'s default branches
    // and does not compile, so there is no "unrecognised variant" to test.
    const all: Warning[] = [
      "DependentsUnknown",
      "CompilesLocally",
      "NonRegistrySource",
      { WouldBreak: { names: ["a"] } },
      { ThirdPartyRegistry: { host: "modelscope.cn" } },
      { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } },
      { WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } },
      { AlreadyGone: { path: "~/.local/share/claude" } },
      { RemovesToolchains: { path: "~/.rustup", names: ["stable-aarch64-apple-darwin"] } },
      { DeletesCargoHome: { path: "~/.cargo" } },
      { RemovesCargoInstalled: { names: ["hexyl"] } },
      "HomebrewRustupLosesToolchains",
      "EditsShellConfig",
      { LeavesShellConfigLine: { path: "~/.zshrc", certain: true } },
      { Message: "boom" },
    ];
    const keyless = all.filter((warning) => warningKey(warning) === null);
    expect(keyless).toEqual([{ Message: "boom" }]);
  });
});

describe("warningArgs", () => {
  it("interpolates WouldBreak's names and count for pluralisation", () => {
    expect(warningArgs({ WouldBreak: { names: ["python@3.13"] } })).toEqual({
      count: 1,
      names: "python@3.13",
    });
    expect(warningArgs({ WouldBreak: { names: ["a", "b"] } })).toEqual({
      count: 2,
      names: "a, b",
    });
  });

  it("interpolates the registry host so the copy can name it in either language", () => {
    // The sentence used to be assembled in Rust, in English, and shown
    // verbatim -- including above the Uninstall button, to a zh-CN user
    // pulling from modelscope.cn.
    expect(warningArgs({ ThirdPartyRegistry: { host: "modelscope.cn" } })).toEqual({
      host: "modelscope.cn",
    });
  });

  it("interpolates rustup's two folders, the toolchain names, the cargo-installed programs with a count, and the startup file", () => {
    expect(
      warningArgs({
        RemovesToolchains: {
          path: "~/.rustup",
          names: ["stable-aarch64-apple-darwin", "nightly-aarch64-apple-darwin"],
        },
      }),
    ).toEqual({ path: "~/.rustup", names: "stable-aarch64-apple-darwin, nightly-aarch64-apple-darwin" });
    // No names: the unlisted key has no names slot, only the path.
    expect(warningArgs({ RemovesToolchains: { path: "~/.rustup", names: [] } })).toEqual({
      path: "~/.rustup",
    });
    expect(warningArgs({ DeletesCargoHome: { path: "~/.cargo" } })).toEqual({ path: "~/.cargo" });
    expect(warningArgs({ RemovesCargoInstalled: { names: ["hexyl"] } })).toEqual({
      count: 1,
      names: "hexyl",
    });
    expect(warningArgs({ RemovesCargoInstalled: { names: ["hexyl", "rg"] } })).toEqual({
      count: 2,
      names: "hexyl, rg",
    });
    expect(warningArgs({ LeavesShellConfigLine: { path: "~/.zshrc", certain: false } })).toEqual({
      path: "~/.zshrc",
    });
    expect(warningArgs("HomebrewRustupLosesToolchains")).toEqual({});
    expect(warningArgs("EditsShellConfig")).toEqual({});
  });

  it("interpolates the path a trash, keep or already-gone item names", () => {
    // The path arrives with `$HOME` already abbreviated to `~` on the Rust
    // side (`scan::display_path`): data for the sentence, not a path to
    // act on.
    expect(warningArgs({ WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } })).toEqual({
      path: "~/.local/bin/claude",
    });
    expect(warningArgs({ WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } })).toEqual({
      path: "~/.claude",
    });
    expect(warningArgs({ AlreadyGone: { path: "~/.local/share/claude" } })).toEqual({
      path: "~/.local/share/claude",
    });
  });

  it("is empty for every other variant", () => {
    expect(warningArgs("DependentsUnknown")).toEqual({});
    expect(warningArgs("CompilesLocally")).toEqual({});
    expect(warningArgs("NonRegistrySource")).toEqual({});
    expect(warningArgs({ Message: "boom" })).toEqual({});
  });
});

describe("warningMessage", () => {
  it("reads a Message's text straight off the wire", () => {
    expect(warningMessage({ Message: "installed from git, cannot check crates.io" })).toBe(
      "installed from git, cannot check crates.io",
    );
  });

  it("is null for every fixed, key-driven variant", () => {
    expect(warningMessage("DependentsUnknown")).toBeNull();
    expect(warningMessage("CompilesLocally")).toBeNull();
    expect(warningMessage("NonRegistrySource")).toBeNull();
    expect(warningMessage({ WouldBreak: { names: ["a"] } })).toBeNull();
    expect(warningMessage({ ThirdPartyRegistry: { host: "modelscope.cn" } })).toBeNull();
  });
});

describe("warningText", () => {
  it("looks a fixed warning up through t(), with its args", () => {
    expect(warningText(fakeT, "DependentsUnknown")).toBe("warnings.dependentsUnknown");
    expect(warningText(fakeT, { WouldBreak: { names: ["a", "b"] } })).toBe(
      'warnings.wouldBreak({"count":2,"names":"a, b"})',
    );
  });

  it("reads a Message's text directly, bypassing t()", () => {
    expect(warningText(fakeT, { Message: "boom" })).toBe("boom");
  });
});

describe("warningTexts", () => {
  it("renders every warning in order, Message included", () => {
    const warnings: Warning[] = [
      "DependentsUnknown",
      { Message: "boom" },
      { WouldBreak: { names: ["a"] } },
    ];
    expect(warningTexts(fakeT, warnings)).toEqual([
      "warnings.dependentsUnknown",
      "boom",
      'warnings.wouldBreak({"count":1,"names":"a"})',
    ]);
  });

  it("is empty for an empty list", () => {
    expect(warningTexts(fakeT, [])).toEqual([]);
  });
});
