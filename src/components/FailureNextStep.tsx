import { useTranslation } from "react-i18next";
import type { FailureCause } from "../lib/failureCause";
import { outcomeCause } from "../lib/failureCause";
import { adapterIdOf, adapterLabel } from "../lib/sources";
import type { OpKind, OpSummary } from "../lib/types";
import type { LogEntry } from "../store/ui";

/**
 * The next step under a tool's own words, one sentence per cause
 * (`failureCause`), and one for a cause the words do not give. Spelled
 * out, so the reachability test finds every key.
 */
export const FAILURE_LOG_STEP_KEYS: Record<FailureCause | "generic", string> = {
  generic: "failureSteps.log.generic",
  network: "failureSteps.log.network",
  diskFull: "failureSteps.log.diskFull",
  permission: "failureSteps.log.permission",
  busy: "failureSteps.log.busy",
  homebrewUpdating: "failureSteps.log.homebrewUpdating",
  needsPassword: "failureSteps.log.needsPassword",
  passwordNotAccepted: "failureSteps.log.passwordNotAccepted",
};

/**
 * How to try once more, by what was tried: an update has its Retry
 * button on its row; an uninstall or an install is done again.
 */
export const TRY_AGAIN_KEYS: Record<OpKind, string> = {
  Upgrade: "failureSteps.again.Upgrade",
  Uninstall: "failureSteps.again.Uninstall",
  Install: "failureSteps.again.Install",
};

/**
 * The sentence's key and its words, for `op`'s log, or null where it has
 * none: only an operation that ended `Failed` with words of the tool's own
 * (`Failed.summary`) and whose log, as this window has it, holds at least
 * one of the lines it wrote to stderr -- the lines the sentence points up
 * at. Banager's own failures (`BanagerFailed`) are Banager's words, not a
 * tool's, and a tool that said nothing has `FailedSilentDetail` instead.
 */
export function failureLogStep(
  op: OpSummary,
  logs: readonly LogEntry[],
): { key: string; cause: FailureCause | null } | null {
  if (op.status !== "Done") return null;
  const outcome = op.outcome;
  if (outcome === null || typeof outcome === "string" || !("Failed" in outcome)) return null;
  if (outcome.Failed.summary.trim() === "") return null;
  const wroteToStderr = logs.some((line) => line.opId === op.id && "stream" in line && line.stream === "Stderr");
  if (!wroteToStderr) return null;
  const cause = outcomeCause(outcome);
  return { key: FAILURE_LOG_STEP_KEYS[cause ?? "generic"], cause };
}

/**
 * Under the log of an operation that failed in a tool's own words, a
 * fixed sentence that says whose words they are and what to do next:
 * 「上面是Homebrew自己的报错。可以稍后点按“重试”；还是失败，就点按“拷贝日
 * 志”，发给懂的人看。」 -- the try-again part fitted to the cause where the
 * words give one (wait for the network, free some space; a password sudo
 * could not ask for stops at the same step again), and to the kind of
 * operation: Retry for an update, doing it again for the others. The
 * program named is the source's (`adapterLabel`), which is what wrote the
 * lines -- Homebrew's, not the formula's.
 */
export function FailureNextStep({ op, logs, id }: { op: OpSummary; logs: readonly LogEntry[]; id?: string }) {
  const { t } = useTranslation();
  const step = failureLogStep(op, logs);
  if (step === null) return null;
  return (
    <p id={id} data-failure-next-step="" className="mt-3 break-words text-body text-foreground">
      {t(step.key, {
        program: adapterLabel(t, adapterIdOf(op.instance_id)),
        again: t(TRY_AGAIN_KEYS[op.kind]),
      })}
    </p>
  );
}
