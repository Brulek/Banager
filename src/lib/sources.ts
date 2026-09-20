/**
 * What the front end knows about each package source, in one place so the
 * pages and the snapshot gate cannot drift apart. Nothing here talks to the
 * backend; these are presentational facts about adapter ids that arrive over
 * the wire.
 */

/** i18n key holding each adapter's human name. */
export const ADAPTER_LABEL_KEYS: Record<string, string> = {
  brew: "adapters.brew",
  npm: "adapters.npm",
  pipx: "adapters.pipx",
  uv: "adapters.uv",
  pip: "adapters.pip",
  cargo: "adapters.cargo",
  ollama: "adapters.ollama",
};

// pip can only report what is installed; it offers no install/uninstall
// path Canager could safely drive (spec's per-adapter contract table).
// Read-only here is a presentational fact about that one source, not a
// judgement call the UI is making on its own.
export const READ_ONLY_ADAPTER_IDS = new Set(["pip"]);

/**
 * Whether this source's group header on the Installed page renders a
 * `SourceNotice` under it: pip's read-only note, or the "Canager can't reach
 * it" warning that *every* unhealthy instance gets.
 *
 * `!healthy` is not special-cased per adapter. Six adapters can report it --
 * brew, npm, uv, pipx, cargo and ollama -- and it means the same thing for
 * all of them: the CLI is on PATH but Canager could not talk to it. The
 * backend skips an unhealthy instance's fan-out, pushes no error and does
 * not mark the snapshot stale, keeping the instance in `snapshot.instances`
 * only so the UI can say so. While this read "ollama && !healthy", every
 * other unhealthy source's group was dropped instead and nothing anywhere
 * told the user their globally-installed packages had stopped being listed.
 *
 * Exported rather than kept private to `InstalledPage` because
 * `SnapshotStatus` has to ask the same question: its zero-artifact empty
 * state replaces `children` entirely, so without this it would hide the very
 * notice that is the only thing an instance with no artifacts has to say.
 * One rule, one place -- duplicating it would let the two drift.
 */
export function hasSourceNotice(adapterId: string, healthy: boolean): boolean {
  return READ_ONLY_ADAPTER_IDS.has(adapterId) || !healthy;
}
