import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { usePlanOperation, useSubmitOperation } from "../lib/queries";
import { planErrorMessage } from "../lib/sources";
import { warningTexts } from "../lib/warnings";
import { artifactKeyId, useUiStore } from "../store/ui";
import type { ArtifactKey, IssuedPlan, OpRequest, UpdateCandidate } from "../lib/types";
import { CommandPreview } from "./CommandPreview";
import { Dialog } from "./ui/Dialog";

function toRequest(candidate: UpdateCandidate): OpRequest {
  return {
    kind: "Upgrade",
    instance_id: candidate.key.instance_id,
    artifact_kind: candidate.key.kind,
    name: candidate.key.name,
  };
}

function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/**
 * One selected row's journey through a batch. Exactly one of `issued` /
 * `planError` is set once its plan settles; exactly one of `submittedOpId` /
 * `submitError` once its submit settles. A row whose plan failed is listed
 * in the dialog with its reason and is never submitted.
 */
export interface BatchItem {
  // The whole candidate, not just its key: the confirmation has to say
  // what version you are moving to, and `current`/`target`/`channel` live
  // here and nowhere else once the dialog is open. Captured when the batch
  // is built, so a refresh landing behind the dialog cannot change the
  // numbers under the command the user is reading.
  candidate: UpdateCandidate;
  /** The row's name, as the list showed it when the batch was built. */
  name: string;
  issued: IssuedPlan | null;
  planError: string | null;
  submittedOpId: number | null;
  submitError: string | null;
}

/**
 * The confirmation flow's whole state, kept explicitly instead of being read
 * off `usePlanOperation`/`useSubmitOperation`'s observer flags: an observer
 * only ever reflects its *last* call, so a batch of N `mutateAsync` calls
 * would report one result and lose the other N−1 (A fails, B succeeds: the
 * page would show B's success and swallow A's error).
 *
 *   planning ─(every plan settled)─▶ ready ─(Confirm)─▶ submitting ─▶ done
 *
 * The dialog opens at `ready` if at least one plan was issued; when every
 * plan failed the batch goes straight to `done` with the dialog shut and
 * the reasons shown on the page. `done` is reached after submitting only
 * when something failed — a batch whose every item started closes the
 * dialog instead. `id` is compared with `batchIdRef` before any async
 * callback writes back, so a superseded batch's late reply can neither
 * overwrite a newer preview nor close a newer dialog.
 */
export interface Batch {
  id: number;
  phase: "planning" | "ready" | "submitting" | "done";
  items: BatchItem[];
}

function hasIssuedPlan(batch: Batch): boolean {
  return batch.items.some((item) => item.issued !== null);
}

/**
 * Narrows a `BatchItem` to the branch where its plan failed. `issued` and
 * `planError` are set as a pair in `openConfirm` -- a fulfilled `mutateAsync`
 * sets `issued` and leaves `planError` null, a rejected one does the
 * reverse -- so this is never false for an item already known to have no
 * `issued` plan (see `pageErrors` below), but the compiler has no way to see
 * that invariant across the two fields on its own.
 */
function hasPlanError(item: BatchItem): item is BatchItem & { planError: string } {
  return item.planError !== null;
}

export interface UpdateConfirmOptions {
  /** The name a row shows, which the confirmation names it by. */
  nameOf: (candidate: UpdateCandidate) => string;
  /** The list's own order, which the confirmation lists items in. */
  compare: (a: UpdateCandidate, b: UpdateCandidate) => number;
  /** The source's name, for a refusal's sentence (`planErrorMessage`). */
  sourceLabelFor: (instanceId: string) => string;
}

/** What `useUpdateConfirm` hands the page, and `UpdateConfirmDialog` draws. */
export interface UpdateConfirm {
  /** Plans every one of `chosen` and, once one plan is back, opens the confirmation. */
  openConfirm(chosen: UpdateCandidate[]): Promise<void>;
  /** The confirmation is on screen: the Update buttons that open it are off meanwhile. */
  dialogOpen: boolean;
  /**
   * Every plan failed, so there is nothing to confirm: the reasons, for the
   * page to show where the user pressed Update. Cleared by the next batch.
   */
  pageErrors: Array<BatchItem & { planError: string }>;
  batch: Batch | null;
  submitting: boolean;
  confirmAndSubmit(): Promise<void>;
  close(): void;
}

/**
 * The update confirmation, one flow for every page that offers Update:
 * the Updates page's rows, Update selected and Update all, and the
 * Installed page's detail. Plans each update, shows the exact command,
 * the version it moves to and every warning, then submits one after the
 * other and remembers which version each operation is for
 * (`rememberUpdateTarget`), so the row's progress
 * (`useUpdateOperationFor`) can tell its outcome from a later version's.
 */
export function useUpdateConfirm({ nameOf, compare, sourceLabelFor }: UpdateConfirmOptions): UpdateConfirm {
  const { t } = useTranslation();
  // Used only for their promise-returning `mutateAsync` — which keeps
  // `useSubmitOperation`'s operations-query invalidation — never for their
  // `isPending`/`isError`/`error`; every flag the UI needs comes from `batch`.
  const planMutation = usePlanOperation();
  const submitMutation = useSubmitOperation();
  const [batch, setBatch] = useState<Batch | null>(null);
  // Monotonic. The batch whose id equals this is the only one allowed to
  // write state; every async continuation checks `isCurrent` after `await`.
  const batchIdRef = useRef(0);

  function isCurrent(id: number): boolean {
    return batchIdRef.current === id;
  }

  function deselect(key: ArtifactKey) {
    // Read the store directly: this runs after an `await`, when a
    // selection captured by an earlier render may already be stale.
    const store = useUiStore.getState();
    if (store.selectedUpdates.includes(artifactKeyId(key))) {
      store.toggleUpdate(key);
    }
  }

  async function openConfirm(chosen: UpdateCandidate[]) {
    // A new id retires whatever batch was still planning. Planning has no
    // side effect beyond issuing PlanIds that expire on their own, so the
    // newest click wins and the older batch's late replies are dropped by
    // `isCurrent`. Submitting is different — see the lock in the dialog.
    const id = batchIdRef.current + 1;
    batchIdRef.current = id;
    // In the list's own order, so the confirmation reads as the rows did.
    const candidates = [...chosen].sort(compare);
    const blank = (c: UpdateCandidate): BatchItem => ({
      candidate: c,
      name: nameOf(c),
      issued: null,
      planError: null,
      submittedOpId: null,
      submitError: null,
    });
    setBatch({ id, phase: "planning", items: candidates.map(blank) });

    // allSettled, not all: one rejected plan must not hide the others, and
    // each item keeps its own backend message verbatim.
    const results = await Promise.allSettled(
      candidates.map((c) => planMutation.mutateAsync(toRequest(c))),
    );
    if (!isCurrent(id)) return;

    const items = candidates.map((c, i): BatchItem => {
      const result = results[i];
      return {
        ...blank(c),
        issued: result.status === "fulfilled" ? result.value : null,
        planError:
          result.status === "rejected"
            ? planErrorMessage(t, errorMessage(result.reason), sourceLabelFor(c.key.instance_id))
            : null,
      };
    });
    // Nothing to confirm when no plan came back: the dialog stays shut and
    // the reasons are rendered on the page (see `pageErrors` below).
    setBatch({ id, phase: items.some((item) => item.issued !== null) ? "ready" : "done", items });
  }

  async function confirmAndSubmit() {
    if (!batch || batch.phase !== "ready") return;
    const { id } = batch;
    const items = [...batch.items];
    setBatch({ id, phase: "submitting", items });

    // Sequential, not concurrent: each item's result is recorded before the
    // next is sent, so a failure part-way leaves an exact record of what did
    // start. A started item leaves the selection at once, so a retry after a
    // partial failure re-plans only what never started — a single-use PlanId
    // cannot stop the same item being re-queued under a fresh id, only the
    // selection can.
    for (let i = 0; i < items.length; i += 1) {
      const item = items[i];
      if (!item.issued) continue;
      try {
        const opId = await submitMutation.mutateAsync(item.issued.id);
        items[i] = { ...item, submittedOpId: opId };
        // Which version this operation is for, so its row can tell its
        // outcome from a later version's (`useUpdateOperationFor`).
        // Recorded whether or not this batch is still current: the
        // operation runs.
        useUiStore.getState().rememberUpdateTarget(opId, item.candidate.target);
        // Guarded like every other post-await write: `deselect` mutates the
        // shared selection store, so a superseded batch must not reach it.
        if (isCurrent(id)) deselect(item.candidate.key);
      } catch (e) {
        // A PlanId is single-use and expires after 10 minutes. Whatever the
        // backend said (`Expired`, `Unknown`, anything else), this id is
        // spent: record the reason and carry on with the next item.
        // Through `planErrorMessage` like the planning failure above, for
        // the same reason: `submit` re-runs the actionability gate against
        // the current snapshot, so "that source stopped answering while
        // you were reading this" is a refusal this path can produce, and
        // it must not arrive as JSON or as a Rust enum.
        items[i] = {
          ...item,
          submitError: planErrorMessage(
            t,
            errorMessage(e),
            sourceLabelFor(item.candidate.key.instance_id),
          ),
        };
      }
      if (!isCurrent(id)) return;
      setBatch({ id, phase: "submitting", items: [...items] });
    }
    if (!isCurrent(id)) return;

    const anyFailed = items.some((item) => item.planError !== null || item.submitError !== null);
    // Only a clean sweep closes the dialog; otherwise it stays open and
    // says, per item, what started and what did not, and why.
    setBatch(anyFailed ? { id, phase: "done", items } : null);
  }

  const dialogOpen = batch !== null && batch.phase !== "planning" && hasIssuedPlan(batch);
  const submitting = batch?.phase === "submitting";
  // Every plan failed: there is nothing to confirm, so the reasons go on the
  // page rather than into an empty dialog. Cleared by the next batch.
  const pageErrors =
    batch !== null && batch.phase === "done" && !hasIssuedPlan(batch)
      ? batch.items.filter(hasPlanError)
      : [];

  return {
    openConfirm,
    dialogOpen,
    pageErrors,
    batch,
    submitting,
    confirmAndSubmit,
    close: () => setBatch(null),
  };
}

export interface UpdateConfirmDialogProps {
  confirm: UpdateConfirm;
}

/**
 * The confirmation `useUpdateConfirm` drives: each item's name, the
 * version it moves to, the exact command, whether it can be cancelled,
 * its warnings and whether it asks for a password -- then, once
 * submitted, what started and what did not.
 */
export function UpdateConfirmDialog({ confirm }: UpdateConfirmDialogProps) {
  const { t } = useTranslation();
  const { batch, dialogOpen, submitting } = confirm;

  /**
   * The version jump for the confirmation dialog, or null when there is no
   * honest one to show: a `Digest` candidate says a newer build of the
   * model is available, never two digests -- they are from different hash
   * spaces (crates/canager-core/src/adapters/ollama/mod.rs) -- and nothing,
   * rather than a dangling arrow, when a source could name only one side.
   * Not behind "Show technical details": spec §6 asks this screen to show
   * the version jump, and a confirmation that names the command but not the
   * change is not a confirmation.
   */
  const dialogVersionJump = (candidate: UpdateCandidate): string | null => {
    if (candidate.channel === "Digest") return t("updates.newBuild");
    if (candidate.current === "" || candidate.target === "") return null;
    return t("updates.versionChange", { current: candidate.current, target: candidate.target });
  };

  return (
    <Dialog
      open={dialogOpen}
      onOpenChange={(open) => {
        // Escape and overlay clicks arrive here. A submitting batch runs to
        // completion no matter what — closing early would leave the old
        // loop running against a dialog the user might reopen — so the
        // request is ignored until it has settled. The footer follows the
        // same rule: Cancel is disabled while submitting.
        if (!open && !submitting) confirm.close();
      }}
      title={t("updates.confirmTitle")}
      footer={
        batch?.phase === "done" ? (
          <button type="button" onClick={confirm.close} className="rounded-md px-3 py-1 text-sm">
            {t("common.close")}
          </button>
        ) : (
          <>
            <button
              type="button"
              onClick={confirm.close}
              disabled={submitting}
              className="rounded-md px-3 py-1 text-sm disabled:opacity-50"
            >
              {t("common.cancel")}
            </button>
            <button
              type="button"
              onClick={confirm.confirmAndSubmit}
              disabled={batch?.phase !== "ready"}
              className="rounded-md bg-[var(--color-accent)] px-3 py-1 text-sm font-medium text-[var(--color-accent-foreground)] disabled:opacity-50"
            >
              {t("updates.confirmUpdate")}
            </button>
          </>
        )
      }
    >
      <div className="flex flex-col gap-4">
        {(batch?.items ?? []).map((item) => {
          const itemWarnings = item.issued ? warningTexts(t, item.issued.plan.warnings) : [];
          const jump = dialogVersionJump(item.candidate);
          return (
            <div key={artifactKeyId(item.candidate.key)} className="flex flex-col gap-1">
              <p className="text-sm font-medium text-[var(--color-foreground)]">{item.name}</p>
              {/* What you are moving to, spelled out (`dialogVersionJump`). */}
              {jump !== null ? <p className="text-sm text-[var(--color-muted)]">{jump}</p> : null}
              {item.planError !== null ? (
                <p role="alert" className="text-sm text-[var(--color-danger)]">
                  {t("updates.planFailed", { message: item.planError })}
                </p>
              ) : null}
              {item.issued !== null ? <CommandPreview action={item.issued.plan.action} /> : null}
              {item.issued?.plan.cancel_policy === "NoCancel" ? (
                // Per item, next to the command it is true of (a batch
                // can mix a rustup self update with Homebrew upgrades):
                // once Running, `OperationBar` offers no Cancel for it.
                <p className="text-sm font-medium text-[var(--color-foreground)]">
                  {t("operations.noCancelHint")}
                </p>
              ) : null}
              {itemWarnings.length > 0 ? (
                <ul className="list-disc pl-5 text-sm text-[var(--color-foreground)]">
                  {itemWarnings.map((warning) => (
                    <li key={warning}>{warning}</li>
                  ))}
                </ul>
              ) : null}
              {item.issued?.plan.needs_password ? (
                // Per item, not per batch: a batch can mix Casks (which the
                // brew adapter marks) and formulae (which it does not), so
                // the notice belongs next to the command that will trigger
                // the prompt. Spec §6: a password is never a surprise.
                <p className="text-sm font-medium text-[var(--color-foreground)]">
                  {t("commandPreview.needsPassword")}
                </p>
              ) : null}
              {item.submittedOpId !== null ? (
                <p className="text-sm text-[var(--color-muted)]">{t("updates.started")}</p>
              ) : null}
              {item.submitError !== null ? (
                <p role="alert" className="text-sm text-[var(--color-danger)]">
                  {t("updates.submitFailed", { message: item.submitError })}
                </p>
              ) : null}
            </div>
          );
        })}
      </div>
    </Dialog>
  );
}
