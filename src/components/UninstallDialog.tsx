import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSnapshot, usePlanOperation, useSubmitOperation } from "../lib/queries";
import {
  ADAPTER_LABEL_KEYS,
  parseUninstallBlocked,
  planErrorMessage,
  UNINSTALL_BLOCKED_KEYS,
} from "../lib/sources";
import type { OpRequest } from "../lib/types";
import { warningTexts } from "../lib/warnings";
import { CommandPreview } from "./CommandPreview";
import { COMMAND_SLOT, withCommand } from "./withCommand";
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
  // Set synchronously before `submitMutation.mutate()` and read by
  // `handleConfirm` before it: `submitMutation.isPending`, which disables
  // the button, reaches React only through TanStack's setTimeout(0)
  // notify, so a second click in the same event-loop turn still finds an
  // enabled button. Cleared when that submit settles, and in the effect
  // below on every open, close and retarget, so a submit that was cut off
  // by closing the dialog (whose callbacks then never fire, because
  // `reset()` detaches the observer) cannot leave it stuck.
  const submitLatch = useRef(false);
  // A fresh preview was issued because the previous confirm did not start
  // anything. Rendered as a note beside that preview; retired when the user
  // confirms again, and in the effect below.
  const [reissued, setReissued] = useState(false);

  useEffect(() => {
    sessionRef.current += 1;
    submitLatch.current = false;
    setReissued(false);
    if (open) {
      planMutation.mutate(request);
    } else {
      planMutation.reset();
      submitMutation.reset();
    }
    // planMutation/submitMutation are stable across renders; only re-run
    // when the dialog opens/closes or targets a different artifact.
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

  // The one refusal this dialog words itself rather than through
  // `planErrorMessage`: the tool will not uninstall this package (a pinned
  // Homebrew formula or cask, `uninstall_blocked` in
  // crates/canager-core/src/session/plans.rs), which only a stale Installed
  // page can reach. Its sentence is not "couldn't check what this would
  // affect" -- Canager did check -- and it carries the unpin command, set
  // apart as code as on the Installed page's row.
  function refusalText(raw: string, frame: "uninstall.planError" | "uninstall.submitError") {
    const blocked = parseUninstallBlocked(raw);
    if (blocked === null) {
      return t(frame, { message: planErrorMessage(t, raw, sourceLabel) });
    }
    const copy = UNINSTALL_BLOCKED_KEYS[blocked];
    return withCommand(
      t(copy.refused, { command: COMMAND_SLOT, source: sourceLabel }),
      copy.command(
        { instance_id: request.instance_id, kind: request.artifact_kind, name: request.name },
        instance,
      ),
    );
  }

  function handleConfirm() {
    if (!issued || submitLatch.current) return;
    submitLatch.current = true;
    setReissued(false);
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
        // one. The error itself (rendered below) stays up while the re-plan
        // runs, since it is what says why the dialog is checking again, and
        // is reset the moment the re-plan settles: beside a fresh preview
        // and an enabled Confirm it would only read as "still broken", and
        // beside a failed re-plan's own error it would be a second red
        // paragraph about a state that has passed. When the re-plan brings
        // a preview, `reissued` puts a note beside it instead.
        // Guarded for the same reason as `onSuccess`: a retired session's
        // re-plan would overwrite the current session's preview with a plan
        // for the wrong artifact.
        if (sessionRef.current !== session) return;
        planMutation.mutate(request, {
          onSettled: (fresh) => {
            if (sessionRef.current !== session) return;
            submitMutation.reset();
            if (fresh !== undefined) setReissued(true);
          },
        });
      },
      onSettled: () => {
        submitLatch.current = false;
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
            {refusalText(planMutation.error.message, "uninstall.planError")}
          </p>
        )}

        {submitMutation.isError && (
          <p role="alert" className="text-sm text-[var(--color-danger)]">
            {refusalText(submitMutation.error.message, "uninstall.submitError")}
          </p>
        )}

        {reissued && plan && (
          // `status`, not `alert`: nothing is wrong with the fresh preview,
          // the user only needs to know the last confirm did not start it.
          <p role="status" className="text-sm text-[var(--color-muted)]">
            {/* No "confirm once more" when the fresh preview lists affected
                packages: that disables Uninstall below, and the body says why. */}
            {hasAffected ? t("uninstall.reissued") : t("uninstall.reissuedConfirmAgain")}
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
