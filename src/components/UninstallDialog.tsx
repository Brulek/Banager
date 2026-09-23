import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useSnapshot, usePlanOperation, useSubmitOperation } from "../lib/queries";
import { ADAPTER_LABEL_KEYS, planErrorMessage } from "../lib/sources";
import type { OpRequest } from "../lib/types";
import { warningTexts } from "../lib/warnings";
import { CommandPreview } from "./CommandPreview";
import { Dialog } from "./ui/Dialog";

export interface UninstallDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  request: OpRequest;
  displayName: string;
  onSubmitted?: (opId: number) => void;
}

/**
 * The uninstall confirmation. It plans the operation itself, so the exact
 * command and everything that would break are on screen before anything can
 * be submitted (spec §6), and confirm stays disabled while the plan says
 * something depends on the artifact -- with why, and what to do about it,
 * printed in the dialog body next to the list of what would break, not
 * hidden in a `title` on the disabled button itself. The command preview is
 * unconditional: it is not subject to the "show technical details" setting.
 */
export function UninstallDialog({
  open,
  onOpenChange,
  request,
  displayName,
  onSubmitted,
}: UninstallDialogProps) {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const planMutation = usePlanOperation();
  const submitMutation = useSubmitOperation();
  // For the refusals that can reach a real person verbatim otherwise
  // (`planErrorMessage`'s NotActionable case, from either `plan_operation`
  // or `submit_operation`): the instance's adapter, and therefore its
  // label, does not change out from under a stale snapshot even when its
  // read-only/unavailable state does.
  const instance = snapshot?.instances?.find((i) => i.id === request.instance_id);
  const labelKey = instance ? ADAPTER_LABEL_KEYS[instance.adapter_id] : undefined;
  const sourceLabel = labelKey ? t(labelKey) : (instance?.adapter_id ?? request.instance_id);
  // Monotonic id for "the dialog as it is open right now, for this artifact".
  // Opening, closing or retargeting the dialog retires the previous session,
  // and every callback that runs after an `await` compares the session it was
  // started in against this before writing anything back.
  const sessionRef = useRef(0);

  useEffect(() => {
    sessionRef.current += 1;
    if (open) {
      planMutation.mutate(request);
    } else {
      planMutation.reset();
      submitMutation.reset();
    }
    // planMutation/submitMutation are stable across renders; only re-run
    // when the dialog opens/closes or targets a different artifact.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, request.instance_id, request.artifact_kind, request.name]);

  const issued = planMutation.data;
  const plan = issued?.plan;
  const hasAffected = (plan?.affected.length ?? 0) > 0;
  // Rendered here, once, rather than as text per `<li>`: a warning this
  // build's mirror does not recognise renders nothing (`warningText`
  // returns null for it), and the heading above the list must agree --
  // `plan.warnings.length > 0` alone would show "Before you continue:"
  // over an empty list.
  const planWarnings = warningTexts(t, plan?.warnings ?? []);

  function handleConfirm() {
    if (!issued) return;
    const session = sessionRef.current;
    submitMutation.mutate(issued.id, {
      onSuccess: (opId) => {
        // A reply for a retired session must not close the dialog that
        // replaced it, nor report its op id as the one the user just
        // started. The operation itself is not lost: it is already in
        // `list_operations` and on the event stream.
        if (sessionRef.current !== session) return;
        onSubmitted?.(opId);
        onOpenChange(false);
      },
      onError: () => {
        // A PlanId is single-use and expires after 10 minutes; whatever the
        // backend said, this one is spent. Re-plan so the dialog shows a
        // fresh id and preview instead of letting the user resubmit a dead
        // one. The error itself stays visible (rendered below) until the
        // dialog closes, because `submitMutation` is only reset on close.
        // Guarded for the same reason as `onSuccess`: a retired session's
        // re-plan would overwrite the current session's preview with a plan
        // for the wrong artifact.
        if (sessionRef.current !== session) return;
        planMutation.mutate(request);
      },
    });
  }

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("uninstall.title", { name: displayName })}
      footer={
        <>
          <button
            type="button"
            onClick={() => onOpenChange(false)}
            className="rounded-md px-3 py-1 text-sm"
          >
            {t("uninstall.cancel")}
          </button>
          <button
            type="button"
            onClick={handleConfirm}
            disabled={!plan || hasAffected || submitMutation.isPending}
            className="rounded-md bg-[var(--color-danger)] px-3 py-1 text-sm font-medium text-[var(--color-accent-foreground)] disabled:opacity-50"
          >
            {t("uninstall.confirm")}
          </button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <p className="text-sm text-[var(--color-foreground)]">{t("uninstall.description")}</p>

        {planMutation.isPending && (
          <p className="text-sm text-[var(--color-muted)]">{t("uninstall.checking")}</p>
        )}

        {planMutation.isError && (
          <p role="alert" className="text-sm text-[var(--color-danger)]">
            {t("uninstall.planError", {
              message: planErrorMessage(t, planMutation.error.message, sourceLabel),
            })}
          </p>
        )}

        {submitMutation.isError && (
          <p role="alert" className="text-sm text-[var(--color-danger)]">
            {t("uninstall.submitError", {
              message: planErrorMessage(t, submitMutation.error.message, sourceLabel),
            })}
          </p>
        )}

        {plan && (
          <div className="flex flex-col gap-3">
            {planWarnings.length > 0 && (
              <div>
                <p className="text-sm font-medium text-[var(--color-foreground)]">
                  {t("uninstall.warningsTitle")}
                </p>
                <ul className="list-disc pl-5 text-sm text-[var(--color-foreground)]">
                  {planWarnings.map((warning) => (
                    <li key={warning}>{warning}</li>
                  ))}
                </ul>
              </div>
            )}

            {hasAffected && (
              <div>
                <p className="text-sm font-medium text-[var(--color-foreground)]">
                  {t("uninstall.affectedTitle")}
                </p>
                <ul className="list-disc pl-5 text-sm text-[var(--color-foreground)]">
                  {plan.affected.map((name) => (
                    <li key={name}>{name}</li>
                  ))}
                </ul>
                {/* Why Confirm below is disabled, said in the dialog body rather
                    than only in a `title` on that disabled button: a disabled
                    button takes no pointer events and drops out of the tab
                    order, so neither a mouse hover nor a keyboard/VoiceOver
                    user ever reached that tooltip. This paragraph is plain
                    text in the flow, reachable by everyone who reached the
                    list above it. */}
                <p className="text-sm text-[var(--color-danger)]">
                  {t("uninstall.affectedBlocksConfirm", { name: displayName })}
                </p>
              </div>
            )}

            <CommandPreview program={plan.program} args={plan.args} />

            {plan.needs_password && (
              // Every Cask uninstall sets `needs_password`, so removing a GUI
              // app pops a system password dialog. Spec §6 requires that to be
              // marked in the preview: a password is never a surprise.
              <p className="text-sm font-medium text-[var(--color-foreground)]">
                {t("commandPreview.needsPassword")}
              </p>
            )}
          </div>
        )}
      </div>
    </Dialog>
  );
}
