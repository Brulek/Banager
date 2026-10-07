import { useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ArtifactKey, IssuedPlan, PlanAction } from "../lib/types";
import { usePlanOperation, useSettings } from "../lib/queries";
import { FAILURE_CAUSE_KEYS } from "../lib/failureCause";
import { OP_KIND_KEYS } from "../lib/operations";
import { adapterIdOf, adapterLabel, planErrorMessage } from "../lib/sources";
import { warningLines } from "../lib/warnings";
import { PasswordInstructions } from "./PasswordCommand";
import { InfoDetail, TextWithInfo } from "./InfoDetail";
import { OutcomeIcon } from "./OutcomeIcon";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";

// The same primary command the operation's log hands over. Follow-ups
// remain outside this handoff, as they are in PasswordCommand.
function primaryCommand(action: PlanAction) {
  if ("Command" in action) return action.Command;
  if ("CommandThen" in action) return action.CommandThen;
  return null;
}

/**
 * View Steps for a Homebrew update the history kept as stopped where sudo
 * wanted the Mac's password (`usePasswordRecoveryKeys`), after a restart:
 * the history keeps no command, so each opening asks the existing planner
 * (`plan_operation`) for this update anew, and shows the Terminal steps
 * the log of that stop showed (`PasswordInstructions`) only for a preview
 * of this same tool by Homebrew -- nothing rebuilt from a name or an old
 * record, and nothing submitted. Titled and worded as that log was: the
 * tool, 「更新 · 需要输入密码」 and the cause's step. A refused preview
 * says to check again, its reason behind the ⓘ in the window's words
 * (`planErrorMessage`), and shows no command. The button is a row's
 * (`regular`), or `small` on a 「最近的更新记录」 line, as that list's View
 * Log is (r21 C7).
 */
export function PasswordRecovery({
  artifactKey,
  name,
  size = "regular",
}: {
  artifactKey: ArtifactKey;
  name: string;
  size?: "small" | "regular";
}) {
  const { t } = useTranslation();
  const planner = usePlanOperation();
  const { data: settings } = useSettings();
  const technical = settings?.show_technical_details ?? false;
  const [open, setOpen] = useState(false);
  const [issued, setIssued] = useState<IssuedPlan | null>(null);
  // The backend's refusal as it sent it, "" for a preview of something
  // else, or null.
  const [error, setError] = useState<string | null>(null);
  const requestNumber = useRef(0);
  const stepId = useId();
  const close = () => {
    requestNumber.current++;
    setOpen(false);
    setIssued(null);
  };
  const show = async () => {
    const number = ++requestNumber.current;
    setIssued(null);
    setError(null);
    setOpen(true);
    try {
      const fresh = await planner.mutateAsync({
        kind: "Upgrade",
        instance_id: artifactKey.instance_id,
        artifact_kind: artifactKey.kind,
        name: artifactKey.name,
      });
      if (number !== requestNumber.current) return;
      const request = fresh.plan.request;
      const action = primaryCommand(fresh.plan.action);
      // No fallback reconstructed from a tool name or old history.
      if (
        request.kind !== "Upgrade" ||
        request.instance_id !== artifactKey.instance_id ||
        request.artifact_kind !== artifactKey.kind ||
        request.name !== artifactKey.name ||
        adapterIdOf(artifactKey.instance_id) !== "brew" ||
        action?.program.split("/").pop() !== "brew"
      ) {
        setError("");
      } else {
        setIssued(fresh);
      }
    } catch (failure) {
      if (number === requestNumber.current) {
        setError(failure instanceof Error ? failure.message : String(failure));
      }
    }
  };
  const action = issued === null ? null : primaryCommand(issued.plan.action);
  const warnings = issued === null ? [] : Object.values(warningLines(t, issued.plan.warnings)).flat();
  // Why the preview was refused, in the window's words; none for a
  // refusal it has no words for.
  const why =
    error === null || error === ""
      ? null
      : planErrorMessage(t, error, adapterLabel(t, adapterIdOf(artifactKey.instance_id)), technical);
  const unavailable = t("passwordRecovery.unavailable");
  return (
    <>
      <button
        type="button"
        className={size === "small" ? BUTTON.small.grey : BUTTON.regular.grey}
        onClick={() => void show()}
        aria-label={t("needsPassword.viewStepsLabel", { name })}
      >
        {t("needsPassword.viewSteps")}
      </button>
      <Dialog
        open={open}
        onOpenChange={(value) => {
          if (!value) close();
        }}
        title={name}
        subtitle={
          <span className="inline-flex items-start gap-1">
            <OutcomeIcon tone="failure" size={12} className="mt-px" />
            <span className="min-w-0 break-words">
              {t("operations.kindStatus", {
                kind: t(OP_KIND_KEYS.Upgrade),
                status: t(FAILURE_CAUSE_KEYS.needsPassword.word),
              })}
            </span>
          </span>
        }
        describedBy={stepId}
        width="log"
        focusSelf
        footer={
          <button type="button" className={BUTTON.large.default} onClick={close}>
            {t("common.done")}
          </button>
        }
      >
        <p id={stepId} className="mb-3 break-words text-body text-foreground">
          {t(FAILURE_CAUSE_KEYS.needsPassword.next)}
        </p>
        {error !== null ? (
          <p role="alert" className="mb-3 break-words text-body text-foreground">
            {why === null ? (
              unavailable
            ) : (
              <TextWithInfo text={unavailable} label={t("common.detailsLabel", { title: name })}>
                {why}
              </TextWithInfo>
            )}
          </p>
        ) : action ? (
          <>
            {warnings.map((line, index) => (
              <p key={index} className="mb-2 break-words text-body text-foreground">
                {line.text}
                {line.detail ? (
                  <InfoDetail label={t("common.detailsLabel", { title: line.text })}>{line.detail}</InfoDetail>
                ) : null}
              </p>
            ))}
            <PasswordInstructions preview={{ argv_preview: [action.program, ...action.args], env_preview: action.env }} />
          </>
        ) : (
          <p role="status" className="text-body text-muted">
            {t("passwordRecovery.preparing")}
          </p>
        )}
      </Dialog>
    </>
  );
}
