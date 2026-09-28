import { useEffect, useRef, useState, type RefObject } from "react";
import { useTranslation } from "react-i18next";
import { useSnapshot, usePlanOperation, useSubmitOperation } from "../lib/queries";
import {
  adapterIdOf,
  adapterLabel,
  parseUninstallBlocked,
  parseUninstallUnsafe,
  planErrorDetail,
  planErrorMessage,
  uninstallBlockedCopy,
} from "../lib/sources";
import type { OpRequest } from "../lib/types";
import { deletesForGood, warningLines, type WarningLine } from "../lib/warnings";
import { CommandPreview } from "./CommandPreview";
import { SheetLines, SheetPending, Refusal, SheetSection, SheetTool } from "./SheetParts";
import { WarningIcon } from "./icons";
import { COMMAND_SLOT, withCommand } from "./withCommand";
import { Dialog, SHEET_BUTTON } from "./ui/Dialog";

export interface UninstallDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  request: OpRequest;
  displayName: string;
  onSubmitted?: (opId: number) => void;
  /** What opened it -- a row's Uninstall, a drawer's -- which gets the focus back when it closes. */
  returnFocusTo?: RefObject<HTMLElement | null>;
  /** Called once it has closed and handed the focus back. */
  onClosed?: () => void;
}

/**
 * The uninstall confirmation, as a sheet: 「卸载 Claude Code？」, the tool
 * with its avatar and, under it, the one sentence its source's uninstall
 * has about what goes and what stays (`Warning.UninstallScope`), what the
 * uninstall does in three groups, then Cancel and a red Uninstall --
 * 「永久卸载」 where a line says the uninstall deletes something for good
 * (`deletesForGood`): rustup's own, and a cask whose recorded uninstall
 * deletes paths.
 *
 * It plans the operation itself, so everything that would change is on
 * screen before anything can be submitted (spec §6), in the copy table's
 * three groups (C4): 「移到废纸篓」, what a path-list uninstall moves --
 * with what it found already gone -- and the one sentence it has in place
 * of a command; 「保留不动」, what it leaves where it is; and 「请注意」,
 * everything else: what still needs the package, rustup deleting folders
 * for good, a dependency check that did not finish, that it cannot be
 * stopped once it starts, a password. A line's longer why is behind its
 * ⓘ. The command itself is one click away (`CommandPreview`), open from
 * the start with technical details on.
 *
 * Uninstall stays disabled while the plan says something still needs the
 * package -- with why, and what to do about it, in the sheet's body next
 * to the list of what needs it, not in a `title` on the disabled button.
 */
export function UninstallDialog({
  open,
  onOpenChange,
  request,
  displayName,
  onSubmitted,
  returnFocusTo,
  onClosed,
}: UninstallDialogProps) {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const planMutation = usePlanOperation();
  const submitMutation = useSubmitOperation();
  const cancelRef = useRef<HTMLButtonElement>(null);
  // For the refusals that can reach a real person verbatim otherwise
  // (`planErrorMessage`'s NotActionable case, from either `plan_operation`
  // or `submit_operation`): the instance's adapter, and therefore its
  // label, does not change out from under a stale snapshot even when its
  // read-only/unavailable state does. The avatar goes by the same.
  const instance = snapshot?.instances?.find((i) => i.id === request.instance_id);
  // A snapshot that lost the instance still names the source, by the id.
  const adapterId = instance?.adapter_id ?? adapterIdOf(request.instance_id);
  const sourceLabel = adapterLabel(t, adapterId);
  // The version the tool's row shows: never a model's digest.
  const artifact = snapshot?.artifacts?.find(
    (a) =>
      a.key.instance_id === request.instance_id &&
      a.key.kind === request.artifact_kind &&
      a.key.name === request.name,
  );
  const version =
    artifact === undefined || artifact.key.kind === "Model" || artifact.version === "" ? null : artifact.version;
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
  const affected = plan?.affected ?? [];
  const hasAffected = affected.length > 0;
  // `warningLines` is the one rule for turning `plan.warnings` into lines
  // and groups; with the plan's `affected` list shown once below, a
  // `WouldBreak` naming the same packages is not said a second time, and
  // the scope sentence names the tool as the title does.
  const lines = warningLines(t, plan?.warnings ?? [], affected, displayName);
  const trashPlan = plan !== undefined && "TrashPaths" in plan.action;
  // Said on the button too, where a line says it: what goes is deleted
  // for good, not moved to the Trash.
  const permanent = plan?.warnings.some(deletesForGood) ?? false;
  // What to know before going on, after the lines the plan carries: that
  // it cannot be stopped once it starts -- the one policy the operation
  // bar offers no Cancel for once the command is Running
  // (`OperationManager::cancel`, crates/canager-core/src/ops/mod.rs):
  // rustup's own uninstall, which removes Rust directory by directory --
  // and that it may ask for the Mac's password. Every Cask uninstall sets
  // `needs_password`, though not every app then asks (the copy table's
  // T3). Spec §6: a password is never a surprise.
  const notes: WarningLine[] = [
    ...lines.note,
    ...(plan?.cancel_policy === "NoCancel"
      ? [{ text: t("operations.noCancelHint"), detail: t("operations.noCancelHintDetail") }]
      : []),
    ...(plan?.needs_password ? [{ text: t("commandPreview.needsPassword"), detail: null }] : []),
  ];

  // Two refusals are shown as sentences of their own rather than inside
  // `uninstall.planError`'s "Couldn't check what this affects", because
  // Canager did check: the tool will not uninstall this package (a pinned
  // Homebrew formula or cask, `uninstall_blocked` in
  // crates/canager-core/src/session/plans.rs), which only a stale Installed
  // page can reach and whose sentence carries the unpin command, set apart
  // as code as on the Installed page's row; and a path-list uninstall whose
  // preview refused one of its paths (`uninstall_unsafe`,
  // `removal::plan_removal`), whose sentence names the path and already
  // says nothing was changed.
  function refusal(raw: string, frame: "uninstall.planError" | "uninstall.submitError") {
    const detail = planErrorDetail(t, raw);
    if (parseUninstallUnsafe(raw) !== null) {
      const text = planErrorMessage(t, raw, sourceLabel);
      return <Refusal text={text} detail={detail} detailTitle={text} />;
    }
    const blocked = parseUninstallBlocked(raw);
    if (blocked === null) {
      const text = t(frame, { message: planErrorMessage(t, raw, sourceLabel) });
      return <Refusal text={text} detail={detail} detailTitle={text} />;
    }
    const copy = uninstallBlockedCopy(blocked, instance?.adapter_id);
    const command = copy.command(
      { instance_id: request.instance_id, kind: request.artifact_kind, name: request.name },
      instance,
    );
    return (
      <Refusal
        text={withCommand(t(copy.refused, { command: COMMAND_SLOT, source: sourceLabel }), command)}
        detail={null}
        detailTitle=""
      />
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
      // Cancel first: nothing here should be one keypress from removing.
      initialFocus={cancelRef}
      returnFocusTo={returnFocusTo}
      onClosed={onClosed}
      footer={
        <>
          <button ref={cancelRef} type="button" onClick={() => onOpenChange(false)} className={SHEET_BUTTON.secondary}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            onClick={handleConfirm}
            disabled={!plan || hasAffected || submitMutation.isPending}
            className={SHEET_BUTTON.danger}
          >
            {t(permanent ? "uninstall.confirmPermanent" : "uninstall.confirm")}
          </button>
        </>
      }
    >
      <ul>
        <SheetTool
          adapterId={adapterId}
          sourceLabel={sourceLabel}
          iconKey={{ instance_id: request.instance_id, kind: request.artifact_kind, name: request.name }}
          name={displayName}
          aside={version}
        >
          {/* What goes and what stays, directly under the tool rather than
              behind an ⓘ: the sentence the plan's source has for it. */}
          {lines.scope.map((line) => (
            <p key={line.text} className="mt-1 break-words text-body text-foreground">
              {line.text}
            </p>
          ))}
        </SheetTool>
      </ul>

      {planMutation.isPending ? <SheetPending text={t("uninstall.checking")} /> : null}

      {planMutation.isError ? (
        <div className="mt-4">{refusal(planMutation.error.message, "uninstall.planError")}</div>
      ) : null}

      {submitMutation.isError ? (
        <div className="mt-4">{refusal(submitMutation.error.message, "uninstall.submitError")}</div>
      ) : null}

      {reissued && plan ? (
        // `status`, not `alert`: nothing is wrong with the fresh preview,
        // the user only needs to know the last confirm did not start it.
        <p role="status" className="mt-4 text-body text-muted">
          {/* No "confirm once more" when the fresh preview lists affected
              packages: that disables Uninstall below, and the body says why. */}
          {hasAffected ? t("uninstall.reissued") : t("uninstall.reissuedConfirmAgain")}
        </p>
      ) : null}

      {plan && issued ? (
        <>
          {lines.trash.length > 0 || trashPlan ? (
            <SheetSection title={t("commandPreview.trashLabel")}>
              <SheetLines lines={lines.trash} />
              {trashPlan ? (
                <div className="mt-2">
                  <CommandPreview plans={[{ id: issued.id, action: plan.action }]} />
                </div>
              ) : null}
            </SheetSection>
          ) : null}

          {lines.keep.length > 0 ? (
            <SheetSection title={t("uninstall.keepListTitle")}>
              <SheetLines lines={lines.keep} />
            </SheetSection>
          ) : null}

          {hasAffected || notes.length > 0 ? (
            <SheetSection
              title={t("uninstall.warningsTitle")}
              icon={<WarningIcon size={14} className="shrink-0 text-warning" />}
            >
              {hasAffected ? (
                <div className={notes.length > 0 ? "mb-3" : undefined}>
                  <p className="text-body font-medium text-foreground">{t("uninstall.affectedTitle")}</p>
                  <ul className="mt-1.5 flex flex-wrap gap-1.5">
                    {affected.map((name) => (
                      <li
                        key={name}
                        className="rounded-full bg-[var(--color-hover)] px-2 py-0.5 text-small font-medium text-foreground"
                      >
                        {name}
                      </li>
                    ))}
                  </ul>
                  {/* Why Uninstall below is disabled, said in the body rather
                      than only in a `title` on that disabled button: a
                      disabled button takes no pointer events and drops out
                      of the tab order, so neither a mouse hover nor a
                      keyboard/VoiceOver user ever reached that tooltip.
                      This line is plain text in the flow, reachable by
                      everyone who reached the list above it. */}
                  <p className="mt-1.5 text-body text-danger">
                    {t("uninstall.affectedBlocksConfirm", { name: displayName })}
                  </p>
                </div>
              ) : null}
              <SheetLines lines={notes} />
            </SheetSection>
          ) : null}

          {trashPlan ? null : <CommandPreview plans={[{ id: issued.id, action: plan.action }]} />}
        </>
      ) : null}
    </Dialog>
  );
}
