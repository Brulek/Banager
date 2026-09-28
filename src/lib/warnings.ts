/**
 * How `Plan.warnings` and `UpdateCandidate.warnings` become text in the
 * user's language, and where each goes in a confirmation. Same split as
 * `outcomeKey`/`outcomeArgs` in `src/lib/format.ts`: no JSX here, and `t`
 * only as a parameter, so both confirmations and the updates page share
 * one rule and it is testable without rendering anything.
 */
import type { CaskStep, KeptWhat, RemovedWhat, UninstallScope, Warning } from "./types";

/** The sentence for each kind of path a path-list uninstall moves; a
 *  `Record` over `RemovedWhat`, so a kind without copy fails `tsc`. */
const REMOVED_WHAT_KEYS: Record<RemovedWhat, string> = {
  Launcher: "warnings.willTrash.Launcher",
  Program: "warnings.willTrash.Program",
  Cache: "warnings.willTrash.Cache",
  Backups: "warnings.willTrash.Backups",
};

/** The sentence for each kind of path a path-list uninstall keeps. */
const KEPT_WHAT_KEYS: Record<KeptWhat, string> = {
  Settings: "warnings.willKeep.Settings",
  SettingsAndHistory: "warnings.willKeep.SettingsAndHistory",
  ToolState: "warnings.willKeep.ToolState",
  ShellConfigLines: "warnings.willKeep.ShellConfigLines",
  OutsideHome: "warnings.willKeep.OutsideHome",
  NotOurs: "warnings.willKeep.NotOurs",
  InstallerCache: "warnings.willKeep.InstallerCache",
};

/**
 * Why each kind of kept path stays, where the line itself does not say
 * it: behind the line's ⓘ. The first two need none -- your settings stay
 * because they are yours. A `Record` over `KeptWhat`, so a kind added
 * without an answer here fails `tsc`.
 */
const KEPT_WHAT_DETAIL_KEYS: Record<KeptWhat, string | null> = {
  Settings: null,
  SettingsAndHistory: null,
  ToolState: "warnings.willKeep.ToolStateDetail",
  ShellConfigLines: "warnings.willKeep.ShellConfigLinesDetail",
  OutsideHome: "warnings.willKeep.OutsideHomeDetail",
  NotOurs: "warnings.willKeep.NotOursDetail",
  InstallerCache: "warnings.willKeep.InstallerCacheDetail",
};

/**
 * The sentence for each source's uninstall: what goes and what stays. A
 * `Record` over `UninstallScope`, so a kind without copy fails `tsc`. Each
 * interpolates `{{name}}`, the row's name, which the uninstall
 * confirmation gives (`warningLines`' `subject`).
 */
const UNINSTALL_SCOPE_KEYS: Record<UninstallScope, string> = {
  HomebrewFormulaOnly: "warnings.uninstallScope.HomebrewFormulaOnly",
  HomebrewFormula: "warnings.uninstallScope.HomebrewFormula",
  HomebrewCaskPlain: "warnings.uninstallScope.HomebrewCaskPlain",
  HomebrewCaskSteps: "warnings.uninstallScope.HomebrewCaskSteps",
  HomebrewCaskStepsOnly: "warnings.uninstallScope.HomebrewCaskStepsOnly",
  HomebrewCask: "warnings.uninstallScope.HomebrewCask",
  Npm: "warnings.uninstallScope.Npm",
  Pipx: "warnings.uninstallScope.Pipx",
  Uv: "warnings.uninstallScope.Uv",
  Cargo: "warnings.uninstallScope.Cargo",
  Ollama: "warnings.uninstallScope.Ollama",
};

/**
 * The line for each kind of extra step a cask's recorded uninstall takes.
 * A `Record` over `CaskStep`. Each but `DeletesUnnamed` and `RunsOwnSteps`,
 * which name nothing, interpolates `{{items}}` and pluralises on
 * `{{count}}`.
 */
const CASK_STEP_KEYS: Record<CaskStep, string> = {
  Deletes: "warnings.caskStep.Deletes",
  DeletesUnnamed: "warnings.caskStep.DeletesUnnamed",
  Trashes: "warnings.caskStep.Trashes",
  RemovesPackages: "warnings.caskStep.RemovesPackages",
  RunsScript: "warnings.caskStep.RunsScript",
  RunsOwnSteps: "warnings.caskStep.RunsOwnSteps",
  RemovesServices: "warnings.caskStep.RemovesServices",
  RemovesKexts: "warnings.caskStep.RemovesKexts",
  DeletesCertificates: "warnings.caskStep.DeletesCertificates",
  RemovesLoginItems: "warnings.caskStep.RemovesLoginItems",
  QuitsApps: "warnings.caskStep.QuitsApps",
};

/**
 * The `warnings.*` key for a `Warning`'s copy, or `null` for the `Message`
 * catch-all, whose text is read straight off the wire (see
 * `warningMessage` below).
 *
 * Exhaustive, the way `faultKey` in `src/lib/format.ts` is: every variant
 * of `Warning` is named here, and the `never` defaults make `tsc` fail on
 * one that is not. It used to `return null` for anything it did not
 * recognise, and `warningTexts` drops a `null` without a word -- so a
 * variant added to `types.ts` without a case here reached the uninstall
 * dialog as a silently shorter list, which for a warning that names a
 * file about to be removed is the worst possible failure.
 */
export function warningKey(warning: Warning): string | null {
  if (typeof warning === "string") {
    switch (warning) {
      case "DependentsUnknown":
        return "warnings.dependentsUnknown";
      case "CompilesLocally":
        return "warnings.compilesLocally";
      case "NonRegistrySource":
        return "warnings.nonRegistrySource";
      case "HomebrewRustupLosesToolchains":
        return "warnings.homebrewRustupLosesToolchains";
      case "EditsShellConfig":
        return "warnings.editsShellConfig";
      case "HomebrewAutoremoves":
        return "warnings.homebrewAutoremoves";
      case "HomebrewCleanupAutoremoves":
        return "warnings.homebrewCleanupAutoremoves";
      default: {
        const unhandled: never = warning;
        return unhandled;
      }
    }
  }
  if ("WouldBreak" in warning) return "warnings.wouldBreak";
  if ("ThirdPartyRegistry" in warning) return "warnings.thirdPartyRegistry";
  if ("WillTrash" in warning) return REMOVED_WHAT_KEYS[warning.WillTrash.what];
  if ("WillKeep" in warning) return KEPT_WHAT_KEYS[warning.WillKeep.what];
  if ("AlreadyGone" in warning) return "warnings.alreadyGone";
  if ("RemovesToolchains" in warning) {
    // Without names the sentence has no parenthesis to fill.
    return warning.RemovesToolchains.names.length > 0
      ? "warnings.removesToolchains"
      : "warnings.removesToolchainsUnlisted";
  }
  if ("DeletesCargoHome" in warning) return "warnings.deletesCargoHome";
  if ("RemovesCargoInstalled" in warning) return "warnings.removesCargoInstalled";
  if ("LeavesShellConfigLine" in warning) {
    // "will print an error" only for a line rustup's own sourcing form
    // spells with every line above it standing alone (the core decides,
    // `rustup::classify_leftover`); anything else that mentions the env
    // file "may".
    return warning.LeavesShellConfigLine.certain
      ? "warnings.leavesShellConfigLine"
      : "warnings.leavesShellConfigLineMaybe";
  }
  if ("UninstallScope" in warning) return UNINSTALL_SCOPE_KEYS[warning.UninstallScope.what];
  if ("CaskUninstallStep" in warning) return CASK_STEP_KEYS[warning.CaskUninstallStep.step];
  if ("Message" in warning) return null;
  const unhandled: never = warning;
  return unhandled;
}

/**
 * Interpolation values for `t(warningKey(warning), warningArgs(warning))`.
 * Exhaustive over the payload variants for the same reason `warningKey`
 * is (`faultArgs` in `src/lib/format.ts` is the model): a payload variant
 * with a key but no values here would render its sentence with a literal
 * `{{path}}` in it. Bare-string variants carry nothing, so one `{}` covers
 * them all.
 */
export function warningArgs(warning: Warning): Record<string, unknown> {
  if (typeof warning === "string") return {};
  if ("WouldBreak" in warning) {
    const names = warning.WouldBreak.names;
    return { count: names.length, names: names.join(", ") };
  }
  if ("ThirdPartyRegistry" in warning) return { host: warning.ThirdPartyRegistry.host };
  if ("WillTrash" in warning) return { path: warning.WillTrash.path };
  if ("WillKeep" in warning) return { path: warning.WillKeep.path };
  if ("AlreadyGone" in warning) return { path: warning.AlreadyGone.path };
  if ("RemovesToolchains" in warning) {
    const { path, names } = warning.RemovesToolchains;
    return names.length > 0 ? { path, names: names.join(", ") } : { path };
  }
  if ("DeletesCargoHome" in warning) return { path: warning.DeletesCargoHome.path };
  if ("RemovesCargoInstalled" in warning) {
    const names = warning.RemovesCargoInstalled.names;
    return { count: names.length, names: names.join(", ") };
  }
  if ("LeavesShellConfigLine" in warning) return { path: warning.LeavesShellConfigLine.path };
  // Its `{{name}}` is the row's, which only the page has (`warningText`).
  if ("UninstallScope" in warning) return {};
  if ("CaskUninstallStep" in warning) {
    const items = warning.CaskUninstallStep.items;
    return items.length > 0 ? { count: items.length, items: items.join(", ") } : {};
  }
  if ("Message" in warning) return {};
  const unhandled: never = warning;
  return unhandled;
}

/**
 * The raw text of a `Message` warning, or `null` for anything else.
 *
 * Deliberately not looked up through `t()`: the text is built at runtime
 * on the Rust side (a subprocess's stderr, a network error) and shown
 * exactly as received, in whatever language it
 * came in, because localising it is backlogged behind
 * `show_technical_details` (spec §6) rather than solved by this enum.
 */
export function warningMessage(warning: Warning): string | null {
  if (typeof warning !== "string" && "Message" in warning) return warning.Message;
  return null;
}

/** Whatever `useTranslation()`'s `t` needs to look a key up; kept minimal
 *  so this module does not have to import i18next's own types. */
type Translate = (key: string, options?: Record<string, unknown>) => string;

/**
 * One `Warning`, rendered: `t(warningKey(warning), warningArgs(warning))`
 * for a fixed warning, `warningMessage(warning)` for a `Message`. The
 * convenience wrapper every call site actually wants; `warningKey`/
 * `warningArgs`/`warningMessage` stay exported and `t()`-free for testing.
 * `subject` is the name of the tool the plan is about, as its row names
 * it: an `UninstallScope` sentence says it, and without one has no line
 * (`null`) rather than a line with an empty name in it. Otherwise, with
 * `warningKey` exhaustive, a fixed warning always has a key and a
 * `Message` always has its text.
 */
export function warningText(t: Translate, warning: Warning, subject?: string): string | null {
  const key = warningKey(warning);
  if (typeof warning !== "string" && "UninstallScope" in warning) {
    return key === null || subject === undefined ? null : t(key, { name: subject });
  }
  return key ? t(key, warningArgs(warning)) : warningMessage(warning);
}

/**
 * The key of a warning's longer why, which a confirmation puts behind the
 * line's ⓘ (the copy table's `<key>Detail`), or null when the line says
 * all there is: what a kept path is and why it stays, what rustup's
 * permanent deletions and the line it leaves in a startup file mean for
 * you, and which Homebrew setting brings back an autoremove Canager turns
 * off. The line keeps what decides whether to go on -- "permanently
 * deletes", the path, what goes with it; the ⓘ has the rest. The Cargo
 * folder's line has nothing behind it: that the whole folder goes, and
 * none of it to the Trash, is what decides.
 *
 * Every variant is named, so one added to `Warning` fails `tsc` here; at
 * run time, a variant this build does not know has nothing behind its ⓘ.
 */
export function warningDetailKey(warning: Warning): string | null {
  if (typeof warning === "string") {
    switch (warning) {
      case "HomebrewAutoremoves":
        return "warnings.homebrewAutoremovesDetail";
      case "HomebrewCleanupAutoremoves":
        return "warnings.homebrewCleanupAutoremovesDetail";
      case "DependentsUnknown":
      case "CompilesLocally":
      case "NonRegistrySource":
      case "HomebrewRustupLosesToolchains":
      case "EditsShellConfig":
        return null;
      default: {
        const unhandled: never = warning;
        void unhandled;
        return null;
      }
    }
  }
  if ("WillKeep" in warning) return KEPT_WHAT_DETAIL_KEYS[warning.WillKeep.what];
  // The listed and the unlisted sentence share one why.
  if ("RemovesToolchains" in warning) return "warnings.removesToolchainsDetail";
  if ("RemovesCargoInstalled" in warning) return "warnings.removesCargoInstalledDetail";
  if ("LeavesShellConfigLine" in warning) {
    return warning.LeavesShellConfigLine.certain
      ? "warnings.leavesShellConfigLineDetail"
      : "warnings.leavesShellConfigLineMaybeDetail";
  }
  if (
    "WouldBreak" in warning ||
    "ThirdPartyRegistry" in warning ||
    "WillTrash" in warning ||
    "AlreadyGone" in warning ||
    "DeletesCargoHome" in warning ||
    "UninstallScope" in warning ||
    "CaskUninstallStep" in warning ||
    "Message" in warning
  ) {
    return null;
  }
  const unhandled: never = warning;
  void unhandled;
  return null;
}

/**
 * Where a warning goes in a confirmation (the copy table's C4 premise):
 * `scope`, the one sentence an uninstall says directly under the tool
 * about what goes and what stays; `trash`, what a path-list uninstall
 * moves to the Trash, and what it found already gone from there; `keep`,
 * what it leaves where it is; and `note`, everything else -- what to know
 * before you continue, from a dependency Canager could not check to a
 * cask's extra uninstall steps and rustup deleting a folder for good.
 *
 * Every payload variant is named, so one added to `Warning` fails `tsc`
 * here; at run time, a variant this build does not know is a `note`, the
 * group no one skims past. So is every bare-string variant.
 */
export type WarningGroup = "scope" | "trash" | "keep" | "note";

export function warningGroup(warning: Warning): WarningGroup {
  if (typeof warning === "string") return "note";
  if ("UninstallScope" in warning) return "scope";
  if ("WillTrash" in warning || "AlreadyGone" in warning) return "trash";
  if ("WillKeep" in warning) return "keep";
  if (
    "WouldBreak" in warning ||
    "ThirdPartyRegistry" in warning ||
    "RemovesToolchains" in warning ||
    "DeletesCargoHome" in warning ||
    "RemovesCargoInstalled" in warning ||
    "LeavesShellConfigLine" in warning ||
    "CaskUninstallStep" in warning ||
    "Message" in warning
  ) {
    return "note";
  }
  const unhandled: never = warning;
  void unhandled;
  return "note";
}

/**
 * Whether a warning says the uninstall deletes something for good, with
 * nothing moved to the Trash: rustup's own uninstall, which deletes the
 * rustup folder with its toolchains, the Cargo folder and the programs in
 * it (their sentences start "Permanently deletes"), and a cask whose
 * recorded uninstall deletes paths -- by `delete:` or an uninstall step of
 * type `remove` -- named or not (`CaskUninstallStep` `Deletes` and
 * `DeletesUnnamed`: "Also permanently deletes"). The uninstall
 * confirmation's button then says so
 * too (`uninstall.confirmPermanent`). Only what a line says counts: a plan
 * with no such line may well delete files -- `brew uninstall` does, and so
 * does the autoremove Homebrew's two lines speak of -- but says nothing
 * about the Trash, and neither does its button.
 *
 * Every variant is named, so one added to `Warning` fails `tsc` here; at
 * run time, a variant this build does not know is not one, and its line
 * is still said as a note (`warningGroup`).
 */
export function deletesForGood(warning: Warning): boolean {
  if (typeof warning === "string") {
    switch (warning) {
      case "DependentsUnknown":
      case "CompilesLocally":
      case "NonRegistrySource":
      case "HomebrewRustupLosesToolchains":
      case "EditsShellConfig":
      case "HomebrewAutoremoves":
      case "HomebrewCleanupAutoremoves":
        return false;
      default: {
        const unhandled: never = warning;
        void unhandled;
        return false;
      }
    }
  }
  if ("RemovesToolchains" in warning || "DeletesCargoHome" in warning || "RemovesCargoInstalled" in warning) {
    return true;
  }
  if ("CaskUninstallStep" in warning) {
    const step = warning.CaskUninstallStep.step;
    return step === "Deletes" || step === "DeletesUnnamed";
  }
  if (
    "WouldBreak" in warning ||
    "ThirdPartyRegistry" in warning ||
    "WillTrash" in warning ||
    "WillKeep" in warning ||
    "AlreadyGone" in warning ||
    "LeavesShellConfigLine" in warning ||
    "UninstallScope" in warning ||
    "Message" in warning
  ) {
    return false;
  }
  const unhandled: never = warning;
  void unhandled;
  return false;
}

/** One line of a confirmation: its sentence, and its longer why for an ⓘ, if it has one. */
export interface WarningLine {
  text: string;
  detail: string | null;
}

/** `warning`'s line, or null only for what `warningText` gives none. */
export function warningLine(t: Translate, warning: Warning, subject?: string): WarningLine | null {
  const text = warningText(t, warning, subject);
  if (text === null) return null;
  const detailKey = warningDetailKey(warning);
  return { text, detail: detailKey === null ? null : t(detailKey) };
}

/** A plan's warnings as lines, each in its group, in the plan's order. */
export type WarningLines = Record<WarningGroup, WarningLine[]>;

/**
 * `warnings`, rendered and grouped (`warningGroup`), in order. With
 * `affected`, the plan's own list of what still needs the package -- which
 * the uninstall confirmation shows once, as its own list -- a `WouldBreak`
 * naming the same packages is left out rather than said a second time:
 * Homebrew's preview fills both from one `brew uses`
 * (crates/canager-core/src/adapters/brew/mod.rs). With `subject`, the
 * tool's name as its row shows it, an uninstall's scope sentence says it;
 * without, there is no scope line (`warningText`).
 */
export function warningLines(
  t: Translate,
  warnings: Warning[],
  affected: string[] = [],
  subject?: string,
): WarningLines {
  const lines: WarningLines = { scope: [], trash: [], keep: [], note: [] };
  for (const warning of warnings) {
    if (affected.length > 0 && typeof warning !== "string" && "WouldBreak" in warning) continue;
    const line = warningLine(t, warning, subject);
    if (line !== null) lines[warningGroup(warning)].push(line);
  }
  return lines;
}
