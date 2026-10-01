import { describe, expect, it } from "vitest";
import {
  deletesForGood,
  isCaution,
  skipsTrash,
  warningArgs,
  warningDetailKey,
  warningGroup,
  warningKey,
  warningLine,
  warningLines,
  warningMessage,
  warningText,
} from "./warnings";
import type { CaskStep, KeptWhat, RemoveCheck, UninstallScope, Warning } from "./types";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";
import i18n from "../i18n";

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
    expect(warningKey("DownloadsModelChanges")).toBe("warnings.downloadsModelChanges");
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
    // Step D's kinds: the backup a self-updater leaves, and the five things
    // the Antigravity and Grok lists keep.
    expect(warningKey({ WillTrash: { path: "~/.local/bin/agy.1727000000.old", what: "Backups" } })).toBe(
      "warnings.willTrash.Backups",
    );
    expect(warningKey({ WillKeep: { path: "~/.gemini/antigravity-cli", what: "ToolState" } })).toBe(
      "warnings.willKeep.ToolState",
    );
    expect(warningKey({ WillKeep: { path: "~/.zshrc", what: "ShellConfigLines" } })).toBe(
      "warnings.willKeep.ShellConfigLines",
    );
    expect(warningKey({ WillKeep: { path: "/usr/local/bin/grok", what: "OutsideHome" } })).toBe(
      "warnings.willKeep.OutsideHome",
    );
    expect(warningKey({ WillKeep: { path: "~/.local/bin/agent", what: "NotOurs" } })).toBe(
      "warnings.willKeep.NotOurs",
    );
    expect(warningKey({ WillKeep: { path: "~/.cache/antigravity", what: "InstallerCache" } })).toBe(
      "warnings.willKeep.InstallerCache",
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

  it("says which formulae HOMEBREW_NO_CLEANUP_FORMULAE leaves out, after the lines it leaves them out of", () => {
    const line = (old_versions: boolean, autoremove: boolean, names = ["python@3.13", "node"]) => {
      const warning: Warning = { HomebrewNoCleanupFormulae: { names, old_versions, autoremove } };
      return {
        en: i18n.getFixedT("en")(warningKey(warning) as string, warningArgs(warning, en.common.listSeparator)),
        zh: i18n.getFixedT("zh-CN")(warningKey(warning) as string, warningArgs(warning, zhCN.common.listSeparator)),
        group: warningGroup(warning),
        caution: isCaution(warning),
      };
    };
    expect(line(true, false)).toEqual({
      en: "Homebrew leaves out python@3.13, node, which HOMEBREW_NO_CLEANUP_FORMULAE lists: it won't delete their older versions.",
      zh: "HOMEBREW_NO_CLEANUP_FORMULAE列出的python@3.13、node除外：Homebrew不会删除它们的旧版本。",
      group: "note",
      caution: false,
    });
    expect(line(false, true).zh).toBe(
      "HOMEBREW_NO_CLEANUP_FORMULAE列出的python@3.13、node除外：Homebrew不会自动删除它们，也不会自动删除它们运行时需要的软件。",
    );
    expect(line(true, true).en).toBe(
      "Homebrew leaves out python@3.13, node, which HOMEBREW_NO_CLEANUP_FORMULAE lists: it won't delete their older versions, or autoremove them or what they need to run.",
    );
    expect(line(true, true).zh).toBe(
      "HOMEBREW_NO_CLEANUP_FORMULAE列出的python@3.13、node除外：Homebrew不会删除它们的旧版本，也不会自动删除它们和它们运行时需要的软件。",
    );
  });

  it("says Homebrew deletes the trust list's entry for what it uninstalls, its why behind the ⓘ", () => {
    const warning: Warning = { HomebrewForgetsTrust: { name: "gautham-v/tap/claudebar" } };
    const zh = i18n.getFixedT("zh-CN");
    const enT = i18n.getFixedT("en");
    expect(enT(warningKey(warning) as string, warningArgs(warning))).toBe(
      "Homebrew also removes gautham-v/tap/claudebar from its trust list.",
    );
    expect(zh(warningKey(warning) as string, warningArgs(warning))).toBe(
      "Homebrew还会从它的信任列表中删除“gautham-v/tap/claudebar”。",
    );
    expect(zh(warningDetailKey(warning) as string)).toBe(
      "这条记录只针对它本身，列表里的其他记录不受影响。",
    );
    expect(warningGroup(warning)).toBe("note");
    expect(isCaution(warning)).toBe(false);
    expect(deletesForGood(warning)).toBe(false);
  });

  it("says a cask's sentence where Homebrew may not run what it recorded, in both languages", () => {
    const say = (lang: string, what: UninstallScope) =>
      warningText(i18n.getFixedT(lang), { UninstallScope: { what } }, "Thing");
    expect(say("zh-CN", "HomebrewCaskPlainThirdParty")).toBe(
      "删除Homebrew为“Thing”放置的文件；它的设置和数据保留不动，它的安装器如果另外装了文件，也不删除。",
    );
    expect(say("zh-CN", "HomebrewCaskRuby")).toBe(
      "删除Homebrew为“Thing”放置的文件，并执行它的卸载步骤；其中部分步骤还会删除什么，无法事先得知。Homebrew读不出安装时记下的步骤时，会按它现在的定义执行。",
    );
    expect(say("zh-CN", "HomebrewCaskStepsOnlyRuby")).toBe(
      "执行“Thing”的卸载步骤；其中部分步骤还会删除什么，无法事先得知。Homebrew读不出安装时记下的步骤时，会按它现在的定义执行。",
    );
    expect(say("zh-CN", "HomebrewCaskPlainRuby")).toBe(
      "删除Homebrew为“Thing”放置的文件。Homebrew读不出安装时记下的内容时，会按它现在的定义执行，那样还会删除什么，无法事先得知。",
    );
    expect(say("en", "HomebrewCaskPlainRuby")).toBe(
      "Deletes the files Homebrew placed for Thing. If Homebrew can't read what it recorded at install, it uses the cask's current definition, and what that deletes can't be seen in advance.",
    );
    expect(say("zh-CN", "HomebrewCaskStepsIfTrusted")).toBe(
      "删除Homebrew为“Thing”放置的文件；它的卸载步骤，只在Homebrew信任它的来源时才执行。",
    );
    expect(say("zh-CN", "HomebrewCaskStepsOnlyIfTrusted")).toBe("“Thing”的卸载步骤，只在Homebrew信任它的来源时才执行。");
    expect(say("en", "HomebrewCaskStepsIfTrusted")).toBe(
      "Deletes the files Homebrew placed for Thing; its uninstall steps run only if Homebrew trusts where it comes from.",
    );
    expect(say("en", "HomebrewCaskPlainThirdParty")).toBe(
      "Deletes the files Homebrew placed for Thing; its settings and data stay, and so does anything its installer put on this Mac separately.",
    );
  });

  it("gives Homebrew's three brew.env warnings their keys", () => {
    expect(warningKey("HomebrewAutoremoves")).toBe("warnings.homebrewAutoremoves");
    expect(warningKey("HomebrewPeriodicCleanup")).toBe("warnings.homebrewPeriodicCleanup");
    expect(warningKey("HomebrewCleanupAutoremoves")).toBe("warnings.homebrewCleanupAutoremoves");
  });

  it("says Homebrew's clean-up runs after every install or update, and for all its software when the periodic one is due", () => {
    // Homebrew 7.0.6-70: `Cleanup.install_clean!` after every `brew
    // install` and `brew upgrade` (install.rb:326, cleanup.rb:361-389),
    // `Cleanup#clean!` only when the periodic clean-up is due
    // (install.rb:327, cleanup.rb:418-445). One line for both.
    expect(en.warnings.homebrewPeriodicCleanup).toBe(
      "After installing or updating, Homebrew deletes the older versions of this software and of any it updates along with it, and stray old downloads; when its periodic clean-up is due, those of all Homebrew software.",
    );
    expect(zhCN.warnings.homebrewPeriodicCleanup).toBe(
      "安装或更新后，Homebrew会删除此软件及一起更新的软件的旧版本，以及残留的旧下载文件；定期清理到期时，所有Homebrew软件的旧版本和旧下载文件也会被删除。",
    );
  });

  it("gives each source's scope sentence and each kind of cask step its own key", () => {
    expect(warningKey({ UninstallScope: { what: "HomebrewFormulaOnly" } })).toBe(
      "warnings.uninstallScope.HomebrewFormulaOnly",
    );
    expect(warningKey({ UninstallScope: { what: "Npm" } })).toBe("warnings.uninstallScope.Npm");
    expect(warningKey({ CaskUninstallStep: { step: "RemovesPackages", items: ["a"] } })).toBe(
      "warnings.caskStep.RemovesPackages",
    );
    expect(warningKey({ CaskUninstallStep: { step: "RunsOwnSteps", items: [] } })).toBe(
      "warnings.caskStep.RunsOwnSteps",
    );
    expect(warningKey({ CaskUninstallStep: { step: "DeletesUnnamed", items: [] } })).toBe(
      "warnings.caskStep.DeletesUnnamed",
    );
  });

  it("gives the paths a remove step deletes only where they pass its check that check's line, named or not", () => {
    // `symlink_target_contains`, `content_contains`, or both
    // (install_steps.rb:1051-1060 in Homebrew 7.0.6-70).
    for (const [check, line] of CHECKS) {
      expect(
        warningKey({ CaskUninstallStep: { step: "Deletes", items: ["/usr/local/bin/arm-*"], only_if: check } }),
      ).toBe(`warnings.caskStep.Deletes${line}`);
      expect(warningKey({ CaskUninstallStep: { step: "DeletesUnnamed", items: [], only_if: check } })).toBe(
        `warnings.caskStep.DeletesUnnamed${line}`,
      );
    }
    // No check: the kind's own line.
    expect(warningKey({ CaskUninstallStep: { step: "Deletes", items: ["/usr/local/playdate"] } })).toBe(
      "warnings.caskStep.Deletes",
    );
  });

  it("gives a line of services or apps no number when one of their ids is a pattern", () => {
    // Homebrew stops every running service, and quits every running app,
    // a `*` id matches: 「还会停止并删除 7 个后台服务。」 counted
    // `com.adobe.CCXProcess.*` as one.
    expect(warningKey({ CaskUninstallStep: { step: "RemovesServices", items: ADOBE_SERVICES } })).toBe(
      "warnings.caskStep.RemovesServicesMatching",
    );
    expect(
      warningKey({ CaskUninstallStep: { step: "QuitsApps", items: ["com.adobe.accmac", "com.adobe.acc.*"] } }),
    ).toBe("warnings.caskStep.QuitsAppsMatching");
    // Names alone are counted.
    expect(
      warningKey({ CaskUninstallStep: { step: "RemovesServices", items: ADOBE_SERVICES.slice(0, 6) } }),
    ).toBe("warnings.caskStep.RemovesServices");
    expect(warningKey({ CaskUninstallStep: { step: "QuitsApps", items: ["com.adobe.accmac"] } })).toBe(
      "warnings.caskStep.QuitsApps",
    );
    // A path's `*` is part of the path the line shows, not a count.
    expect(
      warningKey({ CaskUninstallStep: { step: "Deletes", items: ["/Applications/Utilities/Adobe Creative Cloud*"] } }),
    ).toBe("warnings.caskStep.Deletes");
  });

  it("has no key for a Message -- its text comes from the wire, not i18n", () => {
    expect(warningKey({ Message: "boom" })).toBeNull();
  });

  it("is null for Message and for nothing else", () => {
    // The runtime half of what `tsc` checks at compile time: every
    // variant of `Warning` is one of these twenty-one, and the only one
    // without a `warnings.*` key is the raw-text catch-all. A variant this
    // list does not name is a `never` in `warningKey`'s default branches
    // and does not compile, so there is no "unrecognised variant" to test.
    const all: Warning[] = [
      "DependentsUnknown",
      "CompilesLocally",
      "DownloadsModelChanges",
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
      "HomebrewAutoremoves",
      "HomebrewPeriodicCleanup",
      "HomebrewCleanupAutoremoves",
      { HomebrewNoCleanupFormulae: { names: ["node"], old_versions: false, autoremove: true } },
      { HomebrewForgetsTrust: { name: "someone/tap/thing" } },
      { UninstallScope: { what: "Pipx" } },
      { CaskUninstallStep: { step: "Trashes", items: ["~/.nvs"] } },
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

  it("interpolates what a cask step names, counted for pluralisation, and nothing for one that names nothing", () => {
    expect(warningArgs({ CaskUninstallStep: { step: "Trashes", items: ["~/.nvs"] } })).toEqual({
      count: 1,
      items: "~/.nvs",
    });
    expect(
      warningArgs({ CaskUninstallStep: { step: "RemovesPackages", items: ["a.pkg", "b.pkg"] } }),
    ).toEqual({ count: 2, items: "a.pkg, b.pkg" });
    expect(warningArgs({ CaskUninstallStep: { step: "RunsOwnSteps", items: [] } })).toEqual({});
    expect(warningArgs({ CaskUninstallStep: { step: "DeletesUnnamed", items: [] } })).toEqual({});
  });

  it("interpolates the text a remove step's check looks for, beside the paths", () => {
    expect(
      warningArgs({
        CaskUninstallStep: {
          step: "Deletes",
          items: ["/usr/local/bin/arm-*"],
          only_if: { LinkTargetContains: "playdate" },
        },
      }),
    ).toEqual({ count: 1, items: "/usr/local/bin/arm-*", link: "playdate" });
    expect(
      warningArgs({
        CaskUninstallStep: { step: "DeletesUnnamed", items: [], only_if: { ContentContains: "SocketLock" } },
      }),
    ).toEqual({ content: "SocketLock" });
    expect(
      warningArgs({
        CaskUninstallStep: {
          step: "Deletes",
          items: ["/usr/local/bin/gpg", "/usr/local/bin/gpg2"],
          only_if: { LinkTargetAndContentContain: { link_target: "MacGPG2", content: "gpg" } },
        },
      }),
    ).toEqual({ count: 2, items: "/usr/local/bin/gpg, /usr/local/bin/gpg2", link: "MacGPG2", content: "gpg" });
  });

  it("is empty for every other variant", () => {
    // A scope sentence's `{{name}}` is the row's, which `warningText` is given.
    expect(warningArgs({ UninstallScope: { what: "Cargo" } })).toEqual({});
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
  it("looks a fixed warning up through t(), with its args, a list joined in the window's language", () => {
    expect(warningText(fakeT, "DependentsUnknown")).toBe("warnings.dependentsUnknown");
    // 「还有 2 个工具要用它：a、b。」, never "a, b" inside a Chinese sentence.
    const chineseT = (key: string, options?: Record<string, unknown>) =>
      key === "common.listSeparator" ? "、" : fakeT(key, options);
    expect(warningText(chineseT, { WouldBreak: { names: ["a", "b"] } })).toBe(
      'warnings.wouldBreak({"count":2,"names":"a、b"})',
    );
    expect(warningText(chineseT, { RemovesCargoInstalled: { names: ["jj-cli", "tokei"] } })).toBe(
      'warnings.removesCargoInstalled({"count":2,"names":"jj-cli、tokei"})',
    );
  });

  it("reads a Message's text directly, bypassing t()", () => {
    expect(warningText(fakeT, { Message: "boom" })).toBe("boom");
  });

  it("says a scope sentence with the name it is given, and has none without one", () => {
    expect(warningText(fakeT, { UninstallScope: { what: "Ollama" } }, "qwen3:8b")).toBe(
      'warnings.uninstallScope.Ollama({"name":"qwen3:8b"})',
    );
    expect(warningText(fakeT, { UninstallScope: { what: "Ollama" } })).toBeNull();
    // Every other warning has no use for it.
    expect(warningText(fakeT, "DependentsUnknown", "jq")).toBe("warnings.dependentsUnknown");
  });
});

/** Every variant of `Warning` once, for the checks that go over them all. */
const EVERY_VARIANT: Warning[] = [
  "DependentsUnknown",
  "CompilesLocally",
  "DownloadsModelChanges",
  "NonRegistrySource",
  "HomebrewRustupLosesToolchains",
  "EditsShellConfig",
  { WouldBreak: { names: ["a"] } },
  { ThirdPartyRegistry: { host: "modelscope.cn" } },
  { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } },
  { WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } },
  { AlreadyGone: { path: "~/.local/share/claude" } },
  { RemovesToolchains: { path: "~/.rustup", names: ["stable-aarch64-apple-darwin"] } },
  { DeletesCargoHome: { path: "~/.cargo" } },
  { RemovesCargoInstalled: { names: ["hexyl"] } },
  { LeavesShellConfigLine: { path: "~/.zshrc", certain: true } },
  "HomebrewAutoremoves",
  "HomebrewPeriodicCleanup",
  "HomebrewCleanupAutoremoves",
  { HomebrewNoCleanupFormulae: { names: ["node"], old_versions: true, autoremove: true } },
  { HomebrewForgetsTrust: { name: "someone/tap/thing" } },
  { UninstallScope: { what: "HomebrewCaskPlain" } },
  { CaskUninstallStep: { step: "Deletes", items: ["~/Library/Application Support/Foo"] } },
  { Message: "boom" },
];

/** Every `UninstallScope`, as the Rust wire-shape test lists them. */
const EVERY_SCOPE: UninstallScope[] = [
  "HomebrewFormulaOnly",
  "HomebrewFormula",
  "HomebrewCaskPlain",
  "HomebrewCaskSteps",
  "HomebrewCaskStepsAutoremoves",
  "HomebrewCaskStepsUnseen",
  "HomebrewCaskStepsOnly",
  "HomebrewCaskStepsOnlyUnseen",
  "HomebrewCask",
  "HomebrewCaskPlainThirdParty",
  "HomebrewCaskRuby",
  "HomebrewCaskStepsOnlyRuby",
  "HomebrewCaskPlainRuby",
  "HomebrewCaskStepsIfTrusted",
  "HomebrewCaskStepsOnlyIfTrusted",
  "Npm",
  "Pipx",
  "Uv",
  "Cargo",
  "Ollama",
];

/** Every `RemoveCheck`, with the tail its lines' keys add to `Deletes` and `DeletesUnnamed`. */
const CHECKS: [RemoveCheck, string][] = [
  [{ LinkTargetContains: "playdate" }, "Links"],
  [{ ContentContains: "SocketLock" }, "FilesContaining"],
  [{ LinkTargetAndContentContain: { link_target: "MacGPG2", content: "gpg" } }, "LinksToFilesContaining"],
];

/** The cask steps whose line counts what it would name, the ids behind its ⓘ. */
const COUNTED_STEPS: CaskStep[] = ["RemovesServices", "QuitsApps"];

/**
 * adobe-creative-cloud's `launchctl:`, as `cask_receipt::classify` reads
 * it from its receipt (cask_receipt.rs's fixture): six names and a
 * pattern.
 */
const ADOBE_SERVICES = [
  "Adobe_Genuine_Software_Integrity_Service",
  "com.adobe.acc.installer",
  "com.adobe.acc.installer.v2",
  "com.adobe.AdobeCreativeCloud",
  "com.adobe.AdobeDesktopService",
  "com.adobe.ccxprocess",
  "com.adobe.CCXProcess.*",
];

/** Every `CaskStep`, in its declared order. */
const EVERY_STEP: CaskStep[] = [
  "Deletes",
  "DeletesUnnamed",
  "Trashes",
  "RemovesPackages",
  "RunsScript",
  "RunsOwnSteps",
  "RemovesServices",
  "RemovesKexts",
  "DeletesCertificates",
  "RemovesLoginItems",
  "QuitsApps",
  "QuitsNamedApps",
];

/** A key's text in one locale, or undefined when it has none. */
function lookUp(resource: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>(
    (node, part) => (typeof node === "object" && node !== null ? (node as Record<string, unknown>)[part] : undefined),
    resource,
  );
}

describe("warningGroup", () => {
  it("puts what moves to the Trash, and what was already gone from there, in the Trash group", () => {
    expect(warningGroup({ WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } })).toBe("trash");
    expect(warningGroup({ AlreadyGone: { path: "~/.local/share/claude" } })).toBe("trash");
  });

  it("puts every kind of kept path in the kept group", () => {
    const kinds: KeptWhat[] = [
      "Settings",
      "SettingsAndHistory",
      "ToolState",
      "ShellConfigLines",
      "OutsideHome",
      "NotOurs",
      "InstallerCache",
    ];
    for (const what of kinds) {
      expect(warningGroup({ WillKeep: { path: "~/x", what } })).toBe("keep");
    }
  });

  it("puts an uninstall's sentence about what goes and what stays in a group of its own", () => {
    for (const what of EVERY_SCOPE) expect(warningGroup({ UninstallScope: { what } })).toBe("scope");
  });

  it("puts everything else under what to know before going on", () => {
    const notes = EVERY_VARIANT.filter(
      (warning) =>
        typeof warning === "string" ||
        !(
          "WillTrash" in warning ||
          "AlreadyGone" in warning ||
          "WillKeep" in warning ||
          "UninstallScope" in warning
        ),
    );
    expect(notes).toHaveLength(19);
    for (const warning of notes) expect(warningGroup(warning)).toBe("note");
    // Every kind of a cask's extra steps.
    for (const step of EVERY_STEP) {
      expect(warningGroup({ CaskUninstallStep: { step, items: ["x"] } })).toBe("note");
    }
  });

  it("makes a variant this build does not know something to note, not something to drop", () => {
    expect(warningGroup("SomeFutureVariant" as unknown as Warning)).toBe("note");
    expect(warningGroup({ SomeFutureVariant: {} } as unknown as Warning)).toBe("note");
  });
});

describe("warningDetailKey", () => {
  it("keeps the why of a kept path behind the line, except for your own settings", () => {
    expect(warningDetailKey({ WillKeep: { path: "~/.claude.json", what: "Settings" } })).toBeNull();
    expect(warningDetailKey({ WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } })).toBeNull();
    expect(warningDetailKey({ WillKeep: { path: "~/.gemini/antigravity-cli", what: "ToolState" } })).toBe(
      "warnings.willKeep.ToolStateDetail",
    );
    expect(warningDetailKey({ WillKeep: { path: "~/.zshrc", what: "ShellConfigLines" } })).toBe(
      "warnings.willKeep.ShellConfigLinesDetail",
    );
    expect(warningDetailKey({ WillKeep: { path: "/usr/local/bin/grok", what: "OutsideHome" } })).toBe(
      "warnings.willKeep.OutsideHomeDetail",
    );
    expect(warningDetailKey({ WillKeep: { path: "~/.local/bin/agent", what: "NotOurs" } })).toBe(
      "warnings.willKeep.NotOursDetail",
    );
    expect(warningDetailKey({ WillKeep: { path: "~/.cache/antigravity", what: "InstallerCache" } })).toBe(
      "warnings.willKeep.InstallerCacheDetail",
    );
  });

  it("keeps what rustup's deletions mean behind the line, one why for both toolchain sentences", () => {
    expect(warningDetailKey({ RemovesToolchains: { path: "~/.rustup", names: ["stable"] } })).toBe(
      "warnings.removesToolchainsDetail",
    );
    expect(warningDetailKey({ RemovesToolchains: { path: "~/.rustup", names: [] } })).toBe(
      "warnings.removesToolchainsDetail",
    );
    expect(warningDetailKey({ RemovesCargoInstalled: { names: ["hexyl"] } })).toBe(
      "warnings.removesCargoInstalledDetail",
    );
    expect(warningDetailKey({ LeavesShellConfigLine: { path: "~/.zshrc", certain: true } })).toBe(
      "warnings.leavesShellConfigLineDetail",
    );
    expect(warningDetailKey({ LeavesShellConfigLine: { path: "~/.zshrc", certain: false } })).toBe(
      "warnings.leavesShellConfigLineMaybeDetail",
    );
  });

  it("keeps which Homebrew setting brought a clean-up or autoremove back behind the line", () => {
    // The line says what else Homebrew removes; the ⓘ, that a brew.env
    // took back the variable Banager runs Homebrew with.
    expect(warningDetailKey("HomebrewAutoremoves")).toBe("warnings.homebrewAutoremovesDetail");
    expect(warningDetailKey("HomebrewPeriodicCleanup")).toBe("warnings.homebrewPeriodicCleanupDetail");
    expect(warningDetailKey("HomebrewCleanupAutoremoves")).toBe(
      "warnings.homebrewCleanupAutoremovesDetail",
    );
    expect(en.warnings.homebrewAutoremovesDetail).toContain("HOMEBREW_NO_AUTOREMOVE=1");
    expect(zhCN.warnings.homebrewAutoremovesDetail).toContain("HOMEBREW_NO_AUTOREMOVE=1");
    for (const locale of [en, zhCN]) {
      expect(locale.warnings.homebrewAutoremovesDetail).toContain("brew.env");
      expect(locale.warnings.homebrewPeriodicCleanupDetail).toContain("HOMEBREW_NO_INSTALL_CLEANUP=1");
      expect(locale.warnings.homebrewPeriodicCleanupDetail).toContain("brew.env");
      expect(locale.warnings.homebrewCleanupAutoremovesDetail).toContain("HOMEBREW_NO_INSTALL_CLEANUP=1");
    }
  });

  it("has nothing behind a line that already says it all", () => {
    for (const warning of [
      // That the whole Cargo folder goes, and none of it to the Trash, is
      // on the line itself.
      { DeletesCargoHome: { path: "~/.cargo" } },
      "DependentsUnknown",
      "CompilesLocally",
      "DownloadsModelChanges",
      "NonRegistrySource",
      "HomebrewRustupLosesToolchains",
      "EditsShellConfig",
      { WouldBreak: { names: ["a"] } },
      { ThirdPartyRegistry: { host: "modelscope.cn" } },
      { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } },
      { AlreadyGone: { path: "~/.local/share/claude" } },
      { Message: "boom" },
      ...EVERY_SCOPE.map((what): Warning => ({ UninstallScope: { what } })),
      ...EVERY_STEP.filter((step) => !COUNTED_STEPS.includes(step)).map(
        (step): Warning => ({ CaskUninstallStep: { step, items: ["x"] } }),
      ),
    ] as Warning[]) {
      expect(warningDetailKey(warning)).toBeNull();
    }
  });

  it("puts the ids a counted cask step does not name behind its ⓘ, as macOS names them", () => {
    // 「还会停止并删除这些后台服务：com.microsoft.VSCode.ShipIt。」 told a
    // beginner nothing: the line counts, and the ids are one press away.
    for (const step of COUNTED_STEPS) {
      expect(warningDetailKey({ CaskUninstallStep: { step, items: ["com.microsoft.VSCode.ShipIt"] } })).toBe(
        "warnings.caskStep.systemNames",
      );
    }
    const chineseT = (key: string, options?: Record<string, unknown>) =>
      key === "common.listSeparator" ? "、" : fakeT(key, options);
    expect(
      warningLine(chineseT, {
        CaskUninstallStep: { step: "QuitsApps", items: ["com.microsoft.VSCode", "com.microsoft.VSCode.helper"] },
      }),
    ).toEqual({
      text: 'warnings.caskStep.QuitsApps({"count":2,"items":"com.microsoft.VSCode、com.microsoft.VSCode.helper"})',
      detail:
        'warnings.caskStep.systemNames({"count":2,"items":"com.microsoft.VSCode、com.microsoft.VSCode.helper"})',
      caution: true,
    });
    // An app Banager found is named on the line itself, with nothing behind it.
    expect(warningDetailKey({ CaskUninstallStep: { step: "QuitsNamedApps", items: ["Visual Studio Code"] } })).toBeNull();
  });

  it("keeps the ids behind the ⓘ of a line a pattern leaves uncounted, the pattern among them", () => {
    const pattern = ["com.adobe.ccxprocess", "com.adobe.CCXProcess.*"];
    for (const step of COUNTED_STEPS) {
      expect(warningDetailKey({ CaskUninstallStep: { step, items: pattern } })).toBe(
        "warnings.caskStep.systemNames",
      );
    }
    const chineseT = (key: string, options?: Record<string, unknown>) =>
      key === "common.listSeparator" ? "、" : fakeT(key, options);
    expect(warningLine(chineseT, { CaskUninstallStep: { step: "RemovesServices", items: pattern } })).toEqual({
      text: 'warnings.caskStep.RemovesServicesMatching({"count":2,"items":"com.adobe.ccxprocess、com.adobe.CCXProcess.*"})',
      detail: 'warnings.caskStep.systemNames({"count":2,"items":"com.adobe.ccxprocess、com.adobe.CCXProcess.*"})',
      caution: true,
    });
  });

  it("has copy in both languages for a line of services or apps a pattern leaves uncounted", () => {
    // No number and no ids on the line: it says a pattern matches some.
    for (const step of COUNTED_STEPS) {
      const key = warningKey({ CaskUninstallStep: { step, items: ["com.example.*"] } }) ?? "";
      expect(key).toBe(`warnings.caskStep.${step}Matching`);
      for (const [locale, pattern] of [
        [en, /pattern/],
        [zhCN, /规则/],
      ] as const) {
        const text = lookUp(locale, key);
        expect(typeof text, key).toBe("string");
        expect(text as string, key).not.toContain("{{");
        expect(text as string, key).not.toMatch(/\d/);
        expect(text as string, key).toMatch(pattern);
      }
    }
  });

  it("has copy in both languages for every scope sentence and every kind of cask step", () => {
    // `{{name}}` in every scope sentence, `{{items}}` in every step that
    // names anything; English says one and several apart, Chinese needs
    // only `_other`.
    for (const what of EVERY_SCOPE) {
      const key = warningKey({ UninstallScope: { what } }) ?? "";
      for (const locale of [en, zhCN]) {
        const text = lookUp(locale, key);
        expect(typeof text, key).toBe("string");
        expect(text as string, key).toContain("{{name}}");
      }
    }
    for (const step of EVERY_STEP) {
      const key = warningKey({ CaskUninstallStep: { step, items: [] } }) ?? "";
      // The two that name nothing have one sentence and no `{{items}}`.
      if (step === "RunsOwnSteps" || step === "DeletesUnnamed") {
        for (const locale of [en, zhCN]) {
          expect(typeof lookUp(locale, key), key).toBe("string");
          expect(lookUp(locale, key) as string, key).not.toContain("{{");
        }
        continue;
      }
      // The two that count what they would name say how many, and their
      // ⓘ has the ids (`systemNames`).
      if (COUNTED_STEPS.includes(step)) {
        for (const [locale, forms] of [
          [en, ["_one", "_other"]],
          [zhCN, ["_other"]],
        ] as const) {
          for (const form of forms) {
            const text = lookUp(locale, `${key}${form}`);
            expect(typeof text, `${key}${form}`).toBe("string");
            expect(text as string, `${key}${form}`).not.toContain("{{items}}");
            if (form === "_other") expect(text as string, `${key}${form}`).toContain("{{count}}");
            const names = lookUp(locale, `warnings.caskStep.systemNames${form}`);
            expect(names as string, `systemNames${form}`).toContain("{{items}}");
          }
        }
        continue;
      }
      for (const [locale, forms] of [
        [en, ["_one", "_other"]],
        [zhCN, ["_other"]],
      ] as const) {
        for (const form of forms) {
          const text = lookUp(locale, `${key}${form}`);
          expect(typeof text, `${key}${form}`).toBe("string");
          expect(text as string, `${key}${form}`).toContain("{{items}}");
        }
      }
    }
  });

  it("has copy in both languages for every line of paths a remove step deletes only where they pass its check", () => {
    // Each says "permanently" and the text the check looks for; a named
    // line says its paths, one and several apart in English.
    for (const [check, line] of CHECKS) {
      const placeholders = Object.keys(warningArgs({ CaskUninstallStep: { step: "DeletesUnnamed", items: [], only_if: check } }));
      expect(placeholders.length).toBeGreaterThan(0);
      const unnamed = `warnings.caskStep.DeletesUnnamed${line}`;
      for (const locale of [en, zhCN]) {
        const text = lookUp(locale, unnamed);
        expect(typeof text, unnamed).toBe("string");
        expect(text as string, unnamed).toMatch(/permanently|永久/);
        expect(text as string, unnamed).not.toContain("{{items}}");
        for (const name of placeholders) expect(text as string, unnamed).toContain(`{{${name}}}`);
      }
      const named = `warnings.caskStep.Deletes${line}`;
      for (const [locale, forms] of [
        [en, ["_one", "_other"]],
        [zhCN, ["_other"]],
      ] as const) {
        for (const form of forms) {
          const text = lookUp(locale, `${named}${form}`);
          expect(typeof text, `${named}${form}`).toBe("string");
          expect(text as string, `${named}${form}`).toMatch(/permanently|永久/);
          expect(text as string, `${named}${form}`).toContain("{{items}}");
          for (const name of placeholders) expect(text as string, `${named}${form}`).toContain(`{{${name}}}`);
        }
      }
    }
  });

  it("names only keys both languages have", () => {
    const keys = [
      ...EVERY_VARIANT,
      ...(["ToolState", "ShellConfigLines", "OutsideHome", "NotOurs", "InstallerCache"] as KeptWhat[]).map(
        (what): Warning => ({ WillKeep: { path: "~/x", what } }),
      ),
      { LeavesShellConfigLine: { path: "~/.zshrc", certain: false } },
    ]
      .map(warningDetailKey)
      .filter((key): key is string => key !== null);
    expect(keys.length).toBeGreaterThan(0);
    for (const key of keys) {
      expect(typeof lookUp(en, key), `en: ${key}`).toBe("string");
      expect(typeof lookUp(zhCN, key), `zh-CN: ${key}`).toBe("string");
    }
  });
});

describe("deletesForGood", () => {
  it("is true of rustup's permanent deletions and a cask's recorded deletions, and of nothing else", () => {
    const forGood = EVERY_VARIANT.filter(deletesForGood);
    expect(forGood).toEqual([
      { RemovesToolchains: { path: "~/.rustup", names: ["stable-aarch64-apple-darwin"] } },
      { DeletesCargoHome: { path: "~/.cargo" } },
      { RemovesCargoInstalled: { names: ["hexyl"] } },
      { CaskUninstallStep: { step: "Deletes", items: ["~/Library/Application Support/Foo"] } },
    ]);
    expect(deletesForGood({ RemovesToolchains: { path: "~/.rustup", names: [] } })).toBe(true);
    // What goes to the Trash can be dragged back out.
    expect(deletesForGood({ WillTrash: { path: "~/.local/share/claude", what: "Program" } })).toBe(false);
    // Only the steps whose lines say "permanently": what `delete:` and a
    // `remove` step name, and what a `remove` step finds as it runs.
    for (const step of EVERY_STEP) {
      expect(deletesForGood({ CaskUninstallStep: { step, items: ["x"] } })).toBe(
        step === "Deletes" || step === "DeletesUnnamed",
      );
    }
    expect(deletesForGood({ CaskUninstallStep: { step: "DeletesUnnamed", items: [] } })).toBe(true);
    for (const locale of [en, zhCN]) {
      expect(locale.warnings.caskStep.DeletesUnnamed).toMatch(/permanently|永久/);
    }
    // A `remove` step that checks each path first can still delete one for
    // good, so its lines count too.
    for (const [check] of CHECKS) {
      expect(deletesForGood({ CaskUninstallStep: { step: "Deletes", items: ["x"], only_if: check } })).toBe(true);
      expect(deletesForGood({ CaskUninstallStep: { step: "DeletesUnnamed", items: [], only_if: check } })).toBe(
        true,
      );
    }
  });

  it("is false of a variant this build does not know", () => {
    expect(deletesForGood("SomeFutureVariant" as unknown as Warning)).toBe(false);
    expect(deletesForGood({ SomeFutureVariant: {} } as unknown as Warning)).toBe(false);
  });
});

describe("skipsTrash", () => {
  it("is true of every sentence whose command deletes in place, and not of those that cannot see or read it all", () => {
    const skips = EVERY_SCOPE.filter((what) => skipsTrash([{ UninstallScope: { what } }]));
    expect(skips).toEqual([
      "HomebrewFormulaOnly",
      "HomebrewFormula",
      "HomebrewCaskPlain",
      "HomebrewCaskSteps",
      "HomebrewCaskStepsAutoremoves",
      "HomebrewCaskStepsOnly",
      "HomebrewCaskPlainThirdParty",
      "Npm",
      "Pipx",
      "Uv",
      "Cargo",
      "Ollama",
    ]);
  });

  it("is false beside a cask step that moves paths to the Trash, wherever it comes in the plan", () => {
    const trashes: Warning = { CaskUninstallStep: { step: "Trashes", items: ["~/.nvs"] } };
    for (const what of ["HomebrewCaskSteps", "HomebrewCaskStepsAutoremoves", "HomebrewCaskStepsOnly"] as const) {
      expect(skipsTrash([{ UninstallScope: { what } }, trashes])).toBe(false);
      expect(skipsTrash([trashes, { UninstallScope: { what } }])).toBe(false);
    }
    // Every other step deletes in place, or deletes nothing.
    for (const step of EVERY_STEP.filter((step) => step !== "Trashes")) {
      expect(
        skipsTrash([{ UninstallScope: { what: "HomebrewCaskSteps" } }, { CaskUninstallStep: { step, items: ["x"] } }]),
        step,
      ).toBe(true);
    }
  });

  it("is false with no sentence: a path-list uninstall, rustup's, npm 6's", () => {
    expect(skipsTrash([])).toBe(false);
    expect(skipsTrash([{ WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } }])).toBe(false);
    expect(skipsTrash([{ RemovesToolchains: { path: "~/.rustup", names: [] } }, { DeletesCargoHome: { path: "~/.cargo" } }])).toBe(
      false,
    );
    // A sentence this build does not know.
    expect(skipsTrash([{ UninstallScope: { what: "SomeFutureScope" as UninstallScope } }])).toBe(false);
  });

  it("has its sentence in both languages, carrying on from the scope's", () => {
    expect(en.uninstall.endsSkipsTrash).toBe("{{sentence}} Removed files don't go to the Trash.");
    expect(zhCN.uninstall.endsSkipsTrash).toBe("{{sentence}}删除的文件不会进入废纸篓。");
  });
});

describe("isCaution", () => {
  it("marks what a person should weigh first, and not what only says how it goes", () => {
    const cautions: Warning[] = [
      "DependentsUnknown",
      "HomebrewRustupLosesToolchains",
      "HomebrewAutoremoves",
      "HomebrewPeriodicCleanup",
      "HomebrewCleanupAutoremoves",
      { WouldBreak: { names: ["wget"] } },
      { ThirdPartyRegistry: { host: "modelscope.cn" } },
      { RemovesToolchains: { path: "~/.rustup", names: ["stable"] } },
      { DeletesCargoHome: { path: "~/.cargo" } },
      { RemovesCargoInstalled: { names: ["tokei"] } },
      { LeavesShellConfigLine: { path: "~/.zshrc", certain: true } },
      { CaskUninstallStep: { step: "Deletes", items: ["~/Library/Foo"] } },
      { Message: "something this build has no words for" },
    ];
    const plain: Warning[] = [
      "CompilesLocally",
      "DownloadsModelChanges",
      "NonRegistrySource",
      "EditsShellConfig",
      { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } },
      { WillKeep: { path: "~/.claude.json", what: "Settings" } },
      { AlreadyGone: { path: "~/.grok/downloads" } },
      { HomebrewNoCleanupFormulae: { names: ["node"], old_versions: true, autoremove: false } },
      { HomebrewForgetsTrust: { name: "someone/tap/thing" } },
      { UninstallScope: { what: "HomebrewFormula" } },
    ];
    expect(cautions.filter((warning) => !isCaution(warning))).toEqual([]);
    expect(plain.filter(isCaution)).toEqual([]);
  });
});

describe("warningLines", () => {
  it("renders every warning in order, in its group, with its why where it has one", () => {
    const lines = warningLines(fakeT, [
      { WillTrash: { path: "~/.local/share/claude", what: "Program" } },
      { WillKeep: { path: "~/.zshrc", what: "ShellConfigLines" } },
      "DependentsUnknown",
      { AlreadyGone: { path: "~/.grok/downloads" } },
      { Message: "boom" },
      { WillKeep: { path: "~/.claude.json", what: "Settings" } },
    ]);
    expect(lines).toEqual({
      scope: [],
      trash: [
        { text: 'warnings.willTrash.Program({"path":"~/.local/share/claude"})', detail: null, caution: false },
        { text: 'warnings.alreadyGone({"path":"~/.grok/downloads"})', detail: null, caution: false },
      ],
      keep: [
        {
          text: 'warnings.willKeep.ShellConfigLines({"path":"~/.zshrc"})',
          // Its why, handed the line's own values, which it may say.
          detail: 'warnings.willKeep.ShellConfigLinesDetail({"path":"~/.zshrc"})',
          caution: false,
        },
        { text: 'warnings.willKeep.Settings({"path":"~/.claude.json"})', detail: null, caution: false },
      ],
      data: [],
      note: [
        { text: "warnings.dependentsUnknown", detail: null, caution: true },
        { text: "boom", detail: null, caution: true },
      ],
    });
  });

  it("does not say again what the plan's own list of what still needs it says", () => {
    // Homebrew's preview fills `WouldBreak` and `affected` from one
    // `brew uses`; the confirmation shows `affected` as its own list.
    const warnings: Warning[] = ["DependentsUnknown", { WouldBreak: { names: ["wget"] } }];
    expect(warningLines(fakeT, warnings, ["wget"]).note).toEqual([
      { text: "warnings.dependentsUnknown", detail: null, caution: true },
    ]);
    // Without that list, it is the only place the names are said.
    expect(warningLines(fakeT, warnings).note.map((line) => line.text)).toEqual([
      "warnings.dependentsUnknown",
      'warnings.wouldBreak({"count":1,"names":"wget"})',
    ]);
  });

  it("says the scope sentence apart, with the tool's name, and the cask's steps as notes", () => {
    const warnings: Warning[] = [
      { UninstallScope: { what: "HomebrewCaskSteps" } },
      { CaskUninstallStep: { step: "RemovesPackages", items: ["com.microsoft.pkg.licensing"] } },
      "HomebrewAutoremoves",
    ];
    const lines = warningLines(fakeT, warnings, [], "Microsoft Word");
    expect(lines.scope).toEqual([
      { text: 'warnings.uninstallScope.HomebrewCaskSteps({"name":"Microsoft Word"})', detail: null, caution: false },
    ]);
    expect(lines.note.map((line) => line.text)).toEqual([
      'warnings.caskStep.RemovesPackages({"count":1,"items":"com.microsoft.pkg.licensing"})',
      "warnings.homebrewAutoremoves",
    ]);
    // Without a name there is no sentence to say it in.
    expect(warningLines(fakeT, warnings).scope).toEqual([]);
  });

  it("is empty for an empty list", () => {
    expect(warningLines(fakeT, [])).toEqual({ scope: [], trash: [], keep: [], data: [], note: [] });
  });

  it("leaves the sources that run on the package to the confirmation's own list, with or without Homebrew's", () => {
    // `Warning::NeededBySource` (crates/banager-core/src/needed_by.rs) has
    // no line: the uninstall confirmation lists it with what Homebrew
    // names, by the source's name, which only the page has (`neededByItem`).
    const needed: Warning = { NeededBySource: { instance_id: "npm:/opt/homebrew", program: true, tools: 4 } };
    expect(warningKey(needed)).toBeNull();
    expect(warningArgs(needed)).toEqual({});
    expect(warningDetailKey(needed)).toBeNull();
    expect(warningText(fakeT, needed)).toBeNull();
    expect(warningGroup(needed)).toBe("note");
    expect(isCaution(needed)).toBe(true);
    expect(deletesForGood(needed)).toBe(false);
    const warnings: Warning[] = [{ UninstallScope: { what: "HomebrewFormulaOnly" } }, needed];
    for (const affected of [[], ["pipx"]]) {
      const lines = warningLines(fakeT, warnings, affected, "node@22");
      expect(lines.note).toEqual([]);
      expect(lines.scope.map((line) => line.text)).toEqual([
        'warnings.uninstallScope.HomebrewFormulaOnly({"name":"node@22"})',
      ]);
    }
  });
});
