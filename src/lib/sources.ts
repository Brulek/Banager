/**
 * What the front end knows about each package source, in one place so the
 * pages and the snapshot gate cannot drift apart. Nothing here talks to the
 * backend; these are presentational facts about what arrives over the wire.
 */
import type { ManagerInstance, ReadOnlyReason } from "./types";

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

/**
 * The `sourceNotice.*` key prefix whose `.title` and `.description` explain
 * each read-only reason.
 *
 * The two reasons need genuinely different copy and the difference matters:
 * pip cannot be driven at all, so the way out is to install Python tools
 * with pipx or uv; an npm whose prefix is root-owned works fine, so the way
 * out is to reinstall Node with Homebrew. Telling the npm user about pipx
 * -- which the Updates page did for every read-only row before the wire
 * carried a reason -- sends someone who does not write code to install a
 * Python tool to fix their JavaScript packages.
 */
export const READ_ONLY_NOTICE_KEYS: Record<ReadOnlyReason, string> = {
  ByDesign: "sourceNotice.pipReadOnly",
  PrefixNotWritable: "sourceNotice.prefixNotWritable",
};

/**
 * Whether Canager may offer operations on this source. The front-end
 * mirror of `ManagerInstance::writable()`; `Session::issue_plan` refuses
 * anything this would have hidden, so a stale snapshot can only ever cost
 * an error message, never an unintended command.
 *
 * `read_only_reason` replaced a hardcoded `READ_ONLY_ADAPTER_IDS = {"pip"}`
 * here. That list could only ever be right about pip, whose read-only-ness
 * is a property of the tool; npm's depends on where Node was installed on
 * *this* machine, which no list of adapter ids can know.
 */
export function canWrite(instance: ManagerInstance): boolean {
  return instance.read_only_reason === null;
}

/**
 * Whether this source's group header on the Installed page renders a
 * `SourceNotice`: a read-only source's explanation, or the "Canager can't
 * reach it" warning that *every* unhealthy instance gets.
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
export function hasSourceNotice(instance: ManagerInstance): boolean {
  return !canWrite(instance) || !instance.healthy;
}
