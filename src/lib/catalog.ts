/**
 * The Discover page's catalog as the window words it (phase 5): each
 * entry's words and logo, what its row says of it, and where an install
 * of it from here stands. The catalog itself is Rust's
 * (crates/banager-core/src/catalog.rs), sent with `get_catalog`.
 */
import type { ArtifactKey, CatalogEntry, CatalogState, ManagerInstance, OpSummary } from "./types";

/** Whatever `useTranslation()`'s `t` needs here; the same convention as `Translate` in src/lib/sources.ts. */
type Translate = (key: string, options?: Record<string, string | number>) => string;

/** One entry's words, and for one installed from here what to type to use it. */
export interface ItemKeys {
  name: string;
  /** What it is: the row's line. */
  what: string;
  /** Who needs it and how it is used once installed: behind the line's ⓘ, and in the install sheet. */
  more: string;
  /** How it is used, said on its own: the row's line once installed from here. */
  use?: string;
  /** What to type in Terminal to use it, for Copy beside `use`: a word the user types, never one Banager runs. */
  command?: string;
}

/**
 * Each entry's words, by its id (`CatalogItem::id` in
 * crates/banager-core/src/catalog.rs). Literal keys, so
 * `completeness.test.ts` finds each one; a catalog id with none here shows
 * its id and no line.
 */
export const ITEM_KEYS: Record<string, ItemKeys> = {
  homebrew: {
    name: "discover.items.homebrew.name",
    what: "discover.items.homebrew.what",
    more: "discover.items.homebrew.more",
  },
  "claude-code": {
    name: "discover.items.claude-code.name",
    what: "discover.items.claude-code.what",
    more: "discover.items.claude-code.more",
    use: "discover.items.claude-code.use",
    command: "claude",
  },
  codex: {
    name: "discover.items.codex.name",
    what: "discover.items.codex.what",
    more: "discover.items.codex.more",
    use: "discover.items.codex.use",
    command: "codex",
  },
  "gemini-cli": {
    name: "discover.items.gemini-cli.name",
    what: "discover.items.gemini-cli.what",
    more: "discover.items.gemini-cli.more",
    use: "discover.items.gemini-cli.use",
    command: "gemini",
  },
  ollama: {
    name: "discover.items.ollama.name",
    what: "discover.items.ollama.what",
    more: "discover.items.ollama.more",
  },
  node: {
    name: "discover.items.node.name",
    what: "discover.items.node.what",
    more: "discover.items.node.more",
    use: "discover.items.node.use",
    command: "node",
  },
  uv: {
    name: "discover.items.uv.name",
    what: "discover.items.uv.what",
    more: "discover.items.uv.more",
    use: "discover.items.uv.use",
    command: "uv",
  },
  ffmpeg: {
    name: "discover.items.ffmpeg.name",
    what: "discover.items.ffmpeg.what",
    more: "discover.items.ffmpeg.more",
    use: "discover.items.ffmpeg.use",
    command: "ffmpeg",
  },
};

/** An entry's name as the page says it: its id where the window has no words for it. */
export function entryName(t: Translate, id: string): string {
  const keys = ITEM_KEYS[id];
  return keys === undefined ? id : t(keys.name);
}

/**
 * What the row says on its right of `state`. A `switch` with no default,
 * so a state added to `CatalogState` without words here fails `tsc`.
 */
export function stateText(t: Translate, state: CatalogState): string {
  if (typeof state === "object") return t("discover.state.needsMacOS", { major: state.NeedsMacOS.major });
  switch (state) {
    case "Installed":
      return t("discover.state.installed");
    case "NotInstalled":
      return t("discover.state.notInstalled");
    case "NeedsHomebrew":
      return t("discover.state.needsHomebrew");
    case "HomebrewUnsupported":
      return t("discover.state.homebrewUnsupported");
  }
}

/**
 * Why `plan_install` refused an entry this Mac cannot install now
 * (`{"kind":"catalog_state"}`, `plan_install_impl` in
 * src-tauri/src/ipc.rs), as the install sheet says it; null for any other
 * refusal, which `planErrorMessage` words.
 */
export function catalogRefusal(t: Translate, raw: string): string | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const { kind, state } = parsed as { kind?: unknown; state?: unknown };
  if (kind !== "catalog_state") return null;
  if (typeof state === "object" && state !== null && "NeedsMacOS" in state) {
    const major = (state as { NeedsMacOS: { major?: unknown } }).NeedsMacOS.major;
    return typeof major === "number" ? t("discover.sheet.needsMacOS", { major }) : null;
  }
  switch (state) {
    case "Installed":
      return t("discover.sheet.installed");
    case "NeedsHomebrew":
    case "HomebrewUnsupported":
      return t("discover.sheet.needsHomebrew");
    default:
      return null;
  }
}

/**
 * What finds an entry's logo: a Homebrew formula or cask by its name, as
 * the Installed page finds that package's, under this Mac's first
 * Homebrew -- whose row, where the cask is installed there, also has its
 * app's icon. Homebrew itself has none: its row draws Homebrew's mark.
 */
export function iconKeyOf(entry: CatalogEntry, brew: ManagerInstance | undefined): ArtifactKey | undefined {
  if (entry.install === "HomebrewItself") return undefined;
  const { kind, name } = entry.install.Homebrew;
  return { instance_id: brew?.id ?? "brew:/opt/homebrew", kind, name };
}

/**
 * The newest install of `entry` from here -- under way, or finished --
 * among `operations` and this window's own submissions not yet listed
 * (`usePendingOperations`), or null. An install is matched by what it
 * installs, the entry's own Homebrew formula or cask by name under any
 * Homebrew: `plan_install` picks the Homebrew, the window does not.
 */
export function installOperationOf(
  entry: CatalogEntry,
  operations: OpSummary[],
  pending: Record<string, OpSummary>,
): OpSummary | null {
  if (entry.install === "HomebrewItself") return null;
  const { kind, name } = entry.install.Homebrew;
  const installs = (op: OpSummary) =>
    op.kind === "Install" && op.artifact_kind === kind && op.name === name && op.instance_id.startsWith("brew:");
  // A reservation not yet answered has no id of its own (-1): it is the
  // newest there is.
  const reserved = Object.values(pending).find(installs);
  if (reserved !== undefined && !operations.some((op) => op.id === reserved.id)) return reserved;
  return operations.filter(installs).reduce<OpSummary | null>((newest, op) => (newest === null || op.id > newest.id ? op : newest), null);
}

/**
 * Where an install from Discover stands, for its row: waiting, under way,
 * being cancelled; done and there; or ended without it -- failed, stopped,
 * or a result Banager found otherwise than the tool said (`check`) or
 * could not tell (`unconfirmed`), each opening its log. A `switch` with no
 * default, so a new status fails `tsc`.
 */
export type InstallProgress =
  | { kind: "queued" }
  | { kind: "running" }
  | { kind: "cancelling" }
  | { kind: "installed" }
  | { kind: "cancelled" }
  | { kind: "failed"; opId: number }
  | { kind: "check"; opId: number }
  | { kind: "unconfirmed"; opId: number };

export function installProgressOf(op: OpSummary): InstallProgress {
  switch (op.status) {
    case "Queued":
      return { kind: "queued" };
    // Verifying is the install's own last step: Homebrew has ended and
    // Banager is reading back whether it is there.
    case "Running":
    case "Verifying":
      return { kind: "running" };
    case "CancelRequested":
    case "Cancelling":
      return { kind: "cancelling" };
    case "Done": {
      const outcome = op.outcome;
      if (outcome === "Succeeded") return { kind: "installed" };
      if (outcome === "Cancelled") return { kind: "cancelled" };
      if (outcome === "Unconfirmed" || outcome === null) return { kind: "unconfirmed", opId: op.id };
      if (typeof outcome === "object" && "NeedsAttention" in outcome) return { kind: "check", opId: op.id };
      return { kind: "failed", opId: op.id };
    }
  }
}

/** Whether an install's row still waits for it: not yet ended. */
export function isUnderway(progress: InstallProgress): boolean {
  return progress.kind === "queued" || progress.kind === "running" || progress.kind === "cancelling";
}
