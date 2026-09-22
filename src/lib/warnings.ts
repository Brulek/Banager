/**
 * How `Plan.warnings` and `UpdateCandidate.warnings` become text in the
 * user's language. Same split as `outcomeKey`/`outcomeArgs` in
 * `src/lib/format.ts`: no `t()` and no JSX here, so both the uninstall
 * dialog and the updates page share one rule and it is testable without
 * rendering anything.
 */
import type { Warning } from "./types";

/**
 * The `warnings.*` key for a `Warning`'s copy, or `null` when there is no
 * key to look up: either this is the `Message` catch-all (its text is
 * read straight off the wire, see `warningMessage` below), or it is a
 * variant this build's hand-written mirror does not recognise yet -- a
 * newer Rust build sent something `types.ts` has no case for (spec §3's
 * note on that union not failing to compile when it drifts). Callers treat
 * both the same way: nothing to render.
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
      default:
        return null;
    }
  }
  if ("WouldBreak" in warning) return "warnings.wouldBreak";
  if ("ThirdPartyRegistry" in warning) return "warnings.thirdPartyRegistry";
  return null;
}

/** Interpolation values for `t(warningKey(warning), warningArgs(warning))`. */
export function warningArgs(warning: Warning): Record<string, unknown> {
  if (typeof warning !== "string" && "WouldBreak" in warning) {
    const names = warning.WouldBreak.names;
    return { count: names.length, names: names.join(", ") };
  }
  if (typeof warning !== "string" && "ThirdPartyRegistry" in warning) {
    return { host: warning.ThirdPartyRegistry.host };
  }
  return {};
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
 * for a fixed warning, `warningMessage(warning)` for a `Message`, `null`
 * for a variant this build does not recognise. The convenience wrapper
 * every call site actually wants; `warningKey`/`warningArgs`/
 * `warningMessage` stay exported and `t()`-free for testing.
 */
export function warningText(t: Translate, warning: Warning): string | null {
  const key = warningKey(warning);
  return key ? t(key, warningArgs(warning)) : warningMessage(warning);
}

/** `warnings`, rendered in order, with unrecognised variants dropped. */
export function warningTexts(t: Translate, warnings: Warning[]): string[] {
  return warnings
    .map((warning) => warningText(t, warning))
    .filter((text): text is string => text !== null);
}
