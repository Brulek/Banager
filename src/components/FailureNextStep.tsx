import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import type { FailureCause } from "../lib/failureCause";
import { FAILURE_CAUSE_KEYS, outcomeCause } from "../lib/failureCause";
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
  Link: "noAnswer.op.again",
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

/**
 * The same sentence where the tool's own words are shown outside the log,
 * with "Show technical details" on: under a row of the batch uninstall's
 * result block (`resultRowStep`), and under the log's subtitle when the
 * log itself no longer holds those words (`subtitleStep`). Spelled out,
 * so the reachability test finds every key.
 *
 * - `generic`: no cause the words give -- how to try again, then View Log
 *   and Copy Log there, the row having no Copy Log of its own.
 * - `withCause`: a cause, whose line (`failure.line.*`) is what the row
 *   says with technical details off -- the words take its place, so the
 *   sentence says it.
 * - `inTerminal`: a cause the Terminal command answers (`PasswordCommand`,
 *   which is in the log): the cause's line, then where the command is.
 * - `noLog`: the log's subtitle, where this window's log has none of the
 *   tool's lines any more (it keeps the newest 2,000): how to try again,
 *   then Copy Error Details, under it (`SubtitleWordsCopy`) -- not Copy
 *   Log, which would copy nothing of them.
 */
export const TOOL_WORDS_STEP_KEYS = {
  generic: "failure.toolWords.generic",
  withCause: "failure.toolWords.withCause",
  inTerminal: "failure.toolWords.inTerminal",
  noLog: "failure.toolWords.noLog",
} as const;

/**
 * Whether `op` ended `Failed` in words a program it ran wrote: the summary
 * is not empty, and a command ran (`exit_code` is set). A path-list
 * uninstall that macOS would not move to the Trash also ends `Failed`, in
 * macOS's words, with no exit code -- not the source's words, so not said
 * to be. Nor, with no exit code, the read npm and uv take before a
 * command that did not finish: what it wrote before it was stopped is no
 * verdict of the source's; it stays in the log.
 */
function endedInToolWords(op: OpSummary): boolean {
  if (op.status !== "Done") return false;
  const outcome = op.outcome;
  if (outcome === null || typeof outcome === "string" || !("Failed" in outcome)) return false;
  return outcome.Failed.exit_code !== null && outcome.Failed.summary.trim() !== "";
}

/**
 * The sentence under a row of the batch uninstall's result block, which
 * shows the tool's own words only with technical details on: null with
 * them off, and for an operation that did not end in a tool's words.
 */
export function resultRowStep(op: OpSummary, technical: boolean): { key: string; cause: FailureCause | null } | null {
  if (!technical || !endedInToolWords(op)) return null;
  const cause = outcomeCause(op.outcome);
  if (cause === null) return { key: TOOL_WORDS_STEP_KEYS.generic, cause };
  if (showsPasswordCommand(op)) return { key: TOOL_WORDS_STEP_KEYS.inTerminal, cause };
  return { key: TOOL_WORDS_STEP_KEYS.withCause, cause };
}

/**
 * The sentence under the log's subtitle, which shows the tool's own words
 * only with technical details on: only where the sentence under the log
 * is missing (`failureLogStep` is null: none of the tool's lines are left
 * in this window's log) and no cause's step is over the log already --
 * where there is one, it says what to do.
 */
export function subtitleStep(
  op: OpSummary,
  logs: readonly LogEntry[],
  technical: boolean,
): { key: string } | null {
  if (!technical || !endedInToolWords(op)) return null;
  if (failureLogStep(op, logs) !== null) return null;
  if (outcomeCause(op.outcome) !== null) return null;
  return { key: TOOL_WORDS_STEP_KEYS.noLog };
}

/** The words for a `TOOL_WORDS_STEP_KEYS` sentence about `op`. */
function toolWordsText(
  t: TFunction,
  op: OpSummary,
  key: string,
  cause: FailureCause | null,
): string {
  return t(key, {
    program: adapterLabel(t, adapterIdOf(op.instance_id)),
    again: t(TRY_AGAIN_KEYS[op.kind]),
    line: cause === null ? "" : t(FAILURE_CAUSE_KEYS[cause].line),
  });
}

/**
 * Under a row of the batch uninstall's result block that shows a tool's
 * own words (technical details on): whose words they are and what to do
 * next (`resultRowStep`).
 */
export function ResultRowStep({ op, technical, className }: { op: OpSummary; technical: boolean; className?: string }) {
  const { t } = useTranslation();
  const step = resultRowStep(op, technical);
  if (step === null) return null;
  return (
    <p data-failure-next-step="" className={className}>
      {toolWordsText(t, op, step.key, step.cause)}
    </p>
  );
}

/**
 * Over the log, under its subtitle, where that subtitle is the only place
 * left with the tool's own words (`subtitleStep`).
 */
export function SubtitleStep({
  op,
  logs,
  technical,
  id,
}: {
  op: OpSummary;
  logs: readonly LogEntry[];
  technical: boolean;
  id?: string;
}) {
  const { t } = useTranslation();
  const step = subtitleStep(op, logs, technical);
  if (step === null) return null;
  return (
    <p id={id} data-failure-next-step="" className="mb-3 break-words text-body text-foreground">
      {toolWordsText(t, op, step.key, null)}
    </p>
  );
}
