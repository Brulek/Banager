/**
 * How `Plan.warnings` and `UpdateCandidate.warnings` become text in the
 * user's language, and where each goes in a confirmation. Same split as
 * `outcomeKey`/`outcomeArgs` in `src/lib/format.ts`: no JSX here, and `t`
 * only as a parameter, so both confirmations and the updates page share
 * one rule and it is testable without rendering anything.
 */
import type { CaskStep, KeptData, KeptWhat, RemoveCheck, RemovedWhat, UninstallScope, Warning } from "./types";
import { modelDownloadNote } from "./modelDownload";

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
 * What each kind of data an uninstall leaves behind (`Warning.KeepsData`)
 * is, said under its path. Only the uninstall confirmation's own group
 * shows these, with the size and Copy Path (`KeptDataGroup`); no screen
 * says them as a warning line, so `warningKey` gives them none. A
 * `Record` over `KeptData`, so a kind without copy fails `tsc`.
 */
export const KEPT_DATA_KEYS: Record<KeptData, string> = {
  ToolData: "keepsData.what.ToolData",
  Models: "keepsData.what.Models",
};

/**
 * Why each kind of kept path stays, where the line itself does not say
 * it: behind the line's ⓘ. The first two need none -- your settings stay
 * because they are yours. A `Record` over `KeptWhat`, so a kind added
 * without an answer here fails `tsc`.
 */
export const KEPT_WHAT_DETAIL_KEYS: Record<KeptWhat, string | null> = {
  Settings: null,
  SettingsAndHistory: null,
  ToolState: "warnings.willKeep.ToolStateDetail",
  ShellConfigLines: "warnings.willKeep.ShellConfigLinesDetail",
  OutsideHome: "warnings.willKeep.OutsideHomeDetail",
  NotOurs: "warnings.willKeep.NotOursDetail",
  InstallerCache: "warnings.willKeep.InstallerCacheDetail",
};

/**
 * The line for what `HOMEBREW_NO_CLEANUP_FORMULAE` leaves out of the
 * Homebrew lines before it: their older versions (after
 * `HomebrewPeriodicCleanup`), the autoremove (after `HomebrewAutoremoves`),
 * or both (after `HomebrewCleanupAutoremoves`). Spelled out, so the
 * reachability test finds every key.
 */
const NO_CLEANUP_FORMULAE_KEYS = {
  oldVersions: "uninstall.noCleanupFormulae.oldVersions",
  autoremove: "uninstall.noCleanupFormulae.autoremove",
  both: "uninstall.noCleanupFormulae.both",
} as const;

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
  HomebrewCaskStepsAutoremoves: "warnings.uninstallScope.HomebrewCaskStepsAutoremoves",
  HomebrewCaskStepsUnseen: "warnings.uninstallScope.HomebrewCaskStepsUnseen",
  HomebrewCaskStepsOnly: "warnings.uninstallScope.HomebrewCaskStepsOnly",
  HomebrewCaskStepsOnlyUnseen: "warnings.uninstallScope.HomebrewCaskStepsOnlyUnseen",
  HomebrewCask: "warnings.uninstallScope.HomebrewCask",
  HomebrewCaskPlainThirdParty: "uninstall.scopeMore.HomebrewCaskPlainThirdParty",
  HomebrewCaskRuby: "uninstall.scopeMore.HomebrewCaskRuby",
  HomebrewCaskStepsOnlyRuby: "uninstall.scopeMore.HomebrewCaskStepsOnlyRuby",
  HomebrewCaskPlainRuby: "uninstall.scopeMore.HomebrewCaskPlainRuby",
  HomebrewCaskStepsIfTrusted: "uninstall.scopeMore.HomebrewCaskStepsIfTrusted",
  HomebrewCaskStepsOnlyIfTrusted: "uninstall.scopeMore.HomebrewCaskStepsOnlyIfTrusted",
  Npm: "warnings.uninstallScope.Npm",
  Pipx: "warnings.uninstallScope.Pipx",
  Uv: "warnings.uninstallScope.Uv",
  Cargo: "warnings.uninstallScope.Cargo",
  Ollama: "warnings.uninstallScope.Ollama",
};

/**
 * The line for each kind of extra step a cask's recorded uninstall takes.
 * A `Record` over `CaskStep`. Each but `DeletesUnnamed` and `RunsOwnSteps`,
 * which name nothing, pluralises on `{{count}}`, and each of those but
 * `RemovesServices` and `QuitsApps` interpolates `{{items}}`. Those two
 * count what they would name: a background service's label and an app
 * Banager did not find on the Mac are reverse-DNS ids
 * (`com.microsoft.VSCode.ShipIt`) that tell a person nothing, so the line
 * says how many -- unless one of the ids is a pattern
 * (`MATCHING_STEP_KEYS`) -- and the ids are behind its ⓘ
 * (`warningDetailKey`). An app it did find is named, as Finder names it
 * (`QuitsNamedApps`).
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
  QuitsNamedApps: "warnings.caskStep.QuitsNamedApps",
};

/**
 * The line for a counted step one of whose ids has a `*` in it. Homebrew
 * 7.0.6-70 takes such an id as a pattern: `launchctl:` stops and removes
 * every running service whose name matches it
 * (`abstract_uninstall.rb:173-181` and `:247-255`), and `quit:` and
 * `signal:` quit or signal every running app whose id matches it
 * (`expand_bundle_id`, `:371-384`, called at `:92` and `:477`). How many
 * that is shows only as the uninstall runs, so the number of ids would be
 * wrong either way -- Adobe Creative Cloud's six service names and
 * `com.adobe.CCXProcess.*` are not seven services. The line gives no
 * number; the ids, the pattern with them, stay behind its ⓘ
 * (`warningDetailKey`).
 */
const MATCHING_STEP_KEYS: Record<"RemovesServices" | "QuitsApps", string> = {
  RemovesServices: "warnings.caskStep.RemovesServicesMatching",
  QuitsApps: "warnings.caskStep.QuitsAppsMatching",
};

/** Whether a counted step's id is a pattern Homebrew matches against what is running. */
function isPattern(id: string): boolean {
  return id.includes("*");
}

/** Which check a `remove` step makes of each path before it deletes it. */
type RemoveCheckKind = "LinkTargetContains" | "ContentContains" | "LinkTargetAndContentContain";

/**
 * The line for the paths a `remove` step deletes only where they pass its
 * check (`RemoveCheck`, a `Deletes` or `DeletesUnnamed` step's `only_if`),
 * named or found only as the step runs. Each interpolates the check's text
 * -- `{{link}}` for a link's target, `{{content}}` for a file's contents
 * -- and a named line `{{items}}`, pluralised on `{{count}}`. A `Record`
 * over the check's kinds, so one added without copy fails `tsc`.
 */
const CHECKED_DELETE_KEYS: Record<RemoveCheckKind, Record<"Deletes" | "DeletesUnnamed", string>> = {
  LinkTargetContains: {
    Deletes: "warnings.caskStep.DeletesLinks",
    DeletesUnnamed: "warnings.caskStep.DeletesUnnamedLinks",
  },
  ContentContains: {
    Deletes: "warnings.caskStep.DeletesFilesContaining",
    DeletesUnnamed: "warnings.caskStep.DeletesUnnamedFilesContaining",
  },
  LinkTargetAndContentContain: {
    Deletes: "warnings.caskStep.DeletesLinksToFilesContaining",
    DeletesUnnamed: "warnings.caskStep.DeletesUnnamedLinksToFilesContaining",
  },
};

/** `check`'s kind; null for one this build does not know. */
function removeCheckKind(check: RemoveCheck): RemoveCheckKind | null {
  if ("LinkTargetContains" in check) return "LinkTargetContains";
  if ("ContentContains" in check) return "ContentContains";
  if ("LinkTargetAndContentContain" in check) return "LinkTargetAndContentContain";
  const unhandled: never = check;
  void unhandled;
  return null;
}

/** The text `check` looks for, as its line interpolates it. */
function removeCheckArgs(check: RemoveCheck): Record<string, string> {
  if ("LinkTargetContains" in check) return { link: check.LinkTargetContains };
  if ("ContentContains" in check) return { content: check.ContentContains };
  if ("LinkTargetAndContentContain" in check) {
    const { link_target: link, content } = check.LinkTargetAndContentContain;
    return { link, content };
  }
  const unhandled: never = check;
  void unhandled;
  return {};
}

/**
 * A cask step's key: its kind's, or, for the paths a `remove` step deletes
 * only where they pass a check, that check's line, or, for services it
 * stops or apps it quits one of whose ids is a pattern, the line with no
 * number (`MATCHING_STEP_KEYS`). The core sets `only_if` on `Deletes` and
 * `DeletesUnnamed` alone (`cask_receipt::classify`); a check this build
 * does not know gets its kind's line, which says more goes, not less.
 */
function caskStepKey(step: CaskStep, items: string[], onlyIf: RemoveCheck | undefined): string {
  const kind = onlyIf === undefined ? null : removeCheckKind(onlyIf);
  if (kind !== null && (step === "Deletes" || step === "DeletesUnnamed")) {
    return CHECKED_DELETE_KEYS[kind][step];
  }
  if ((step === "RemovesServices" || step === "QuitsApps") && items.some(isPattern)) {
    return MATCHING_STEP_KEYS[step];
  }
  return CASK_STEP_KEYS[step];
}

/**
 * The `warnings.*` key for a `Warning`'s copy, or `null` for the `Message`
 * catch-all, whose text is read straight off the wire (see
 * `warningMessage` below), for `KeepsData`, which only the uninstall
 * confirmation's own group renders (`KeptDataGroup`), and for
 * `NeededBySource`, which it lists with what Homebrew names as still
 * needing the package (`neededByItem` in src/lib/neededBy.ts): it needs
 * the source's name, which only the page has.
 *
 * Exhaustive, the way `faultKey` in `src/lib/format.ts` is: every variant
 * of `Warning` is named here, and the `never` defaults make `tsc` fail on
 * one that is not. It used to `return null` for anything it did not
 * recognise, and `warningLines` drops a `null` without a word -- so a
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
      case "DownloadsModelChanges":
        return "warnings.downloadsModelChanges";
      case "NonRegistrySource":
        return "warnings.nonRegistrySource";
      case "TransientLookupFailure":
        return "warnings.transientLookupFailure";
      case "NotLookedUpHere":
        return "notLookedUp.here";
      case "HomebrewRustupLosesToolchains":
        return "warnings.homebrewRustupLosesToolchains";
      case "EditsShellConfig":
        return "warnings.editsShellConfig";
      case "HomebrewAutoremoves":
        return "warnings.homebrewAutoremoves";
      case "HomebrewPeriodicCleanup":
        return "warnings.homebrewPeriodicCleanup";
      case "HomebrewCleanupAutoremoves":
        return "warnings.homebrewCleanupAutoremoves";
      case "HomebrewMayAutoremove":
        return "unreadInProtectedPlace.homebrewMayAutoremove";
      case "HomebrewMayCleanUp":
        return "unreadInProtectedPlace.homebrewMayCleanUp";
      case "HomebrewCleanupMayAutoremove":
        return "unreadInProtectedPlace.homebrewCleanupMayAutoremove";
      case "HomebrewMayAutoUpdate":
        return "unreadBrewEnv.homebrewMayAutoUpdate";
      default: {
        const unhandled: never = warning;
        return unhandled;
      }
    }
  }
  if ("WouldBreak" in warning) return "warnings.wouldBreak";
  if ("SecureConnectionFailed" in warning) return "secureConnection.failed";
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
  if ("ShellConfigUnread" in warning) return "unreadInProtectedPlace.shellConfigUnread";
  if ("HomebrewForgetsTrust" in warning) return "uninstall.forgetsTrust";
  // U9: plural on `{{count}}`, the versions it names.
  if ("HomebrewCleansUpOldVersions" in warning) return "brewVersions.cleansUp";
  if ("HomebrewRemovesEveryVersion" in warning) return "brewVersions.removesEvery";
  // y1-keg: a keg-only formula linked by hand is linked back after its update.
  if ("HomebrewRelinksAfterUpdate" in warning) return "kegLinks.relinks";
  if ("HomebrewNoCleanupFormulae" in warning) {
    const { old_versions: oldVersions, autoremove } = warning.HomebrewNoCleanupFormulae;
    return NO_CLEANUP_FORMULAE_KEYS[oldVersions ? (autoremove ? "both" : "oldVersions") : "autoremove"];
  }
  if ("UninstallScope" in warning) return UNINSTALL_SCOPE_KEYS[warning.UninstallScope.what];
  if ("CaskUninstallStep" in warning) {
    const { step, items, only_if: onlyIf } = warning.CaskUninstallStep;
    return caskStepKey(step, items, onlyIf);
  }
  // Its own group renders it (`KeptDataGroup`, the list of what still needs
  // the package), never a line.
  if ("KeepsData" in warning || "NeededBySource" in warning || "Message" in warning) return null;
  const unhandled: never = warning;
  return unhandled;
}

/**
 * Interpolation values for `t(warningKey(warning), warningArgs(warning))`.
 * Exhaustive over the payload variants for the same reason `warningKey`
 * is (`faultArgs` in `src/lib/format.ts` is the model): a payload variant
 * with a key but no values here would render its sentence with a literal
 * `{{path}}` in it. Bare-string variants carry nothing, so one `{}` covers
 * them all. A list the sentence names -- what still needs a package,
 * rustup's toolchains and the programs it deletes, a cask step's items --
 * is joined with `separator`: the window's language's
 * (`common.listSeparator`, 「、」 in Chinese) where `warningText` renders
 * it, never an English comma inside a Chinese sentence.
 */
export function warningArgs(warning: Warning, separator = ", "): Record<string, unknown> {
  if (typeof warning === "string") return {};
  if ("WouldBreak" in warning) {
    const names = warning.WouldBreak.names;
    return { count: names.length, names: names.join(separator) };
  }
  if ("SecureConnectionFailed" in warning) return { host: warning.SecureConnectionFailed.host };
  if ("ThirdPartyRegistry" in warning) return { host: warning.ThirdPartyRegistry.host };
  if ("WillTrash" in warning) return { path: warning.WillTrash.path };
  if ("WillKeep" in warning) return { path: warning.WillKeep.path };
  if ("AlreadyGone" in warning) return { path: warning.AlreadyGone.path };
  if ("RemovesToolchains" in warning) {
    const { path, names } = warning.RemovesToolchains;
    return names.length > 0 ? { path, names: names.join(separator) } : { path };
  }
  if ("DeletesCargoHome" in warning) return { path: warning.DeletesCargoHome.path };
  if ("RemovesCargoInstalled" in warning) {
    const names = warning.RemovesCargoInstalled.names;
    return { count: names.length, names: names.join(separator) };
  }
  if ("LeavesShellConfigLine" in warning) return { path: warning.LeavesShellConfigLine.path };
  if ("ShellConfigUnread" in warning) return { path: warning.ShellConfigUnread.path };
  if ("HomebrewForgetsTrust" in warning) return { name: warning.HomebrewForgetsTrust.name };
  if ("HomebrewCleansUpOldVersions" in warning) {
    const versions = warning.HomebrewCleansUpOldVersions.versions;
    return { count: versions.length, versions: versions.join(separator) };
  }
  if ("HomebrewRemovesEveryVersion" in warning) {
    const versions = warning.HomebrewRemovesEveryVersion.versions;
    return { count: versions.length, versions: versions.join(separator) };
  }
  if ("HomebrewRelinksAfterUpdate" in warning) {
    const { name, commands } = warning.HomebrewRelinksAfterUpdate;
    return { name, commands: commands.join(separator) };
  }
  if ("HomebrewNoCleanupFormulae" in warning) {
    const names = warning.HomebrewNoCleanupFormulae.names;
    return { count: names.length, names: names.join(separator) };
  }
  // Its `{{name}}` is the row's, which only the page has (`warningText`).
  if ("UninstallScope" in warning) return {};
  if ("CaskUninstallStep" in warning) {
    const { items, only_if: onlyIf } = warning.CaskUninstallStep;
    return {
      ...(items.length > 0 ? { count: items.length, items: items.join(separator) } : {}),
      ...(onlyIf === undefined ? {} : removeCheckArgs(onlyIf)),
    };
  }
  if ("KeepsData" in warning || "NeededBySource" in warning || "Message" in warning) return {};
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
 * (`null`) rather than a line with an empty name in it. A `KeepsData` has
 * no line either: `KeptDataGroup` renders it. Otherwise, with
 * `warningKey` exhaustive, a fixed warning always has a key and a
 * `Message` always has its text. `downloadBytes` is the most a model's
 * update downloads, which only its candidate knows
 * (`UpdateCandidate.download_bytes`): a `DownloadsModelChanges` says it
 * where it is known (`modelDownloadNote`), and its plain sentence where not.
 */
export function warningText(
  t: Translate,
  warning: Warning,
  subject?: string,
  downloadBytes?: number | null,
): string | null {
  const key = warningKey(warning);
  if (typeof warning !== "string" && "UninstallScope" in warning) {
    return key === null || subject === undefined ? null : t(key, { name: subject });
  }
  if (warning === "DownloadsModelChanges") {
    const sized = modelDownloadNote(t, downloadBytes);
    if (sized !== null) return sized;
  }
  return key ? t(key, warningArgs(warning, t("common.listSeparator"))) : warningMessage(warning);
}

/**
 * The key of a warning's longer why, which a confirmation puts behind the
 * line's ⓘ (the copy table's `<key>Detail`), or null when the line says
 * all there is: what a kept path is and why it stays, what rustup's
 * permanent deletions and the line it leaves in a startup file mean for
 * you, which Homebrew setting brings back a clean-up or an autoremove
 * Banager turns off, and the ids a cask's background services and the apps
 * Banager did not find go by, which their lines count, or say a pattern
 * matches, instead of naming. The line keeps what decides whether to go
 * on -- "permanently deletes", the path, what goes with it; the ⓘ has the
 * rest. The Cargo folder's line has nothing behind it: that the whole
 * folder goes, and none of it to the Trash, is what decides.
 *
 * Every variant is named, so one added to `Warning` fails `tsc` here; at
 * run time, a variant this build does not know has nothing behind its ⓘ.
 */
export function warningDetailKey(warning: Warning): string | null {
  if (typeof warning === "string") {
    switch (warning) {
      case "HomebrewAutoremoves":
        return "warnings.homebrewAutoremovesDetail";
      case "HomebrewPeriodicCleanup":
        return "warnings.homebrewPeriodicCleanupDetail";
      case "HomebrewCleanupAutoremoves":
        return "warnings.homebrewCleanupAutoremovesDetail";
      case "HomebrewMayAutoremove":
        return "unreadInProtectedPlace.homebrewMayAutoremoveDetail";
      case "HomebrewMayCleanUp":
        return "unreadInProtectedPlace.homebrewMayCleanUpDetail";
      case "HomebrewCleanupMayAutoremove":
        return "unreadInProtectedPlace.homebrewCleanupMayAutoremoveDetail";
      case "HomebrewMayAutoUpdate":
        return "unreadBrewEnv.homebrewMayAutoUpdateDetail";
      case "DependentsUnknown":
      case "CompilesLocally":
      case "DownloadsModelChanges":
      case "NonRegistrySource":
      case "TransientLookupFailure":
      case "NotLookedUpHere":
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
  // What a counted line did not name: the ids, as macOS names them.
  if ("CaskUninstallStep" in warning) {
    const { step, items } = warning.CaskUninstallStep;
    return (step === "RemovesServices" || step === "QuitsApps") && items.length > 0
      ? "warnings.caskStep.systemNames"
      : null;
  }
  // The listed and the unlisted sentence share one why.
  if ("RemovesToolchains" in warning) return "warnings.removesToolchainsDetail";
  if ("RemovesCargoInstalled" in warning) return "warnings.removesCargoInstalledDetail";
  if ("HomebrewForgetsTrust" in warning) return "uninstall.forgetsTrustDetail";
  // How: the command, and that it is what Homebrew does by default.
  if ("HomebrewCleansUpOldVersions" in warning) return "brewVersions.cleansUpDetail";
  // Which commands, and the command Banager runs if they are not back.
  if ("HomebrewRelinksAfterUpdate" in warning) {
    return warning.HomebrewRelinksAfterUpdate.commands.length > 0
      ? "kegLinks.relinksDetail"
      : "kegLinks.relinksDetailNoCommands";
  }
  if ("LeavesShellConfigLine" in warning) {
    return warning.LeavesShellConfigLine.certain
      ? "warnings.leavesShellConfigLineDetail"
      : "warnings.leavesShellConfigLineMaybeDetail";
  }
  if ("ShellConfigUnread" in warning) return "unreadInProtectedPlace.shellConfigUnreadDetail";
  if (
    "WouldBreak" in warning ||
    "NeededBySource" in warning ||
    "SecureConnectionFailed" in warning ||
    "ThirdPartyRegistry" in warning ||
    "WillTrash" in warning ||
    "AlreadyGone" in warning ||
    "DeletesCargoHome" in warning ||
    "HomebrewNoCleanupFormulae" in warning ||
    "HomebrewRemovesEveryVersion" in warning ||
    "UninstallScope" in warning ||
    "KeepsData" in warning ||
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
 * what it leaves where it is; `data`, a tool's settings and data or
 * Ollama's models, which no uninstall removes (`KeepsData`, which has no
 * line, so `warningLines` leaves this empty; `KeptDataGroup` shows it); and `note`, everything else -- what to know
 * before you continue, from a dependency Banager could not check to a
 * cask's extra uninstall steps and rustup deleting a folder for good.
 *
 * Every payload variant is named, so one added to `Warning` fails `tsc`
 * here; at run time, a variant this build does not know is a `note`, the
 * group no one skims past. So is every bare-string variant.
 */
export type WarningGroup = "scope" | "trash" | "keep" | "data" | "note";

export function warningGroup(warning: Warning): WarningGroup {
  if (typeof warning === "string") return "note";
  if ("UninstallScope" in warning) return "scope";
  if ("WillTrash" in warning || "AlreadyGone" in warning) return "trash";
  if ("WillKeep" in warning) return "keep";
  if ("KeepsData" in warning) return "data";
  if (
    "WouldBreak" in warning ||
    // No line (`warningKey`): the confirmation lists it with Homebrew's
    // dependents instead.
    "NeededBySource" in warning ||
    "SecureConnectionFailed" in warning ||
    "ThirdPartyRegistry" in warning ||
    "RemovesToolchains" in warning ||
    "DeletesCargoHome" in warning ||
    "RemovesCargoInstalled" in warning ||
    "LeavesShellConfigLine" in warning ||
    "ShellConfigUnread" in warning ||
    "HomebrewNoCleanupFormulae" in warning ||
    "HomebrewForgetsTrust" in warning ||
    "HomebrewCleansUpOldVersions" in warning ||
    "HomebrewRemovesEveryVersion" in warning ||
    "HomebrewRelinksAfterUpdate" in warning ||
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
 * type `remove` -- named or not, and whether or not that step deletes a
 * path only where it passes a check (`CaskUninstallStep` `Deletes` and
 * `DeletesUnnamed`, with or without `only_if`: "Also permanently deletes").
 * The uninstall confirmation's button then says so
 * too (`uninstall.confirmPermanent`). Only what a line says counts: a plan
 * with no such line may well delete files -- `brew uninstall` does, and so
 * does the autoremove Homebrew's two lines speak of -- and its button says
 * plain Uninstall; where its source's sentence holds it, its text says
 * that nothing it deletes goes to the Trash (`skipsTrash`).
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
      case "DownloadsModelChanges":
      case "NonRegistrySource":
      case "TransientLookupFailure":
      case "NotLookedUpHere":
      case "HomebrewRustupLosesToolchains":
      case "EditsShellConfig":
      case "HomebrewAutoremoves":
      case "HomebrewPeriodicCleanup":
      case "HomebrewCleanupAutoremoves":
      case "HomebrewMayAutoremove":
      case "HomebrewMayCleanUp":
      case "HomebrewCleanupMayAutoremove":
      case "HomebrewMayAutoUpdate":
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
    "NeededBySource" in warning ||
    "SecureConnectionFailed" in warning ||
    "ThirdPartyRegistry" in warning ||
    "WillTrash" in warning ||
    "WillKeep" in warning ||
    "AlreadyGone" in warning ||
    "LeavesShellConfigLine" in warning ||
    "ShellConfigUnread" in warning ||
    "HomebrewNoCleanupFormulae" in warning ||
    "HomebrewForgetsTrust" in warning ||
    // `brew uninstall` and `brew cleanup` delete what they always delete:
    // the old versions are said by name, as how it goes (U9).
    "HomebrewCleansUpOldVersions" in warning ||
    "HomebrewRemovesEveryVersion" in warning ||
    // `brew link` deletes nothing (y1-keg: never `--overwrite`).
    "HomebrewRelinksAfterUpdate" in warning ||
    "UninstallScope" in warning ||
    "KeepsData" in warning ||
    "Message" in warning
  ) {
    return false;
  }
  const unhandled: never = warning;
  void unhandled;
  return false;
}

/**
 * Whether what an uninstall with each source's sentence (`UninstallScope`)
 * deletes is deleted outright, none of it moved to the Trash. Each runs
 * the source's own uninstall command, and each command deletes files in
 * place. Homebrew (7.0.7-9, this Mac's): a formula's keg
 * (`Keg#uninstall`, `keg.rb:325-339`), and the formulae its autoremove
 * takes the same way; a cask's app and the other artifacts it moved into
 * place are copied back into the Caskroom and deleted from their target
 * (`Moved#uninstall_phase`, `#move_back`, `#delete`,
 * `cask/artifact/moved.rb:45-48`, `:200-255`, through
 * `Utils.gain_permissions_remove`, `cask/utils.rb:59-85`), then that copy
 * is deleted the same way (`purge_versioned_files` from
 * `Installer#uninstall`, `cask/installer.rb:627-642`, `:826-835`), its
 * links are unlinked, and every recorded step but `trash:` deletes in
 * place -- `trash:` alone moves paths to the Trash (`uninstall_trash`,
 * `abstract_uninstall.rb:663-686`), and says so on its own line
 * (`CaskStep` `Trashes`), so a plan with one says nothing of the kind
 * (`skipsTrash`). `npm uninstall -g` (7 or later), `pipx uninstall`,
 * `uv tool uninstall`, `cargo uninstall` and `ollama rm` delete a folder,
 * its commands, a program's files, a model's manifest and unused layers.
 *
 * Not the cask sentences that cannot see or read everything the
 * uninstall does: a program the cask names or Ruby around the uninstall
 * may move something to the Trash for all Banager knows
 * (`HomebrewCaskStepsUnseen`, `HomebrewCaskStepsOnlyUnseen`), a record
 * Banager could not read may hold a `trash:` step (`HomebrewCask`), and so
 * may the definition Homebrew runs in place of a Ruby record it cannot
 * load (`HomebrewCaskRuby`, `HomebrewCaskStepsOnlyRuby`,
 * `HomebrewCaskPlainRuby`) -- and a Ruby
 * record from a tap Banager cannot see Homebrew trusts runs its steps,
 * whatever they are, only if Homebrew does (`HomebrewCaskStepsIfTrusted`,
 * `HomebrewCaskStepsOnlyIfTrusted`). A tap's plain cask deletes in place
 * like any plain cask (`HomebrewCaskPlainThirdParty`). A plan
 * with no sentence -- npm 6, and the tools with their own installer, whose
 * confirmation lists what goes to the Trash or says that rustup's
 * deletions cannot be undone -- says nothing either. A `Record`, so a
 * sentence added without an answer here fails `tsc`.
 */
const SCOPE_SKIPS_TRASH: Record<UninstallScope, boolean> = {
  HomebrewFormulaOnly: true,
  HomebrewFormula: true,
  HomebrewCaskPlain: true,
  HomebrewCaskSteps: true,
  HomebrewCaskStepsAutoremoves: true,
  HomebrewCaskStepsUnseen: false,
  HomebrewCaskStepsOnly: true,
  HomebrewCaskStepsOnlyUnseen: false,
  HomebrewCask: false,
  HomebrewCaskPlainThirdParty: true,
  HomebrewCaskRuby: false,
  HomebrewCaskStepsOnlyRuby: false,
  HomebrewCaskPlainRuby: false,
  HomebrewCaskStepsIfTrusted: false,
  HomebrewCaskStepsOnlyIfTrusted: false,
  Npm: true,
  Pipx: true,
  Uv: true,
  Cargo: true,
  Ollama: true,
};

/**
 * Whether an uninstall's confirmation says that what it deletes does not
 * go to the Trash (`uninstall.endsSkipsTrash`), so no one goes looking for
 * it there afterwards: its plan has a sentence that holds it
 * (`SCOPE_SKIPS_TRASH`) and no cask step that moves anything to the Trash.
 * At run time, a sentence this build does not know says nothing.
 */
export function skipsTrash(warnings: readonly Warning[]): boolean {
  let scope = false;
  for (const warning of warnings) {
    if (typeof warning === "string") continue;
    if ("UninstallScope" in warning) scope = SCOPE_SKIPS_TRASH[warning.UninstallScope.what] === true;
    if ("CaskUninstallStep" in warning && warning.CaskUninstallStep.step === "Trashes") return false;
  }
  return scope;
}

/**
 * Whether a warning's line is routine: true of nearly every update of its
 * kind, so that a list of several does not put the tool first for it. Only
 * U9's 「更新后会删除旧版本…」, which every Homebrew formula's update now
 * carries; were it to count, the tools with a caution or a major update
 * would sink below any number of formulae that say only that.
 */
export function isRoutineNote(warning: Warning): boolean {
  return typeof warning !== "string" && "HomebrewCleansUpOldVersions" in warning;
}

/**
 * Whether a warning's line is a caution -- something a person may not
 * expect and should weigh before going on -- which a confirmation marks
 * with a small ⚠︎ before its words (spec R6): what else goes or stops
 * working, what is deleted for good, a source Banager cannot vouch for, a
 * dependency it could not check, a cask's extra steps, and anything this
 * build has no words for. Not one that only says how it goes: that it
 * compiles, that a model downloads what changed, where it came from, which line rustup takes out, what moves to
 * the Trash or stays, and what the uninstall covers.
 *
 * Every variant is named, so one added to `Warning` fails `tsc` here; at
 * run time, a variant this build does not know is a caution.
 */
export function isCaution(warning: Warning): boolean {
  if (typeof warning === "string") {
    switch (warning) {
      case "DependentsUnknown":
      case "HomebrewRustupLosesToolchains":
      case "HomebrewAutoremoves":
      case "HomebrewPeriodicCleanup":
      case "HomebrewCleanupAutoremoves":
      case "HomebrewMayAutoremove":
      case "HomebrewMayCleanUp":
      case "HomebrewCleanupMayAutoremove":
      case "HomebrewMayAutoUpdate":
        return true;
      case "CompilesLocally":
      case "DownloadsModelChanges":
      case "NonRegistrySource":
      case "TransientLookupFailure":
      case "NotLookedUpHere":
      case "EditsShellConfig":
        return false;
      default: {
        const unhandled: never = warning;
        void unhandled;
        return true;
      }
    }
  }
  if (
    // Why a row could not be checked, as `NonRegistrySource` is: said on
    // its row, never in a confirmation.
    "SecureConnectionFailed" in warning ||
    "WillTrash" in warning ||
    "WillKeep" in warning ||
    "AlreadyGone" in warning ||
    "HomebrewNoCleanupFormulae" in warning ||
    "HomebrewForgetsTrust" in warning ||
    // What Homebrew does by default, each version named (U9).
    "HomebrewCleansUpOldVersions" in warning ||
    "HomebrewRemovesEveryVersion" in warning ||
    // How Banager keeps what the person linked in Terminal (y1-keg).
    "HomebrewRelinksAfterUpdate" in warning ||
    "UninstallScope" in warning ||
    "KeepsData" in warning
  ) {
    return false;
  }
  if (
    "WouldBreak" in warning ||
    "NeededBySource" in warning ||
    "ThirdPartyRegistry" in warning ||
    "RemovesToolchains" in warning ||
    "DeletesCargoHome" in warning ||
    "RemovesCargoInstalled" in warning ||
    "LeavesShellConfigLine" in warning ||
    "ShellConfigUnread" in warning ||
    "CaskUninstallStep" in warning ||
    "Message" in warning
  ) {
    return true;
  }
  const unhandled: never = warning;
  void unhandled;
  return true;
}

/**
 * One line of a confirmation: its sentence, its longer why for an ⓘ if it
 * has one, and whether it is a caution (`isCaution`), marked ⚠︎.
 */
export interface WarningLine {
  text: string;
  detail: string | null;
  caution: boolean;
}

/** `warning`'s line, or null only for what `warningText` gives none. */
export function warningLine(
  t: Translate,
  warning: Warning,
  subject?: string,
  downloadBytes?: number | null,
): WarningLine | null {
  const text = warningText(t, warning, subject, downloadBytes);
  if (text === null) return null;
  const detailKey = warningDetailKey(warning);
  // With the line's own values: the ids a counted line did not name.
  return {
    text,
    detail: detailKey === null ? null : t(detailKey, warningArgs(warning, t("common.listSeparator"))),
    caution: isCaution(warning),
  };
}

/** A plan's warnings as lines, each in its group, in the plan's order. */
export type WarningLines = Record<WarningGroup, WarningLine[]>;

/**
 * `warnings`, rendered and grouped (`warningGroup`), in order. With
 * `affected`, the plan's own list of what still needs the package -- which
 * the uninstall confirmation shows once, as its own list -- a `WouldBreak`
 * naming the same packages is left out rather than said a second time:
 * Homebrew's preview fills both from one `brew uses`
 * (crates/banager-core/src/adapters/brew/mod.rs). With `subject`, the
 * tool's name as its row shows it, an uninstall's scope sentence says it;
 * without, there is no scope line (`warningText`). With `downloadBytes`,
 * an update's candidate's `download_bytes`, a model's note says the most
 * it downloads (`warningText`).
 */
export function warningLines(
  t: Translate,
  warnings: Warning[],
  affected: string[] = [],
  subject?: string,
  downloadBytes?: number | null,
): WarningLines {
  const lines: WarningLines = { scope: [], trash: [], keep: [], data: [], note: [] };
  // Where the plan names what stays (`KeepsData`, the 「卸载后会保留」
  // group), the scope sentence leaves out its own general "settings and
  // data elsewhere are kept": the group says which, and where.
  const keptNamed = warnings.some((warning) => typeof warning !== "string" && "KeepsData" in warning);
  // Where every version of a formula goes (U9), its sentence does not call
  // what goes 「这一版」, which reads as "this one version" beside the line
  // that names them all.
  const everyVersion = warnings.some(
    (warning) => typeof warning !== "string" && "HomebrewRemovesEveryVersion" in warning,
  );
  for (const warning of warnings) {
    if (affected.length > 0 && typeof warning !== "string" && "WouldBreak" in warning) continue;
    const what =
      subject !== undefined && typeof warning !== "string" && "UninstallScope" in warning
        ? warning.UninstallScope.what
        : undefined;
    const short =
      what === undefined
        ? undefined
        : everyVersion && EVERY_VERSION_SCOPE_KEYS[what] !== undefined
          ? EVERY_VERSION_SCOPE_KEYS[what][keptNamed ? "withKept" : "plain"]
          : keptNamed
            ? SCOPE_WITH_KEPT_KEYS[what]
            : undefined;
    const line = warningLine(t, warning, subject, downloadBytes);
    if (line === null) continue;
    lines[warningGroup(warning)].push(short === undefined ? line : { ...line, text: t(short, { name: subject ?? "" }) });
  }
  return lines;
}

/**
 * A formula's scope sentences where its uninstall deletes every version
 * (`HomebrewRemovesEveryVersion`, U9): what Homebrew installed of it, not
 * 「这一版」; `withKept` without the general "settings and data elsewhere
 * are kept", as `SCOPE_WITH_KEPT_KEYS`.
 */
const EVERY_VERSION_SCOPE_KEYS: Partial<Record<UninstallScope, { plain: string; withKept: string }>> = {
  HomebrewFormulaOnly: {
    plain: "brewVersions.scopeEvery.HomebrewFormulaOnly",
    withKept: "brewVersions.scopeEveryWithKept.HomebrewFormulaOnly",
  },
  HomebrewFormula: {
    plain: "brewVersions.scopeEvery.HomebrewFormula",
    withKept: "brewVersions.scopeEveryWithKept.HomebrewFormula",
  },
};

/**
 * The scope sentences that end on a general "its settings and data
 * elsewhere are kept", without it: for an uninstall whose plan names what
 * stays (`warningLines`). The others say something more specific, or
 * nothing of the kind.
 */
const SCOPE_WITH_KEPT_KEYS: Partial<Record<UninstallScope, string>> = {
  HomebrewFormulaOnly: "clarity.scopeWithKept.HomebrewFormulaOnly",
  HomebrewFormula: "clarity.scopeWithKept.HomebrewFormula",
  HomebrewCaskPlain: "clarity.scopeWithKept.HomebrewCaskPlain",
  Npm: "clarity.scopeWithKept.Npm",
  Pipx: "clarity.scopeWithKept.Pipx",
  Uv: "clarity.scopeWithKept.Uv",
};
