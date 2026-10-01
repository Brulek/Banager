import { memo, useEffect, useId, useMemo, useRef, useState, type RefObject } from "react";
import { useTranslation } from "react-i18next";
import { usePlanOperation, useSettings, useSnapshot, useSubmitOperation } from "../lib/queries";
import { adapterIdOf, adapterLabel, instanceLabels, planErrorDetail, refusalSentence } from "../lib/sources";
import { modelPath } from "../lib/names";
import { warningLines, type WarningLine } from "../lib/warnings";
import { majorJump } from "../lib/versionJump";
import { artifactKeyId, useUiStore } from "../store/ui";
import type { ArtifactKey, IssuedPlan, OpRequest, UpdateCandidate } from "../lib/types";
import { CommandPreview } from "./CommandPreview";
import {
  Refusal,
  SheetIcon,
  SheetLines,
  SheetPending,
  SheetText,
  SheetTool,
  SheetToolList,
  sheetMeta,
  useToolsInTurn,
} from "./SheetParts";
import { CheckIcon } from "./icons";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";

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
 * in the dialog with its reason and is never submitted. A reason is kept
 * as the backend gave it and worded where it is drawn (`refusalOf`), so
 * that turning "Show technical details" on shows its words at once.
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
  /** Why its plan was refused, in the backend's words (`refusalSentence`). */
  planError: string | null;
  /** `planError`'s longer why, for its ⓘ (`planErrorDetail`), or null. */
  planErrorDetail: string | null;
  submittedOpId: number | null;
  /** Why its submit was refused, in the backend's words (`refusalSentence`). */
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

/** Why a tool of a batch did not start, as the screen says it (`refusalOf`). */
export interface ItemRefusal {
  text: string;
  /** Its longer why, for its ⓘ, or null. */
  detail: string | null;
}

export interface UpdateConfirmOptions {
  /** The name a row shows, which the confirmation names it by. */
  nameOf: (candidate: UpdateCandidate) => string;
  /** The list's own order, which the confirmation lists items in. */
  compare: (a: UpdateCandidate, b: UpdateCandidate) => number;
  /** The source's name, for a refusal's sentence (`refusalSentence`). */
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
   *
   * `onStarted` is where the focus goes instead once the sheet has closed
   * on a batch that all started: the button pressed is on its way out --
   * a row's Update gives way to its progress, Update all turns off with
   * nothing left to start -- and the focus would fall to the window's
   * body with it. The page puts it on the row, or on the list.
   */
  openConfirm(chosen: UpdateCandidate[], opener?: HTMLElement | null, onStarted?: VoidFunction): Promise<void>;
  /** For the dialog to call once it has closed (`Dialog`'s `onClosed`): `onStarted`, where the batch all started. */
  afterClose(): void;
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
  pageErrors: Array<BatchItem & { refusal: ItemRefusal }>;
  /** Why `item` did not start, as the screen says it, or null when it has not failed. */
  refusalOf(item: BatchItem): ItemRefusal | null;
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
  // Whether a refusal may quote the backend or another program, read as
  // it is drawn (`refusalOf`).
  const { data: settings } = useSettings();
  const technical = settings?.show_technical_details ?? false;
  const [batch, setBatch] = useState<Batch | null>(null);
  // Monotonic. The batch whose id equals this is the only one allowed to
  // write state; every async continuation checks `isCurrent` after `await`.
  const batchIdRef = useRef(0);
  // What opened the newest batch (`openConfirm`'s `opener`).
  const openerRef = useRef<HTMLElement | null>(null);
  // Where the focus goes once the newest batch has all started
  // (`openConfirm`'s `onStarted`), and -- set once it has -- what is left
  // for `afterClose` to do.
  const onStartedRef = useRef<VoidFunction | null>(null);
  const afterCloseRef = useRef<VoidFunction | null>(null);

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

  async function openConfirm(chosen: UpdateCandidate[], opener?: HTMLElement | null, onStarted?: VoidFunction) {
    // A new id retires whatever batch was still planning (`close` retires
    // one too). Planning has no side effect beyond issuing PlanIds that
    // expire on their own, so the newest batch wins and an older one's
    // late replies are dropped by `isCurrent`. Submitting is different —
    // see the lock in the dialog.
    const id = batchIdRef.current + 1;
    batchIdRef.current = id;
    openerRef.current =
      opener ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
    onStartedRef.current = onStarted ?? null;
    afterCloseRef.current = null;
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
    // each item keeps its own backend message verbatim, to be worded where
    // it is drawn (`refusalOf`). The notes and the commands arrive
    // together, once every plan has settled, and so does Update: nothing of
    // a batch can be confirmed before all of it is on the sheet.
    const results = await Promise.allSettled(
      candidates.map((c) => planMutation.mutateAsync(toRequest(c))),
    );
    if (!isCurrent(id)) return;

    const items = candidates.map((c, i): BatchItem => {
      const result = results[i];
      if (result.status === "fulfilled") return { ...blank(c), issued: result.value };
      const raw = errorMessage(result.reason);
      return { ...blank(c), planError: raw, planErrorDetail: planErrorDetail(t, raw) };
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
        // Worded like the planning failure above (`refusalOf`), for the
        // same reason: `submit` re-runs the actionability gate against
        // the current snapshot, so "that source stopped answering while
        // you were reading this" is a refusal this path can produce, and
        // it must not arrive as JSON or as a Rust enum.
        const raw = errorMessage(e);
        items[i] = { ...item, submitError: raw, submitErrorDetail: planErrorDetail(t, raw) };
      }
      if (!isCurrent(id)) return;
      setBatch({ id, phase: "submitting", items: [...items] });
    }
    if (!isCurrent(id)) return;

    const anyFailed = items.some((item) => item.planError !== null || item.submitError !== null);
    // Only a clean sweep closes the dialog; otherwise it stays open and
    // says, per item, what started and what did not, and why.
    if (!anyFailed) afterCloseRef.current = onStartedRef.current;
    setBatch(anyFailed ? { id, phase: "done", items } : null);
  }

  function afterClose() {
    const then = afterCloseRef.current;
    afterCloseRef.current = null;
    then?.();
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
  // Why an item did not start, in the source's name (`sourceLabelFor`):
  // its refusal in 「无法准备此次更新：…」 or 「无法开始更新：…」, without
  // the backend's own words unless "Show technical details" is on
  // (`refusalSentence`).
  function refusalOf(item: BatchItem): ItemRefusal | null {
    const source = sourceLabelFor(item.candidate.key.instance_id);
    if (item.planError !== null) {
      return {
        text: refusalSentence(t, "updates.planFailed", item.planError, source, technical),
        detail: item.planErrorDetail,
      };
    }
    if (item.submitError !== null) {
      return {
        text: refusalSentence(t, "updates.submitFailed", item.submitError, source, technical),
        detail: item.submitErrorDetail,
      };
    }
    return null;
  }

  // Every plan failed: there is nothing to confirm, so the reasons go on the
  // page rather than into an empty dialog. Cleared by the next batch.
  const pageErrors =
    batch !== null && batch.phase === "done" && !hasIssuedPlan(batch)
      ? batch.items.flatMap((item) => {
          const refusal = refusalOf(item);
          return refusal === null ? [] : [{ ...item, refusal }];
        })
      : [];

  return {
    openConfirm,
    afterClose,
    returnFocusTo: openerRef,
    dialogOpen,
    pageErrors,
    refusalOf,
    batch,
    submitting,
    confirmAndSubmit,
    close,
  };
}

export interface UpdateConfirmDialogProps {
  confirm: UpdateConfirm;
}

/** The window's words (`useTranslation`'s `t`), for what is drawn outside a component of its own. */
type Translate = (key: string, options?: Record<string, unknown>) => string;

/**
 * The version jump for the confirmation, or null when there is no honest
 * one to show: a `Digest` candidate says a new version of the model is
 * available, never two digests -- they are from different hash spaces
 * (crates/banager-core/src/adapters/ollama/mod.rs) -- and nothing,
 * rather than a dangling arrow, when a source could name only one side.
 * Not behind "Show technical details": spec §6 asks this screen to show
 * the version jump, and a confirmation that names the command but not
 * the change is not a confirmation.
 */
function versionJump(t: Translate, candidate: UpdateCandidate): string | null {
  if (candidate.channel === "Digest") return t("updates.newBuild");
  if (candidate.current === "" || candidate.target === "") return null;
  return t("updates.versionChange", { current: candidate.current, target: candidate.target });
}

/**
 * What a tool's row -- or the dialog, for one tool -- says under its name:
 * why it did not start (`refusalOf`), that it did, and its notes.
 */
function aboutTool(
  t: Translate,
  item: BatchItem,
  refusal: ItemRefusal | null,
  notes: WarningLine[],
  size: "body" | "small",
) {
  return (
    <>
      {refusal !== null ? (
        <Refusal text={refusal.text} detail={refusal.detail} detailTitle={refusal.text} size={size} />
      ) : null}
      {item.submittedOpId !== null ? (
        <p className="flex items-center gap-1 text-small text-foreground">
          <CheckIcon size={12} className="shrink-0 text-success" />
          {t("updates.started")}
        </p>
      ) : null}
      {notes.length > 0 ? <SheetLines lines={notes} /> : null}
    </>
  );
}

interface BatchToolProps {
  t: Translate;
  item: BatchItem;
  /** Why it did not start, as the dialog says it (`refusalOf`), or null. */
  refusal: ItemRefusal | null;
  /** What to know about it before going on (the dialog's `notesOf`). */
  notes: WarningLine[];
  adapterId: string;
  sourceLabel: string;
  showSource: boolean;
}

/**
 * One tool of the dialog's list (`SheetTool`): its avatar, name and the
 * version it moves to, and under its name its notes, what became of it and
 * why not (`aboutTool`). Drawn again only when one of those changes
 * (`sameBatchTool`): the dialog is drawn again with its page -- as its
 * plans come back, and at each update Update all starts, a hundred and
 * more of them -- and every tool on it, each time, was most of that.
 */
const BatchTool = memo(function BatchTool({
  t,
  item,
  refusal,
  notes,
  adapterId,
  sourceLabel,
  showSource,
}: BatchToolProps) {
  const jump = versionJump(t, item.candidate);
  const digest = item.candidate.channel === "Digest";
  return (
    <SheetTool
      adapterId={adapterId}
      sourceLabel={sourceLabel}
      showSource={showSource}
      iconKey={item.candidate.key}
      name={item.name}
      // A model pulled by a path by its last segment, as its row names it,
      // so its tag is not what is cut short.
      shownName={modelPath(item.candidate.key, item.name)?.name}
      // A model's "new version" is a sentence, not a number: under the name.
      aside={digest ? null : jump}
    >
      {digest && jump !== null ? <p className="text-small text-muted">{jump}</p> : null}
      {aboutTool(t, item, refusal, notes, "small")}
    </SheetTool>
  );
}, sameBatchTool);

/**
 * Whether a tool of the list shows the same as it did (`BatchTool`): each
 * batch's item is a new object at every step, the plan it waited for or
 * the update it started, while what the tool shows of it rarely changes.
 */
function sameBatchTool(was: BatchToolProps, now: BatchToolProps): boolean {
  const a = was.item;
  const b = now.item;
  return (
    was.t === now.t &&
    was.adapterId === now.adapterId &&
    was.sourceLabel === now.sourceLabel &&
    was.showSource === now.showSource &&
    a.candidate === b.candidate &&
    a.name === b.name &&
    a.submittedOpId === b.submittedOpId &&
    was.refusal?.text === now.refusal?.text &&
    was.refusal?.detail === now.refusal?.detail &&
    was.notes.length === now.notes.length &&
    was.notes.every(
      (line, index) =>
        line.text === now.notes[index].text &&
        line.detail === now.notes[index].detail &&
        line.caution === now.notes[index].caution,
    )
  );
}

/**
 * The confirmation `useUpdateConfirm` drives, as a dialog in the manner of
 * a macOS alert (spec §3.6, R6): 「要更新10个工具吗？」 -- 「要更新“git”吗？」
 * for one -- then Cancel and Update, Update the default button.
 *
 * About one tool, it is an alert's layout: the tool's 48 icon over the
 * question, its source and the version it moves to under it, and what
 * there is to know before going on under that, a line each. About several,
 * the tools are a grouped list -- 24 icons, names, the version each moves
 * to -- as high as 320 and scrolling inside past that, with each tool's
 * notes under its own name: its warnings, that it cannot be stopped once
 * it starts, that it may ask for the Mac's password. A batch can mix a
 * rustup self update with Homebrew upgrades, and Casks with formulae, so
 * each note stays with the tool it is true of. The tools with something
 * to say come first -- a refusal before a note -- so that the first of it
 * is in sight without scrolling; the rest keep the list's order. A
 * caution has a ⚠︎ before its words; a line's longer why is behind its ⓘ.
 * The commands are one click away (`CommandPreview`), each under its
 * tool's name, open from the start with technical details on.
 *
 * It is up the moment Update is pressed, with the tools and versions the
 * rows had and 「正在准备…」 where the notes will go -- the uninstall
 * dialog's 「正在检查影响…」, in the same look (`SheetPending`) -- and
 * Update off, with the dialog itself holding the focus, until every plan
 * is back. Then the notes and the commands, Update on, and the focus on
 * it, unless the user has put it somewhere else meanwhile. Once
 * submitted, what started and what did not, under each tool.
 */
export function UpdateConfirmDialog({ confirm }: UpdateConfirmDialogProps) {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { batch, dialogOpen, submitting } = confirm;
  const updateRef = useRef<HTMLButtonElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const items = batch?.items ?? [];
  const issued = items.filter(
    (item): item is BatchItem & { issued: IssuedPlan } => item.issued !== null,
  );
  // More than one tool on the dialog: a list, and each note and command
  // says whose it is.
  const several = items.length > 1;

  // The avatar and the source's name go by its adapter: its instance's,
  // or -- for an instance the snapshot has lost -- the one the id names.
  const adapterFor = (instanceId: string): string =>
    snapshot?.instances.find((instance) => instance.id === instanceId)?.adapter_id ??
    adapterIdOf(instanceId);
  // The source's name as the sidebar gives it -- 「Homebrew（Intel）」 where
  // this Mac has two (`instanceLabels`) -- or its kind's, for an instance
  // the snapshot has lost.
  const labels = useMemo(() => instanceLabels(t, snapshot?.instances ?? []), [t, snapshot]);
  const sourceLabelOf = (instanceId: string): string =>
    labels.get(instanceId) ?? adapterLabel(t, adapterFor(instanceId));

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

  const notesOf = (item: BatchItem): WarningLine[] => {
    if (item.issued === null) return [];
    const { plan } = item.issued;
    const lines = warningLines(t, plan.warnings);
    return [
      // The row's 「大版本更新」, said again where the update is confirmed.
      ...(majorJump(item.candidate) !== null ? [{ text: t("clarity.majorNote"), detail: null, caution: false }] : []),
      ...lines.trash,
      ...lines.keep,
      ...lines.note,
      // Once Running, `OperationBar` offers no Cancel for a NoCancel
      // operation (`OperationManager::cancel`, crates/banager-core/src/ops).
      ...(plan.cancel_policy === "NoCancel"
        ? [{ text: t("operations.noCancelHint"), detail: t("operations.noCancelHintDetail"), caution: true }]
        : []),
      // The brew adapter marks every Cask, though not every app then asks
      // (the copy table's T3). Spec §6: a password is never a surprise.
      ...(plan.needs_password ? [{ text: t("commandPreview.needsPassword"), detail: null, caution: false }] : []),
    ];
  };

  // While it prepares, it asks about everything chosen; then about what
  // can be confirmed of it.
  const asked = phase === "planning" ? items : issued;
  const title =
    asked.length === 1
      ? t("updates.confirmTitleNamed", { name: asked[0].name })
      : t("updates.confirmTitle", { count: asked.length });

  // What there is to say about each tool, under its name: its notes, what
  // became of it once submitted, and why not.
  const said = items.map((item) => ({ item, notes: notesOf(item) }));
  // The tools with a refusal first, then those with notes, then the rest,
  // each in the list's order (`Array.prototype.sort` is stable).
  const rank = ({ item, notes }: { item: BatchItem; notes: WarningLine[] }) =>
    item.planError !== null || item.submitError !== null ? 0 : notes.length > 0 ? 1 : 2;
  const ordered = several ? [...said].sort((a, b) => rank(a) - rank(b)) : said;
  // The first few drawn with the dialog, the rest just after (`useToolsInTurn`).
  const drawn = useToolsInTurn(ordered.length, batch?.id ?? null);
  // A name the list has from two sources says which is which (spec R3).
  const names = items.map((item) => item.name);
  const twice = new Set(names.filter((name, index) => names.indexOf(name) !== index));

  const only = items.length === 1 ? items[0] : null;
  const onlyAdapter = only === null ? null : adapterFor(only.candidate.key.instance_id);
  const onlyJump = only === null ? null : versionJump(t, only.candidate);
  const onlyDigest = only?.candidate.channel === "Digest";
  // What there is to know about the one tool, under its question: with its
  // subtitle, what describes the dialog as it opens.
  const aboutId = useId();

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
      width={several ? "several" : "one"}
      icon={
        only !== null && onlyAdapter !== null ? (
          <SheetIcon
            adapterId={onlyAdapter}
            sourceLabel={sourceLabelOf(only.candidate.key.instance_id)}
            iconKey={only.candidate.key}
          />
        ) : undefined
      }
      // One tool: where it comes from and the version it moves to, under
      // the question. A model's "new version" is a sentence: the text.
      subtitle={
        only !== null && onlyAdapter !== null
          ? sheetMeta(only.name, sourceLabelOf(only.candidate.key.instance_id), onlyDigest ? null : onlyJump)
          : undefined
      }
      describedBy={only !== null ? aboutId : undefined}
      initialFocus={batch?.phase === "done" ? closeRef : updateRef}
      returnFocusTo={confirm.returnFocusTo}
      onClosed={confirm.afterClose}
      footer={
        batch?.phase === "done" ? (
          // What did and did not start, said: the one thing left is to close it.
          <button ref={closeRef} type="button" onClick={confirm.close} className={BUTTON.large.default}>
            {t("common.close")}
          </button>
        ) : (
          <>
            <button type="button" onClick={confirm.close} disabled={submitting} className={BUTTON.large.grey}>
              {t("common.cancel")}
            </button>
            <button
              ref={updateRef}
              type="button"
              onClick={confirm.confirmAndSubmit}
              disabled={batch?.phase !== "ready"}
              className={BUTTON.large.default}
            >
              {t("updates.update")}
            </button>
          </>
        )
      }
    >
      {only !== null ? (
        <div id={aboutId} data-sheet-about="" className="flex flex-col gap-2">
          {onlyDigest && onlyJump !== null ? <SheetText>{onlyJump}</SheetText> : null}
          {aboutTool(t, only, confirm.refusalOf(only), said[0].notes, "body")}
        </div>
      ) : (
        <SheetToolList>
          {ordered.slice(0, drawn).map(({ item, notes }) => {
            const { key } = item.candidate;
            return (
              <BatchTool
                key={artifactKeyId(key)}
                t={t}
                item={item}
                refusal={confirm.refusalOf(item)}
                notes={notes}
                adapterId={adapterFor(key.instance_id)}
                sourceLabel={sourceLabelOf(key.instance_id)}
                showSource={twice.has(item.name)}
              />
            );
          })}
        </SheetToolList>
      )}

      {phase === "planning" ? <SheetPending text={t("updates.preparing")} /> : null}

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
