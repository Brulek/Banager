import type { ArtifactKind, UpdateCandidate } from "./types";

/**
 * A change of major version an update brings -- `3.31.6 → 4.0.0` is
 * `{ from: 3, to: 4 }` -- which the Updates page marks 「大版本」 on the row
 * (research synthesis §4.2 item 3, appendix A3): such an upgrade is the
 * one most likely to change how the tool is used or set up.
 */
export interface MajorJump {
  from: number;
  to: number;
}

/**
 * A major version this large is a date, not a major version:
 * `2026-08-13`, `2026.09.28-64d2043`, `20250101`. Such a tool's every
 * release would read as a new major one.
 */
const DATE_LIKE_FROM = 1900;

/**
 * The major version a version string names (appendix A3): a leading "v" or
 * "V" dropped, and a trailing Homebrew revision ("_2" in `22.23.2_2`); then
 * its leading run of digits, which must end the string or be followed by
 * a separator -- ".", "-", "+", "~", or the "," a Homebrew cask puts
 * before a build number (`5,1234`) -- so that `1a2b…`, a hex digest that
 * happens to start with a digit, or `3rc1` is not read as a version. `null`
 * for anything else: `latest`, `r3222`, `HEAD-1a2b`, an empty string.
 */
export function majorOf(version: string): number | null {
  const bare = version.trim().replace(/^[vV]/, "").replace(/_\d+$/, "");
  const match = /^(\d+)(?:$|[.\-+~,])/.exec(bare);
  if (match === null) return null;
  const major = Number(match[1]);
  return Number.isSafeInteger(major) ? major : null;
}

/**
 * Whether an update from `current` to `target` changes the major version
 * (appendix A3): both parse (`majorOf`), the installed one is 1 or more --
 * a `0.x` tool is never marked, as codex, uv and ruff ship a 0.x minor
 * every week and the mark would be on every row -- neither is a date
 * (`DATE_LIKE_FROM`), and the target's major is larger. Never for an
 * Ollama model (`kind: "Model"`), or any update whose versions are digests
 * (`channel: "Digest"`): a model's "version" is a digest, which can start
 * with digits -- `1a…` → `9f…` -- and is no version at all. Excluded by
 * kind, not by how the string looks.
 */
export function majorJump(
  candidate: Pick<UpdateCandidate, "current" | "target"> & {
    key: { kind: ArtifactKind };
    channel?: UpdateCandidate["channel"];
  },
): MajorJump | null {
  if (candidate.key.kind === "Model" || candidate.channel === "Digest") return null;
  const from = majorOf(candidate.current);
  const to = majorOf(candidate.target);
  if (from === null || to === null) return null;
  if (from < 1 || from >= DATE_LIKE_FROM || to >= DATE_LIKE_FROM) return null;
  return to > from ? { from, to } : null;
}
