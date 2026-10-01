import { useEffect, useId, useMemo, useRef, useState, type RefObject } from "react";
import { useTranslation } from "react-i18next";
import { useSettings, useSnapshot, usePlanOperation, useSubmitOperation } from "../lib/queries";
import {
  adapterIdOf,
  adapterLabel,
  instanceLabels,
  parseUninstallBlocked,
  parseUninstallUnsafe,
  planErrorDetail,
  planErrorMessage,
  refusalSentence,
  uninstallBlockedCopy,
} from "../lib/sources";
import type { OpRequest } from "../lib/types";
import { deletesForGood, skipsTrash, warningLines, type WarningLine } from "../lib/warnings";
import { CommandPreview } from "./CommandPreview";
import { KeptDataGroup } from "./KeptDataGroup";
import { twinUninstallLine } from "./TwinAdvice";
import { formatBytes } from "../lib/format";
import { twinsByArtifact } from "../lib/commands";
import { artifactKeyId } from "../store/ui";
import { Refusal, SheetIcon, SheetLines, SheetPending, SheetSection, SheetText, sheetMeta } from "./SheetParts";
import { COMMAND_SLOT, withCommand } from "./withCommand";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";
import { SMALL_WRAPPING } from "./ui/group";

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
 * The uninstall confirmation, as a macOS alert (spec §3.6): the tool's 48
 * icon, 「要卸载“Claude Code”吗？」, its source and version under that,
 * then as its text the one sentence its source's uninstall has about what
 * goes and what stays (`Warning.UninstallScope`) -- going on with
 * 「删除的文件不会进入废纸篓。」 where the source's own command deletes
 * everything in place (`skipsTrash`), and ending 「此操作无法撤销。」
 * where a line says the uninstall deletes something for good
 * (`deletesForGood`): rustup's own, and a cask whose recorded uninstall
 * deletes paths -- and Cancel and Uninstall as the default button, the
 * accent, not red: the user chose it (HIG). 「永久卸载」 on the button too,
 * where the text says so.
 *
 * It plans the operation itself, so everything that would change is on
 * screen before anything can be submitted (spec §6). Under the text, what
 * to know before going on, a line each, a caution marked ⚠︎, with no
 * heading (spec R6): what still needs the package, rustup deleting folders
 * for good, a dependency check that did not finish, that it cannot be
 * stopped once it starts, a password. Then the copy table's two named
 * groups (C4): 「移到废纸篓」, what a path-list uninstall moves -- with
 * what it found already gone -- and the one sentence it has in place of a
 * command; and 「卸载后会保留」, what it leaves where it is, for every kind
 * of uninstall alike (`KeptDataGroup`). A line's longer why is
 * behind its ⓘ. The command itself is one click away (`CommandPreview`),
 * open from the start with technical details on.
 *
 * Uninstall stays disabled while the plan says something still needs the
 * package -- with why, and what to do about it, in the dialog's body next
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
  const { data: settings } = useSettings();
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
  // As the sidebar names it: 「Homebrew（Intel）」 where this Mac has two
  // (`instanceLabels`), so the dialog says which one it uninstalls from.
  const sourceLabel = useMemo(
    () => instanceLabels(t, snapshot?.instances ?? []).get(request.instance_id) ?? adapterLabel(t, adapterId),
    [t, snapshot, request.instance_id, adapterId],
  );
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
  // (`OperationManager::cancel`, crates/banager-core/src/ops/mod.rs):
  // rustup's own uninstall, which removes Rust directory by directory --
  // and that it may ask for the Mac's password. Every Cask uninstall sets
  // `needs_password`, though not every app then asks (the copy table's
  // T3). Spec §6: a password is never a surprise.
  // The tool's other copies, installed by other sources: each stays, and
  // where Terminal runs one of them now, its command still works after
  // (`twinUninstallLine`).
  const twinLine = useMemo(() => {
    if (artifact === undefined || snapshot === undefined) return null;
    const labels = instanceLabels(t, snapshot.instances);
    const labelFor = (instanceId: string) => labels.get(instanceId) ?? adapterLabel(t, adapterIdOf(instanceId));
    return twinUninstallLine(t, artifact, twinsByArtifact(snapshot.artifacts).get(artifactKeyId(artifact.key)), labelFor);
  }, [t, artifact, snapshot]);
  // A model's own size, as Ollama reports it: about what removing it frees,
  // less the layers another model shares, which stay.
  const frees =
    artifact !== undefined && artifact.key.kind === "Model" && artifact.size_bytes !== null
      ? t("clarity.freesModel", { size: formatBytes(artifact.size_bytes) })
      : null;
  const notes: WarningLine[] = [
    ...(frees === null ? [] : [{ text: frees, detail: null, caution: false }]),
    ...(twinLine === null ? [] : [{ text: twinLine, detail: null, caution: false }]),
    ...lines.note,
    ...(plan?.cancel_policy === "NoCancel"
      ? [{ text: t("operations.noCancelHint"), detail: t("operations.noCancelHintDetail"), caution: true }]
      : []),
    ...(plan?.needs_password ? [{ text: t("commandPreview.needsPassword"), detail: null, caution: false }] : []),
  ];

  // Two refusals are shown as sentences of their own rather than inside
  // `uninstall.planError`'s "Couldn't check what this affects", because
  // Banager did check: the tool will not uninstall this package (a pinned
  // Homebrew formula or cask, `uninstall_blocked` in
  // crates/banager-core/src/session/plans.rs), which only a stale Installed
  // page can reach and whose sentence carries the unpin command, set apart
  // as code as on the Installed page's row; and a path-list uninstall whose
  // preview refused one of its paths (`uninstall_unsafe`,
  // `removal::plan_removal`), whose sentence names the path and already
  // says nothing was changed.
  //
  // Any other says it in `frame`, without the backend's own words unless
  // "Show technical details" is on (`refusalSentence`).
  function refusal(raw: string, frame: "uninstall.planError" | "uninstall.submitError") {
    const detail = planErrorDetail(t, raw);
    const technical = settings?.show_technical_details ?? false;
    const unsafe = parseUninstallUnsafe(raw) !== null ? planErrorMessage(t, raw, sourceLabel, technical) : null;
    if (unsafe !== null) {
      return <Refusal text={unsafe} detail={detail} detailTitle={unsafe} />;
    }
    const blocked = parseUninstallBlocked(raw);
    if (blocked === null) {
      const text = refusalSentence(t, frame, raw, sourceLabel, technical);
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

  // What goes and what stays, as the alert's text: the sentence the plan's
  // source has for it; then, where that sentence holds it (`skipsTrash`),
  // that none of what goes is in the Trash afterwards -- the path-list
  // uninstalls say what they move there, and this one moves nothing; and,
  // where a line says something is deleted for good, that this cannot be
  // undone -- the one place it is said. As Finder's Delete Immediately
  // alert says both, in one paragraph.
  const scope = lines.scope.map((line) => line.text);
  const textId = useId();
  const said =
    plan !== undefined && scope.length > 0 && skipsTrash(plan.warnings)
      ? endWith(scope, "uninstall.endsSkipsTrash")
      : scope;
  const text =
    !permanent
      ? said
      : said.length === 0
        ? [t("uninstall.cannotUndo")]
        : endWith(said, "uninstall.endsCannotUndo");

  /** `sentences`, the last one carrying on with `key`'s sentence. */
  function endWith(sentences: string[], key: "uninstall.endsSkipsTrash" | "uninstall.endsCannotUndo"): string[] {
    return [...sentences.slice(0, -1), t(key, { sentence: sentences[sentences.length - 1] })];
  }

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("uninstall.title", { name: displayName })}
      icon={
        <SheetIcon
          adapterId={adapterId}
          sourceLabel={sourceLabel}
          iconKey={{ instance_id: request.instance_id, kind: request.artifact_kind, name: request.name }}
        />
      }
      subtitle={sheetMeta(displayName, sourceLabel, version)}
      // What goes and what stays: the alert's text, said as it opens.
      describedBy={text.map((_, index) => `${textId}-${index}`).join(" ")}
      // Cancel first: nothing here should be one keypress from removing.
      initialFocus={cancelRef}
      returnFocusTo={returnFocusTo}
      onClosed={onClosed}
      footer={
        <>
          <button ref={cancelRef} type="button" onClick={() => onOpenChange(false)} className={BUTTON.large.grey}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            onClick={handleConfirm}
            disabled={!plan || hasAffected || submitMutation.isPending}
            className={BUTTON.large.default}
          >
            {t(permanent ? "uninstall.confirmPermanent" : "uninstall.confirm")}
          </button>
        </>
      }
    >
      {text.map((sentence, index) => (
        <SheetText key={sentence} id={`${textId}-${index}`}>
          {sentence}
        </SheetText>
      ))}

      {planMutation.isPending ? <SheetPending text={t("uninstall.checking")} /> : null}

      {planMutation.isError ? (
        <div className="mt-3">{refusal(planMutation.error.message, "uninstall.planError")}</div>
      ) : null}

      {submitMutation.isError ? (
        <div className="mt-3">{refusal(submitMutation.error.message, "uninstall.submitError")}</div>
      ) : null}

      {/* `status`, not `alert`: nothing is wrong with the fresh preview,
          the user only needs to know the last confirm did not start it.
          There, empty and out of sight, from the time the sheet opens: a
          status put in the page with its words is one a screen reader
          may never read. */}
      <p role="status" className={reissued && plan ? "mt-3 text-body text-muted" : "sr-only"}>
        {/* No "confirm once more" when the fresh preview lists affected
            packages: that disables Uninstall below, and the body says why. */}
        {reissued && plan ? (hasAffected ? t("uninstall.reissued") : t("uninstall.reissuedConfirmAgain")) : null}
      </p>

      {plan && issued ? (
        <>
          {/* What to know before going on, first, under the text and with
              no heading of its own (spec §3.6): what still needs the
              package, then a line each for the rest, a caution marked. */}
          {hasAffected || notes.length > 0 ? (
            <SheetSection title={t("uninstall.warningsTitle")} titleHidden>
              {hasAffected ? (
                <div className={notes.length > 0 ? "mb-2" : undefined}>
                  <h3 className="text-small font-bold text-muted">{t("uninstall.affectedTitle")}</h3>
                  <ul className="mt-1 flex flex-col gap-1">
                    {affected.map((name) => (
                      <li key={name} className={`text-foreground ${SMALL_WRAPPING}`}>
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
                  <p className="mt-2 text-body text-danger-text">
                    {t("uninstall.affectedBlocksConfirm", { name: displayName })}
                  </p>
                </div>
              ) : null}
              <SheetLines lines={notes} />
            </SheetSection>
          ) : null}

          {lines.trash.length > 0 || trashPlan ? (
            <SheetSection title={t("commandPreview.trashLabel")}>
              <SheetLines lines={lines.trash} />
              {trashPlan ? (
                <div className="mt-1">
                  <CommandPreview plans={[{ id: issued.id, action: plan.action }]} />
                </div>
              ) : null}
            </SheetSection>
          ) : null}

          {/* What stays: a tool's own installer's list, a tool's settings
              and data, Ollama's models -- one group for every uninstall,
              and nothing here offers to remove any of it (`KeptDataGroup`). */}
          <KeptDataGroup warnings={plan.warnings} />

          {trashPlan ? null : <CommandPreview plans={[{ id: issued.id, action: plan.action }]} />}
        </>
      ) : null}
    </Dialog>
  );
}
