import type { InstalledArtifact, ManagerInstance } from "./types";

/**
 * The tools with their own installer whose updates Banager does not check
 * at all: the recipes whose `latest` is `Latest::Unchecked`
 * (crates/banager-core/src/adapters/standalone/recipe.rs): Codex's and
 * opencode's own installs, listed only: their newest versions live on hosts
 * Banager does not connect to, so `check_updates` lists nothing for it --
 * no update, and no "could not check" either. No update listed is then no
 * news, and the Installed page must not call the row 「已是最新」.
 */
export const UNCHECKED_STANDALONE: ReadonlySet<string> = new Set(["standalone-codex", "standalone-opencode"]);

/** Whether Banager checks `instance`'s rows for updates at all. */
export function updatesUnchecked(instance: ManagerInstance): boolean {
  return UNCHECKED_STANDALONE.has(instance.adapter_id);
}

/**
 * What the row says in place of 「已是最新」: that the tool updates itself
 * -- only when the backend found its install following the latest release
 * (`auto_updates`, from the installer's `auto-update-version` naming the
 * release in use) -- or else only that Banager does not check it.
 */
export type UncheckedUpdates = "updatesItself" | "notChecked";

export function uncheckedUpdatesOf(artifact: InstalledArtifact, instance: ManagerInstance): UncheckedUpdates | null {
  if (!updatesUnchecked(instance)) return null;
  return artifact.auto_updates ? "updatesItself" : "notChecked";
}
