import { useEffect, useId, useRef, useState, type RefObject } from "react";
import { useTranslation } from "react-i18next";
import { usePlanOperation, useSnapshot, useSubmitOperation } from "../lib/queries";
import { adapterIdOf, adapterLabel, planErrorDetail, planErrorMessage } from "../lib/sources";
import { warningLines, type WarningLine } from "../lib/warnings";
import { artifactKeyId, useUiStore } from "../store/ui";
import type { ArtifactKey, IssuedPlan, OpRequest, UpdateCandidate } from "../lib/types";
import { CommandPreview } from "./CommandPreview";
import { SheetLines, SheetPending, Refusal, SheetSection, SheetTool } from "./SheetParts";
import { CheckIcon, ChevronIcon, WarningIcon } from "./icons";
import { Dialog, SHEET_BUTTON } from "./ui/Dialog";

/**
 * How many tools a long confirmation lists before 「还有 N 个」, so that
 * Update all over ten tools is not a sheet of list with the commands far
 * below it. Folded only past one more than this, so the fold never hides
 * a single tool behind a line as tall as it.
 */
const FOLDED_TOOLS = 5;

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
  /** `planError`'s longer why, for its ⓘ (`planErrorDetail`), or null. */
  planErrorDetail: string | null;
  submittedOpId: number | null;
  submitError: string | null;
  /** `submitError`'s longer why, for its ⓘ, or null. */
  submitErrorDetail: string | null;
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
 * The dialog opens at `planning`, the moment Update is pressed: the tools
 * and their versions are known from the rows, and only what to know
 * before going on, and the commands, wait for the plans. It stays open at
 * `ready` if at least one plan was issued; when every plan failed the
 * batch goes straight to `done`, the dialog shuts and the reasons are
 * shown on the page. `done` is reached after submitting only when
 * something failed — a batch whose every item started closes the dialog
 * instead. `id` is compared with `batchIdRef` before any async callback
 * writes back, so the late reply of a batch that was superseded -- or
 * closed while it was still planning -- can neither overwrite a newer
 * preview, nor open or close a dialog.
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
  /**
   * Opens the confirmation on `chosen` at once, preparing, and plans every
   * one of them. `opener` is what was pressed -- a row's Update, Update
   * selected, Update all, a drawer's Update -- which gets the focus back
   * when the confirmation closes; without it, what has the focus now.
   * Passed rather than read when the sheet opens, because by then the
   * button is disabled (`dialogOpen`), and a click in WebKit does not
   * focus a button at all.
   */
  openConfirm(chosen: UpdateCandidate[], opener?: HTMLElement | null): Promise<void>;
  /** What the confirmation gives the focus back to (`openConfirm`'s `opener`). */
  returnFocusTo: RefObject<HTMLElement | null>;
  /**
   * The confirmation is on screen, preparing or ready: the Update buttons
   * that open it are off meanwhile.
   */
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
 * Installed page's detail. Plans each update, shows the version it moves
 * to and every warning, with the exact command one click away, then
 * submits one after the other and remembers which version each operation
 * is for (`rememberUpdateTarget`), so the row's progress
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
  // What opened the newest batch (`openConfirm`'s `opener`).
  const openerRef = useRef<HTMLElement | null>(null);

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

  async function openConfirm(chosen: UpdateCandidate[], opener?: HTMLElement | null) {
    // A new id retires whatever batch was still planning (`close` retires
    // one too). Planning has no side effect beyond issuing PlanIds that
    // expire on their own, so the newest batch wins and an older one's
    // late replies are dropped by `isCurrent`. Submitting is different —
    // see the lock in the dialog.
    const id = batchIdRef.current + 1;
    batchIdRef.current = id;
    openerRef.current =
      opener ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
    // In the list's own order, so the confirmation reads as the rows did.
    const candidates = [...chosen].sort(compare);
    const blank = (c: UpdateCandidate): BatchItem => ({
      candidate: c,
      name: nameOf(c),
      issued: null,
      planError: null,
      planErrorDetail: null,
      submittedOpId: null,
      submitError: null,
      submitErrorDetail: null,
    });
    // The sheet is up from here, preparing: the tools and their versions
    // are the rows', and nothing waits on the backend to show them.
    setBatch({ id, phase: "planning", items: candidates.map(blank) });

    // allSettled, not all: one rejected plan must not hide the others, and
    // each item keeps its own backend message verbatim. The notes and the
    // commands arrive together, once every plan has settled, and so does
    // Update: nothing of a batch can be confirmed before all of it is on
    // the sheet.
    const results = await Promise.allSettled(
      candidates.map((c) => planMutation.mutateAsync(toRequest(c))),
    );
    if (!isCurrent(id)) return;

    const items = candidates.map((c, i): BatchItem => {
      const result = results[i];
      if (result.status === "fulfilled") return { ...blank(c), issued: result.value };
      const raw = errorMessage(result.reason);
      return {
        ...blank(c),
        planError: planErrorMessage(t, raw, sourceLabelFor(c.key.instance_id)),
        planErrorDetail: planErrorDetail(t, raw),
      };
    });
    // Nothing to confirm when no plan came back: the dialog shuts and the
    // reasons are rendered on the page (see `pageErrors` below).
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
        const raw = errorMessage(e);
        items[i] = {
          ...item,
          submitError: planErrorMessage(t, raw, sourceLabelFor(item.candidate.key.instance_id)),
          submitErrorDetail: planErrorDetail(t, raw),
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

  function close() {
    // A batch closed while it is still planning is retired, so that its
    // plans, arriving after, open nothing. A submitting one is never
    // closed -- the dialog refuses to (its lock) -- and ready or done, it
    // has nothing left to arrive.
    if (batch?.phase === "planning") batchIdRef.current += 1;
    setBatch(null);
  }

  // Up from the press: preparing, then with what there is to confirm. Shut
  // when every plan failed, whose reasons are then the page's.
  const dialogOpen = batch !== null && (batch.phase === "planning" || hasIssuedPlan(batch));
  const submitting = batch?.phase === "submitting";
  // Every plan failed: there is nothing to confirm, so the reasons go on the
  // page rather than into an empty dialog. Cleared by the next batch.
  const pageErrors =
    batch !== null && batch.phase === "done" && !hasIssuedPlan(batch)
      ? batch.items.filter(hasPlanError)
      : [];

  return {
    openConfirm,
    returnFocusTo: openerRef,
    dialogOpen,
    pageErrors,
    batch,
    submitting,
    confirmAndSubmit,
    close,
  };
}

export interface UpdateConfirmDialogProps {
  confirm: UpdateConfirm;
}

/**
 * The confirmation `useUpdateConfirm` drives, as a sheet: 「更新 3 个工具？」
 * -- 「更新 ffmpeg？」 for one -- the tools with their avatars and the
 * version each moves to, what to know before going on, then Cancel and
 * Update. Once submitted, what started and what did not, under each tool.
 *
 * It is up the moment Update is pressed, with the tools and versions the
 * rows had and 「正在准备…」 where the notes will go -- the uninstall
 * sheet's 「正在检查影响…」, in the same look (`SheetPending`) -- and
 * Update off, with the sheet itself holding the focus, until every plan
 * is back. Then the notes and the commands, Update on, and the focus on
 * it, unless the user has put it somewhere else meanwhile.
 *
 * The notes are grouped under 「请注意」, a tool's own under its name where
 * the sheet lists several: its warnings, that it cannot be stopped once it
 * starts, that it may ask for the Mac's password. A batch can mix a rustup
 * self update with Homebrew upgrades, and Casks with formulae, so each
 * note stays with the tool it is true of. A line's longer why is behind
 * its ⓘ. The commands are one click away (`CommandPreview`), each under
 * its tool's name, open from the start with technical details on.
 *
 * Until it is submitted, 「请注意」 comes first, above the tools: Update all
 * over ten tools filled the sheet with its list, and the notes were below
 * the fold, under an Update that had the focus. Beside Cancel and Update,
 * 「有 4 条需要留意」 says how many there are, and takes the focus -- and
 * the sheet's scroll -- to them. A list longer than six shows its first
 * five and the rest one press away, unless a tool in it has a refusal to
 * show. Once it is done, what did not start comes first instead.
 */
export function UpdateConfirmDialog({ confirm }: UpdateConfirmDialogProps) {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { batch, dialogOpen, submitting } = confirm;
  const updateRef = useRef<HTMLButtonElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const notesRef = useRef<HTMLElement>(null);
  const toolsId = useId();
  // The batch whose whole list the user unfolded: a new batch starts folded.
  const [unfoldedBatch, setUnfoldedBatch] = useState<number | null>(null);
  const items = batch?.items ?? [];
  const issued = items.filter(
    (item): item is BatchItem & { issued: IssuedPlan } => item.issued !== null,
  );
  // More than one tool on the sheet: each note and command says whose it is.
  const several = items.length > 1;

  /**
   * The version jump for the confirmation, or null when there is no honest
   * one to show: a `Digest` candidate says a newer build of the model is
   * available, never two digests -- they are from different hash spaces
   * (crates/canager-core/src/adapters/ollama/mod.rs) -- and nothing,
   * rather than a dangling arrow, when a source could name only one side.
   * Not behind "Show technical details": spec §6 asks this screen to show
   * the version jump, and a confirmation that names the command but not
   * the change is not a confirmation.
   */
  const versionJump = (candidate: UpdateCandidate): string | null => {
    if (candidate.channel === "Digest") return t("updates.newBuild");
    if (candidate.current === "" || candidate.target === "") return null;
    return t("updates.versionChange", { current: candidate.current, target: candidate.target });
  };

  // The avatar and the source's name go by its adapter: its instance's,
  // or -- for an instance the snapshot has lost -- the one the id names.
  const adapterFor = (instanceId: string): string =>
    snapshot?.instances.find((instance) => instance.id === instanceId)?.adapter_id ??
    adapterIdOf(instanceId);

  // A batch that did not all start stays open to say which did not, with
  // Close in place of Cancel and Update -- where the focus goes, rather
  // than with the Update button it was on.
  //
  // Ready, Update takes the focus the sheet held for it while it was off
  // (`Dialog`'s `initialFocus`) -- only from the sheet itself: a focus the
  // user moved to Cancel or into the sheet's body while it was preparing
  // stays where they put it, so that a key pressed there is never taken
  // by Update instead.
  const phase = batch?.phase;
  useEffect(() => {
    if (phase === "done") closeRef.current?.focus();
    if (phase === "ready") {
      const update = updateRef.current;
      const sheet = update?.closest('[role="dialog"]');
      if (update && sheet && document.activeElement === sheet) update.focus();
    }
  }, [phase]);

  const notesOf = (item: BatchItem & { issued: IssuedPlan }): WarningLine[] => {
    const { plan } = item.issued;
    const lines = warningLines(t, plan.warnings);
    return [
      ...lines.trash,
      ...lines.keep,
      ...lines.note,
      // Once Running, `OperationBar` offers no Cancel for a NoCancel
      // operation (`OperationManager::cancel`, crates/canager-core/src/ops).
      ...(plan.cancel_policy === "NoCancel"
        ? [{ text: t("operations.noCancelHint"), detail: t("operations.noCancelHintDetail") }]
        : []),
      // The brew adapter marks every Cask, though not every app then asks
      // (the copy table's T3). Spec §6: a password is never a surprise.
      ...(plan.needs_password ? [{ text: t("commandPreview.needsPassword"), detail: null }] : []),
    ];
  };
  const noted = issued
    .map((item) => ({ item, notes: notesOf(item) }))
    .filter(({ notes }) => notes.length > 0);
  const noteCount = noted.reduce((count, { notes }) => count + notes.length, 0);

  // While it prepares, it asks about everything chosen; then about what
  // can be confirmed of it.
  const asked = phase === "planning" ? items : issued;
  const title =
    asked.length === 1
      ? t("updates.confirmTitleNamed", { name: asked[0].name })
      : t("updates.confirmTitle", { count: asked.length });

  // The first `FOLDED_TOOLS` tools, and the rest one press away -- never
  // while one of them has a refusal to show, which every tool then shows.
  const foldable =
    items.length > FOLDED_TOOLS + 1 &&
    items.every((item) => item.planError === null && item.submitError === null);
  const unfolded = batch !== null && unfoldedBatch === batch.id;
  const shownItems = foldable && !unfolded ? items.slice(0, FOLDED_TOOLS) : items;
  // What there is to know before Update, first; what did not start, once done.
  const notesFirst = phase !== "done";

  const notesSection =
    noted.length > 0 ? (
      <SheetSection
        ref={notesRef}
        first={notesFirst}
        title={t("updates.warningsTitle")}
        icon={<WarningIcon size={14} className="shrink-0 text-warning" />}
      >
        <div className="flex flex-col gap-3">
          {noted.map(({ item, notes: lines }) => (
            // A `<div>` per tool, holding its name and its notes and no
            // other tool's.
            <div key={artifactKeyId(item.candidate.key)}>
              {several ? <p className="mb-1 text-body font-medium text-foreground">{item.name}</p> : null}
              <SheetLines lines={lines} />
            </div>
          ))}
        </div>
      </SheetSection>
    ) : null;

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
      title={title}
      initialFocus={batch?.phase === "done" ? closeRef : updateRef}
      returnFocusTo={confirm.returnFocusTo}
      footer={
        batch?.phase === "done" ? (
          <button ref={closeRef} type="button" onClick={confirm.close} className={SHEET_BUTTON.secondary}>
            {t("common.close")}
          </button>
        ) : (
          <>
            {noteCount > 0 ? (
              // How many notes there are, where the eye is when Update is
              // pressed; it takes the focus, and the scroll, to them.
              <button
                type="button"
                onClick={() => notesRef.current?.focus()}
                className="mr-auto inline-flex min-w-0 items-center gap-1.5 rounded-sm text-small font-medium text-foreground outline-none hover:underline focus-visible:ring-2 focus-visible:ring-accent"
              >
                <WarningIcon size={14} className="shrink-0 text-warning" />
                {t("updates.notesSummary", { count: noteCount })}
              </button>
            ) : null}
            <button type="button" onClick={confirm.close} disabled={submitting} className={SHEET_BUTTON.secondary}>
              {t("common.cancel")}
            </button>
            <button
              ref={updateRef}
              type="button"
              onClick={confirm.confirmAndSubmit}
              disabled={batch?.phase !== "ready"}
              className={SHEET_BUTTON.primary}
            >
              {t("updates.update")}
            </button>
          </>
        )
      }
    >
      {notesFirst ? notesSection : null}

      <ul id={toolsId} className={notesFirst && notesSection !== null ? "mt-5 flex flex-col" : "flex flex-col"}>
        {shownItems.map((item) => {
          const { key } = item.candidate;
          const jump = versionJump(item.candidate);
          const digest = item.candidate.channel === "Digest";
          const planFailed = t("updates.planFailed", { message: item.planError ?? "" });
          const submitFailed = t("updates.submitFailed", { message: item.submitError ?? "" });
          const adapterId = adapterFor(key.instance_id);
          return (
            <SheetTool
              key={artifactKeyId(key)}
              adapterId={adapterId}
              sourceLabel={adapterLabel(t, adapterId)}
              iconKey={key}
              name={item.name}
              // A model's "newer build" is a sentence, not a number: under the name.
              aside={digest ? null : jump}
            >
              {digest && jump !== null ? <p className="text-small text-muted">{jump}</p> : null}
              {item.planError !== null ? (
                <Refusal
                  text={planFailed}
                  detail={item.planErrorDetail}
                  detailTitle={planFailed}
                  className="mt-1"
                />
              ) : null}
              {item.submittedOpId !== null ? (
                <p className="mt-1 flex items-center gap-1 text-small font-medium text-success">
                  <CheckIcon size={13} className="shrink-0" />
                  {t("updates.started")}
                </p>
              ) : null}
              {item.submitError !== null ? (
                <Refusal
                  text={submitFailed}
                  detail={item.submitErrorDetail}
                  detailTitle={submitFailed}
                  className="mt-1"
                />
              ) : null}
            </SheetTool>
          );
        })}
      </ul>
      {foldable ? (
        <button
          type="button"
          aria-expanded={unfolded}
          aria-controls={toolsId}
          onClick={() => setUnfoldedBatch(unfolded ? null : (batch?.id ?? null))}
          className="mt-1 flex items-center gap-1.5 rounded-button py-1 text-body text-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-accent"
        >
          <ChevronIcon size={14} className={`shrink-0 transition-transform ${unfolded ? "rotate-90" : ""}`} />
          {unfolded
            ? t("updates.fewerTools")
            : t("updates.moreTools", { count: items.length - FOLDED_TOOLS })}
        </button>
      ) : null}

      {phase === "planning" ? <SheetPending text={t("updates.preparing")} /> : null}

      {notesFirst ? null : notesSection}

      <CommandPreview
        plans={issued.map((item) => ({
          id: item.issued.id,
          name: several ? item.name : undefined,
          action: item.issued.plan.action,
        }))}
      />
    </Dialog>
  );
}
