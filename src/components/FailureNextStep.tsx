import { useTranslation } from "react-i18next";
import type { FailureCause } from "../lib/failureCause";
import { outcomeCause } from "../lib/failureCause";
import { adapterIdOf, adapterLabel } from "../lib/sources";
import { showsPasswordCommand } from "./PasswordCommand";
import type { OpKind, OpSummary } from "../lib/types";
import type { LogEntry } from "../store/ui";

/**
 * The sentence under a tool's own words, by what is over the log. Spelled
 * out, so the reachability test finds every key.
 *
 * - `generic`: no cause the words give, so nothing over the log says what
 *   to do -- it says how to try again (`TRY_AGAIN_KEYS`), then Copy Log.
 * - `afterStep`: a cause, whose step is over the log (`failure.next.*`);
 *   it does not say that step a second time, only what to do if it fails.
 * - `inTerminal`: sudo wanted a password Banager cannot type, and the
 *   command to run in Terminal is over the log (`PasswordCommand`): no
 *   Retry, which would stop at the same step -- the row has none.
 * - `copyOnly`: the same, from a source with no command to hand over.
 */
export const FAILURE_LOG_STEP_KEYS = {
  generic: "failureSteps.log.generic",
  afterStep: "failureSteps.log.afterStep",
  inTerminal: "failureSteps.log.inTerminal",
  copyOnly: "failureSteps.log.copyOnly",
} as const;

/**
 * How to try once more, by what was tried, for a failure with no known
 * cause: an update has its Retry button on its row; an uninstall or an
 * install is done again.
 */
export const TRY_AGAIN_KEYS: Record<OpKind, string> = {
  Upgrade: "failureSteps.again.Upgrade",
  Uninstall: "failureSteps.again.Uninstall",
  Install: "failureSteps.again.Install",
};

/** Which sentence goes under the log of a failure with `cause`. */
function stepKey(op: OpSummary, cause: FailureCause | null): string {
  if (cause === null) return FAILURE_LOG_STEP_KEYS.generic;
  if (cause === "needsPassword") {
    return showsPasswordCommand(op) ? FAILURE_LOG_STEP_KEYS.inTerminal : FAILURE_LOG_STEP_KEYS.copyOnly;
  }
  return FAILURE_LOG_STEP_KEYS.afterStep;
}

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
  return { key: stepKey(op, cause), cause };
}

/**
 * Under the log of an operation that failed in a tool's own words, a
 * fixed sentence that says whose words they are and what to do next:
 * 「上面是Homebrew自己的报错。可以稍后点按“重试”；还是失败，就点按“拷贝日
 * 志”，发给懂的人看。」 -- where the words give a cause, its step is over
 * the log already, and this one only says what to do if that does not
 * help (`stepKey`). The program named is the source's (`adapterLabel`),
 * which is what wrote the lines -- Homebrew's, not the formula's.
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
