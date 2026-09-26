/**
 * How `Plan.warnings` and `UpdateCandidate.warnings` become text in the
 * user's language. Same split as `outcomeKey`/`outcomeArgs` in
 * `src/lib/format.ts`: no `t()` and no JSX here, so both the uninstall
 * dialog and the updates page share one rule and it is testable without
 * rendering anything.
 */
import type { KeptWhat, RemovedWhat, Warning } from "./types";

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
 * The return type keeps `null` for `warningTexts`'s filter; with
 * `warningKey` exhaustive, a fixed warning always has a key and a
 * `Message` always has its text, so it is never actually `null`.
 */
export function warningText(t: Translate, warning: Warning): string | null {
  const key = warningKey(warning);
  return key ? t(key, warningArgs(warning)) : warningMessage(warning);
}

/** `warnings`, rendered in order. */
export function warningTexts(t: Translate, warnings: Warning[]): string[] {
  return warnings
    .map((warning) => warningText(t, warning))
    .filter((text): text is string => text !== null);
}
