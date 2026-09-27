import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  useSnapshot,
  useSettings,
  useSaveSettings,
  usePlanOperation,
  useSubmitOperation,
  useOperations,
} from "../lib/queries";
import { useUiStore, artifactKeyId } from "../store/ui";
import {
  ADAPTER_LABEL_KEYS,
  artifactBlurb,
  planErrorMessage,
  READ_ONLY_DETAIL_KEYS,
  settingsSaveErrorMessage,
  sourceNoticesFor,
  UNAVAILABLE_DETAIL_KEYS,
  UPDATE_BLOCKED_KEYS,
} from "../lib/sources";
import { warningMessage, warningText, warningTexts } from "../lib/warnings";
import { ToolRow } from "../components/ToolRow";
import { StatusChip } from "../components/StatusChip";
import { Menu, type MenuItem } from "../components/ui/Menu";
import { SourceNotices } from "../components/SourceNotices";
import { CommandPreview } from "../components/CommandPreview";
import { COMMAND_SLOT, withCommand } from "../components/withCommand";
import { Dialog } from "../components/ui/Dialog";
import {
  CheckCircleIcon,
  CheckIcon,
  ChevronIcon,
  InfoIcon,
  SpinnerIcon,
} from "../components/icons";
import type {
  ArtifactKey,
  InstalledArtifact,
  InstanceNote,
  IssuedPlan,
  ManagerInstance,
  OpRequest,
  OpSummary,
  Outcome,
  Settings,
  UpdateBlocked,
  UpdateCandidate,
} from "../lib/types";
import {
  actionableUpdatesOf,
  canSkipVersion,
  everySourceChecked,
  notHidden,
  updateStateOf,
  withSkippedVersion,
} from "../lib/updateState";
import type { UpdateState } from "../lib/updateState";

// The virtualizer's first guesses: a row, the "Can't update here" toggle
// and the line under it. Each slot then measures itself through
// `measureElement`.
const ROW_ESTIMATE = 60;
const SECTION_ESTIMATE = 48;
const SUMMARY_ESTIMATE = 36;

/**
 * One slot in the virtualized list. The page is one flat list, sorted by
 * name, the way 360's update list is: every row it can update, then the
 * toggle for the rows it cannot ("Can't update here (5)"), folded until
 * pressed. Each row carries its source -- the avatar's colour and a small
 * chip -- in place of the per-source headings the list used to be grouped
 * under.
 */
type ListItem =
  | { type: "update"; candidate: UpdateCandidate }
  | { type: "section"; count: number; expanded: boolean }
  | { type: "summary"; count: number };

/**
 * A slot's identity: its React key, and the key the virtualizer files the
 * slot's measured height under -- the same string, so a height stays with
 * the row or the toggle it was measured from. Left to its default, the
 * virtualizer keys by position: when an update finished and its row went,
 * every slot below moved up while its React key kept its DOM node, and
 * the heights stayed where they were -- the toggle, now where a row had
 * been, was placed as if it were that row. A moved node is not measured
 * again (its ref does not change, and a ResizeObserver sees no resize), so
 * nothing corrected it. An artifact key id has a `|` in it and the other
 * two do not, so they cannot collide.
 */
function listItemKey(item: ListItem): string {
  switch (item.type) {
    case "update":
      return artifactKeyId(item.candidate.key);
    case "section":
      return "section:cant-update-here";
    case "summary":
      return "summary:cannot-check";
  }
}

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
interface BatchItem {
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
interface Batch {
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

/**
 * Whether each note says that typing the tool's name in Terminal may not
 * run this instance's copy: this copy is not on the PATH Canager sees, so
 * the name finds nothing there or another program with that name
 * (`NotOnPath`); another program with that name is found there before this
 * copy (`ShadowedBy*`); or the launcher's program files are gone
 * (`LauncherOnly`). Read by `saysItUpdatesItself`: a tool that updates
 * itself does so when it runs, and "it usually updates itself" is true of
 * this copy only where typing its name runs it. A `Record`, so a note
 * added to `InstanceNote` without an answer here fails `tsc`.
 */
const NAME_MAY_NOT_RUN_THIS_COPY: Record<InstanceNote, boolean> = {
  // Homebrew's: about its list of software, not about which copy runs.
  IndexMayBeStale: false,
  IndexUpdating: false,
  NotOnPath: true,
  ShadowedByHomebrew: true,
  ShadowedByNpm: true,
  ShadowedByOther: true,
  LauncherOnly: true,
};

/**
 * What a row shows in place of its Update button while an update of it is
 * under way or has just finished (`UpdateProgress`). `failed` and `check`
 * keep the operation's id, for the log they offer.
 */
type RowProgress =
  | { kind: "queued" }
  | { kind: "running" }
  | { kind: "cancelling" }
  | { kind: "succeeded" }
  | { kind: "cancelled" }
  | { kind: "failed"; opId: number }
  | { kind: "check"; opId: number };

/**
 * How a finished update ended, for its row. "Check" -- look at the log --
 * for every outcome that is neither a plain success nor a plain failure:
 * the tool said it worked and Canager could not confirm it (`Unconfirmed`)
 * or found the opposite (`NeedsAttention`). A finished operation with no
 * outcome, which the backend never sends, claims nothing either way.
 */
function outcomeProgress(outcome: Outcome | null, opId: number): RowProgress {
  if (outcome === null) return { kind: "check", opId };
  if (typeof outcome === "string") {
    switch (outcome) {
      case "Succeeded":
        return { kind: "succeeded" };
      case "Cancelled":
        return { kind: "cancelled" };
      case "Unconfirmed":
        return { kind: "check", opId };
      default: {
        const unhandled: never = outcome;
        return unhandled;
      }
    }
  }
  if ("NeedsAttention" in outcome) return { kind: "check", opId };
  if ("Failed" in outcome || "CanagerFailed" in outcome) return { kind: "failed", opId };
  const unhandled: never = outcome;
  return unhandled;
}

/** Where an update stands, from its operation. A `switch` with no default, so a new status fails `tsc`. */
function progressOf(op: OpSummary): RowProgress {
  switch (op.status) {
    case "Queued":
      return { kind: "queued" };
    // Verifying is the update's own last step: the command has ended and
    // Canager is reading the result back.
    case "Running":
    case "Verifying":
      return { kind: "running" };
    case "CancelRequested":
    case "Cancelling":
      return { kind: "cancelling" };
    case "Done":
      return outcomeProgress(op.outcome, op.id);
  }
}

interface UpdateProgressProps {
  progress: RowProgress;
  /** The row's name, for "View log"'s accessible name. */
  name: string;
  onViewLog: (opId: number) => void;
}

/**
 * The row's own progress, where its Update button was: 360's "the progress
 * is in the row". Waiting, updating with a spinner, a tick when it is
 * done; a failure, or an outcome to check, with the way to its log.
 */
function UpdateProgress({ progress, name, onViewLog }: UpdateProgressProps) {
  const { t } = useTranslation();
  const viewLog = (opId: number) => (
    <button
      type="button"
      onClick={() => onViewLog(opId)}
      aria-label={t("updates.progress.viewLogLabel", { name })}
      className="rounded-sm text-small font-medium text-accent-text outline-none hover:underline focus-visible:ring-2 focus-visible:ring-accent"
    >
      {t("updates.progress.viewLog")}
    </button>
  );
  switch (progress.kind) {
    case "queued":
      return <span className="text-small text-muted">{t("updates.progress.queued")}</span>;
    case "running":
      return (
        <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-small font-medium text-accent-text">
          <SpinnerIcon size={14} />
          {t("updates.progress.running")}
        </span>
      );
    case "cancelling":
      return (
        <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-small text-muted">
          <SpinnerIcon size={14} />
          {t("updates.progress.cancelling")}
        </span>
      );
    case "succeeded":
      return (
        <span className="inline-flex items-center gap-1 whitespace-nowrap text-small font-medium text-success">
          <CheckIcon size={15} />
          {t("updates.progress.succeeded")}
        </span>
      );
    case "cancelled":
      return <span className="text-small text-muted">{t("updates.progress.cancelled")}</span>;
    case "failed":
      return (
        <span className="flex flex-col items-end leading-tight">
          <span className="text-small font-medium text-danger">{t("updates.progress.failed")}</span>
          {viewLog(progress.opId)}
        </span>
      );
    case "check":
      return (
        <span className="flex flex-col items-end leading-tight">
          <span className="text-small font-medium text-warning">{t("updates.progress.check")}</span>
          {viewLog(progress.opId)}
        </span>
      );
  }
}

/** A chip's detail, a sentence to a line; the lines after the first are the quieter kind. */
function detailLines(lines: ReactNode[]): ReactNode {
  return lines.map((line, index) => (
    <p key={index} className={index === 0 ? "break-words" : "mt-1.5 break-words text-muted"}>
      {line}
    </p>
  ));
}

const HEADER_TEXT_BUTTON =
  "rounded-button px-2.5 py-1.5 text-body font-medium text-accent-text outline-none transition-colors hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-40 disabled:hover:bg-transparent";

export function UpdatesPage() {
  const { t, i18n } = useTranslation();
  const { data: snapshot, isLoading } = useSnapshot();
  const { data: settings } = useSettings();
  const { data: operations } = useOperations();
  const saveSettings = useSaveSettings();
  // Used only for their promise-returning `mutateAsync` — which keeps
  // `useSubmitOperation`'s operations-query invalidation — never for their
  // `isPending`/`isError`/`error`; every flag the UI needs comes from `batch`.
  const planMutation = usePlanOperation();
  const submitMutation = useSubmitOperation();
  const selectedUpdates = useUiStore((s) => s.selectedUpdates);
  const toggleUpdate = useUiStore((s) => s.toggleUpdate);
  const selectUpdates = useUiStore((s) => s.selectUpdates);
  const invertUpdateSelection = useUiStore((s) => s.invertUpdateSelection);
  const updateTargets = useUiStore((s) => s.updateTargets);
  const setFocusedOpId = useUiStore((s) => s.setFocusedOpId);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);

  const listRef = useRef<HTMLDivElement>(null);
  const [batch, setBatch] = useState<Batch | null>(null);
  // Monotonic. The batch whose id equals this is the only one allowed to
  // write state; every async continuation checks `isCurrent` after `await`.
  const batchIdRef = useRef(0);
  // "Can't update here (N)": folded until pressed.
  const [showCantUpdate, setShowCantUpdate] = useState(false);
  // What the last "Copy command" did, said for a moment in the header.
  const [copyStatus, setCopyStatus] = useState<"copied" | "failed" | null>(null);
  const copyTimerRef = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(copyTimerRef.current), []);

  // Every update the user has not hidden, with "Never remind me" or "Skip
  // this version": `notHidden`, the rule in src/lib/updateState.ts that
  // the Installed page's badge reads too. Everything below that lists,
  // counts or selects a row starts from this list.
  const visibleUpdates = useMemo(() => {
    if (!snapshot || !settings) return [];
    return notHidden(snapshot.updates, settings);
  }, [snapshot, settings]);

  // A candidate carries no adapter of its own; the only route from an
  // UpdateCandidate to the source that produced it is its key's
  // `instance_id`, joined back to the snapshot's instances.
  const instancesById = useMemo(() => {
    const byId = new Map<string, ManagerInstance>();
    for (const instance of snapshot?.instances ?? []) byId.set(instance.id, instance);
    return byId;
  }, [snapshot]);

  // One lookup table instead of a `snapshot.artifacts.find` per row: that
  // scan made every render O(updates × artifacts).
  const artifactsById = useMemo(() => {
    const byId = new Map<string, InstalledArtifact>();
    for (const artifact of snapshot?.artifacts ?? []) {
      byId.set(artifactKeyId(artifact.key), artifact);
    }
    return byId;
  }, [snapshot]);

  // The source's name in the user's language: the chip beside a row's
  // name, the `{{source}}` in its detail, and the one refusal that can
  // reach a real person verbatim otherwise (`planErrorMessage`'s
  // NotActionable case). A stale snapshot's own read-only/unavailable
  // state cannot be trusted for *which* reason applies -- that is exactly
  // what went stale -- but the instance's adapter, and therefore its
  // label, does not change underneath it, so this is safe to read from the
  // same snapshot.
  const sourceLabelFor = useCallback(
    (instanceId: string): string => {
      const instance = instancesById.get(instanceId);
      if (!instance) return instanceId;
      const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
      return labelKey ? t(labelKey) : instance.adapter_id;
    },
    [instancesById, t],
  );

  // The name a row shows: the one the Installed page shows for the same
  // software ("Microsoft Visual Studio Code", "Claude Code"), or the
  // package's own name where the snapshot has no entry for it.
  const nameOf = useCallback(
    (candidate: UpdateCandidate): string =>
      artifactsById.get(artifactKeyId(candidate.key))?.display_name || candidate.key.name,
    [artifactsById],
  );

  // By name, as the user reads it: case and accents aside, and "node@22"
  // after "node@9". The key breaks a tie between two sources' same-named
  // packages, so the order never depends on the snapshot's.
  const compareRows = useMemo(() => {
    const collator = new Intl.Collator(i18n.language, { numeric: true, sensitivity: "base" });
    return (a: UpdateCandidate, b: UpdateCandidate) =>
      collator.compare(nameOf(a), nameOf(b)) ||
      collator.compare(artifactKeyId(a.key), artifactKeyId(b.key));
  }, [i18n.language, nameOf]);

  const stateOf = (candidate: UpdateCandidate): UpdateState =>
    updateStateOf(candidate, instancesById.get(candidate.key.instance_id));

  // The rows Canager can update from here: every listed update whose row
  // has an Update button and a checkbox. `actionableUpdatesOf` is
  // `visibleUpdates` filtered by `isUpdateActionable` -- read-only source,
  // could not be checked, blocked, source not answering: `updateStateOf`
  // in src/lib/updateState.ts, which the Installed page's badge reads too,
  // so the two pages cannot disagree about whether a package can be
  // updated -- and it is kept there because the sidebar's count on this
  // page's entry and the Overview's are this list's length, which must
  // never disagree with the page. `Session::issue_plan` applies the same
  // conditions in Rust (spec §2.5 for the source, `blocked_upgrade` in
  // crates/canager-core/src/session/plans.rs for the package), so a stale
  // snapshot costs an error message, not a wrong command.
  //
  // It is also every row that shows a checkbox, and what Select all,
  // Invert selection and Update all hand to the store, so none of them can
  // tick a row the user could not tick by hand.
  const actionableUpdates = useMemo(
    () => (snapshot && settings ? actionableUpdatesOf(snapshot, settings) : []),
    [snapshot, settings],
  );

  // The list's two parts, each by name: the rows with an Update button,
  // and everything else listed -- pinned, read-only, could not be checked,
  // updating itself, its source not answering -- under "Can't update
  // here". Two numbers, never one: folding the second into the first
  // would promise buttons that are not there, and leaving it out would
  // call six listed pip packages "0 updates".
  const { actionableRows, otherRows } = useMemo(() => {
    const actionableIds = new Set(actionableUpdates.map((u) => artifactKeyId(u.key)));
    return {
      actionableRows: [...actionableUpdates].sort(compareRows),
      otherRows: visibleUpdates
        .filter((u) => !actionableIds.has(artifactKeyId(u.key)))
        .sort(compareRows),
    };
  }, [actionableUpdates, visibleUpdates, compareRows]);

  // Only rows that are selected, still visible *and* still actionable
  // count. The store keeps a selection for a row that has since been
  // hidden; without this intersection "Update selected" would be enabled
  // for nothing and open an empty dialog. Actionability is in the same
  // intersection because a selection outlives the row that made it: a
  // candidate selected while it was actionable stays selected after a
  // refresh takes that away (a pin, a failed lookup, a source that stopped
  // answering), and the batch would then plan the very row whose Update
  // button has just gone.
  const selectedVisible = useMemo(
    () => actionableRows.filter((u) => selectedUpdates.includes(artifactKeyId(u.key))),
    [actionableRows, selectedUpdates],
  );

  // What each source has to say about this check, one compact line each
  // at the top of the page: not running, not answering, a list it could
  // not download, another copy that runs when its name is typed. What a
  // source lets Canager do at all -- pip being read-only -- is not a line
  // here: every row of such a source says it with its own "Read-only"
  // chip. How many rows a source has is part of what its notice says: a
  // silent source's "what's listed here is last time's" is true only over
  // rows it actually has.
  //
  // Iterates `snapshot.instances`, which is every source any candidate can
  // come from: `refresh` builds `updates` only from instances it also puts
  // in `instances` (crates/canager-core/src/session/refresh.rs).
  const notices = useMemo(() => {
    const rowsByInstance = new Map<string, number>();
    for (const update of visibleUpdates) {
      const id = update.key.instance_id;
      rowsByInstance.set(id, (rowsByInstance.get(id) ?? 0) + 1);
    }
    return (snapshot?.instances ?? []).flatMap((instance) =>
      sourceNoticesFor(
        instance,
        sourceLabelFor(instance.id),
        rowsByInstance.get(instance.id) ?? 0,
      ).filter((notice) => notice.axis === "state"),
    );
  }, [snapshot, visibleUpdates, sourceLabelFor]);

  // How many rows can only say that Canager could not check them, the
  // tool's own words being hidden while "Show technical details" is off:
  // an uncheckable row with a `Message`. The page says once, over those
  // rows, where to see why, counting these rows and no others -- a
  // `NonRegistrySource` row already says its own reason. It claims no
  // diagnosis: the first line of the tool's stderr is the only thing that
  // tells "this Mac is offline" from "that index is refusing you"
  // (`lookup_failure_reason`, crates/canager-core/src/adapters/mod.rs),
  // and it is precisely what is hidden.
  const hiddenReasonCount = settings?.show_technical_details
    ? 0
    : otherRows.filter(
        (candidate) =>
          !candidate.checkable &&
          candidate.warnings.some((warning) => warningMessage(warning) !== null),
      ).length;

  const items = useMemo<ListItem[]>(
    () => [
      ...actionableRows.map((candidate): ListItem => ({ type: "update", candidate })),
      ...(otherRows.length > 0
        ? [{ type: "section", count: otherRows.length, expanded: showCantUpdate } as const]
        : []),
      ...(showCantUpdate && hiddenReasonCount > 0
        ? [{ type: "summary", count: hiddenReasonCount } as const]
        : []),
      ...(showCantUpdate
        ? otherRows.map((candidate): ListItem => ({ type: "update", candidate }))
        : []),
    ],
    [actionableRows, otherRows, showCantUpdate, hiddenReasonCount],
  );

  // The newest update operation of each package. The backend keeps a
  // finished operation in its list, so an operation is matched to a row
  // by its key -- instance, kind and name -- and, once it has finished, by
  // the version the row offers too (`operationFor`).
  const latestUpdateOp = useMemo(() => {
    const byKey = new Map<string, OpSummary>();
    for (const op of operations ?? []) {
      if (op.kind !== "Upgrade") continue;
      const id = artifactKeyId({ instance_id: op.instance_id, kind: op.artifact_kind, name: op.name });
      const seen = byKey.get(id);
      if (seen === undefined || op.id > seen.id) byKey.set(id, op);
    }
    return byKey;
  }, [operations]);

  /**
   * The operation a row shows in place of its Update button, or null.
   * One still under way, always: a second click could only queue the same
   * update behind it. A finished one only while the row still offers the
   * version it was started for (`updateTargets`, remembered when this page
   * submitted it): "Updated" or "Failed" is about that version, and a
   * newer one the source offers later gets its button back. An operation
   * this page has no record of -- one from before the window was reloaded
   * -- is not shown once it has finished.
   */
  const operationFor = (candidate: UpdateCandidate): OpSummary | null => {
    const op = latestUpdateOp.get(artifactKeyId(candidate.key));
    if (op === undefined) return null;
    if (op.status !== "Done") return op;
    return updateTargets[op.id] === candidate.target ? op : null;
  };

  const viewLog = (opId: number) => {
    setFocusedOpId(opId);
    setDrawerOpen(true);
  };

  /**
   * Whether the row may say its tool usually updates itself: an actionable
   * row of a tool with its own installer that updates itself in the
   * background (`auto_updates`, set by the standalone adapter from its
   * recipe; spec D5). The row is real -- it compares the launcher's live
   * version with the published one -- and keeps its button; the chip says
   * the tool usually does this itself. It does so when it runs, and typing
   * its name runs this copy only where no note says otherwise
   * (`NAME_MAY_NOT_RUN_THIS_COPY`; the notice at the top says why): there
   * the row is a plain one, behind and updatable. Only for the standalone
   * adapters: a self-updating Homebrew cask listed by --greedy is a plain
   * row too, since Homebrew, not the app, is what its button drives.
   */
  const saysItUpdatesItself = (
    candidate: UpdateCandidate,
    instance: ManagerInstance | undefined,
  ): boolean =>
    instance !== undefined &&
    instance.adapter_id.startsWith("standalone-") &&
    artifactsById.get(artifactKeyId(candidate.key))?.auto_updates === true &&
    !instance.status.notes.some((note) => NAME_MAY_NOT_RUN_THIS_COPY[note]);

  /**
   * Why a row could not be checked, for its "Can't check" chip: that it
   * could not, then its reason. A warning with a key of its own
   * (`NonRegistrySource`) was written for this audience and is always
   * given. A `Message` is raw text off the wire -- a tool's stderr, an
   * HTTP error -- kept verbatim on purpose, and it is behind "Show
   * technical details", which is exactly what spec §6 says the right
   * shape is. Distinct, so a row carrying the same reason twice says it
   * once.
   */
  const cannotCheckDetail = (candidate: UpdateCandidate): ReactNode => {
    const reasons = new Set<string>();
    for (const warning of candidate.warnings) {
      const raw = warningMessage(warning);
      const text = raw === null ? warningText(t, warning) : settings?.show_technical_details ? raw : null;
      if (text !== null && text !== "") reasons.add(text);
    }
    return detailLines([t("updates.cannotCheckShort"), ...reasons]);
  };

  // A blocked row's chip and its detail: why the tool will not update it,
  // and what the user can do instead -- the unpin command, set as code in
  // the sentence, or "open it once", with the command that opens it under
  // it while technical details are on.
  const blockedDetail = (
    candidate: UpdateCandidate,
    reason: UpdateBlocked,
    instance: ManagerInstance | undefined,
  ): ReactNode => {
    const copy = UPDATE_BLOCKED_KEYS[reason];
    const source = sourceLabelFor(candidate.key.instance_id);
    const command = copy.command(candidate.key, instance);
    if (copy.commandInDetail) {
      return detailLines([withCommand(t(copy.detail, { command: COMMAND_SLOT, source }), command)]);
    }
    return detailLines([
      t(copy.detail, { source }),
      ...(settings?.show_technical_details
        ? [withCommand(t("updates.runInTerminal", { command: COMMAND_SLOT }), command)]
        : []),
    ]);
  };

  /**
   * The row's status chips, one per `UpdateState`, each with its why
   * behind an ⓘ. A `switch` with no default, so a state added to
   * `UpdateState` without a chip here fails `tsc`. A read-only source's
   * row that could not be checked either says both: "Read-only" is the
   * fact that no button will ever appear on it, whatever the next check
   * finds, and that this check found nothing is its own news.
   */
  const statusChips = (
    candidate: UpdateCandidate,
    state: UpdateState,
    instance: ManagerInstance | undefined,
  ): ReactNode[] => {
    const cannotCheck = (
      <StatusChip key="cannot-check" label={t("updates.cannotCheck")} detail={cannotCheckDetail(candidate)} />
    );
    switch (state.kind) {
      case "actionable":
        return saysItUpdatesItself(candidate, instance)
          ? [
              <StatusChip
                key="updates-itself"
                label={t("updates.selfUpdating")}
                detail={detailLines([t("updates.selfUpdatingDetail")])}
              />,
            ]
          : [];
      case "readOnly": {
        const reason = instance?.read_only_reason ?? null;
        return [
          <StatusChip
            key="read-only"
            label={t("updates.readOnly")}
            detail={reason === null ? undefined : detailLines([t(READ_ONLY_DETAIL_KEYS[reason])])}
          />,
          ...(candidate.checkable ? [] : [cannotCheck]),
        ];
      }
      case "cannotCheck":
        return [cannotCheck];
      case "blocked":
        return [
          <StatusChip
            key="blocked"
            label={t(UPDATE_BLOCKED_KEYS[state.reason].badge)}
            detail={blockedDetail(candidate, state.reason, instance)}
          />,
        ];
      case "sourceUnavailable":
        // An instance missing from the snapshot, which `refresh` never
        // produces, reads as one that did not answer.
        return [
          <StatusChip
            key="unavailable"
            label={t("updates.sourceUnavailable")}
            detail={detailLines([
              t(UNAVAILABLE_DETAIL_KEYS[instance?.status.unavailable ?? "NotResponding"], {
                source: sourceLabelFor(candidate.key.instance_id),
              }),
            ])}
          />,
        ];
    }
  };

  /**
   * The version column: "7.1 → 7.2", in tabular numerals.
   *
   * A `Digest` candidate is Ollama: `current` is the local manifest digest
   * that /api/tags reported and `target` is the registry manifest's config
   * digest -- **different hash spaces**, not two readings of one
   * identifier, and they will not be equal even after a successful pull.
   * The adapter's own comment (crates/canager-core/src/adapters/ollama/
   * mod.rs) says never to render them as a version jump, and a 64-hex
   * string is not something to put in front of this audience either way:
   * such a row says "New version" -- only when it was checked. A row
   * Canager could not check has no version to move to (its `target` is
   * its installed version, `uncheckable_candidate` in crates/canager-core/
   * src/adapters/mod.rs), so it shows the version it has, and a model's
   * nothing at all.
   */
  const versionOf = (candidate: UpdateCandidate): string | null => {
    const { current, target } = candidate;
    if (!candidate.checkable) {
      return candidate.channel === "Digest" || current === "" ? null : current;
    }
    if (candidate.channel === "Digest") return t("updates.newVersion");
    if (current !== "" && target !== "") return t("updates.versionChange", { current, target });
    return target !== "" ? target : current !== "" ? current : null;
  };

  /**
   * The version jump for the confirmation dialog, or null when there is no
   * honest one to show: `versionOf`'s rule in the dialog's own words -- a
   * `Digest` candidate says a newer build of the model is available, never
   * two digests -- and nothing, rather than a dangling arrow, when a source
   * could name only one side. Not behind "Show technical details": spec §6
   * asks this screen to show the version jump, and a confirmation that
   * names the command but not the change is not a confirmation.
   */
  const dialogVersionJump = (candidate: UpdateCandidate): string | null => {
    if (candidate.channel === "Digest") return t("updates.newBuild");
    if (candidate.current === "" || candidate.target === "") return null;
    return t("updates.versionChange", { current: candidate.current, target: candidate.target });
  };

  function isCurrent(id: number): boolean {
    return batchIdRef.current === id;
  }

  function deselect(key: ArtifactKey) {
    // Read the store directly: this runs after an `await`, when the
    // `selectedUpdates` captured by this render may already be stale.
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
    const candidates = [...chosen].sort(compareRows);
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
        // outcome from a later version's (`operationFor`). Recorded
        // whether or not this batch is still current: the operation runs.
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

  // What a row's two hiding items share: `next` builds the settings to
  // save from the ones on screen.
  function hide(candidate: UpdateCandidate, next: (current: Settings) => Settings) {
    // One save at a time. A second choice while the first save is pending
    // would build its settings from the same stale base, and the later save
    // would overwrite the earlier one. The items are disabled meanwhile;
    // this guard covers a choice that was already on its way.
    if (!settings || saveSettings.isPending) return;
    if (selectedUpdates.includes(artifactKeyId(candidate.key))) {
      toggleUpdate(candidate.key);
    }
    saveSettings.mutate(next(settings));
  }

  // "Skip this version": hides this update until the source offers another
  // version (`withSkippedVersion`, `hidingRule`).
  const skipVersion = (candidate: UpdateCandidate) =>
    hide(candidate, (current) => ({
      ...current,
      skipped_versions: withSkippedVersion(current.skipped_versions, candidate),
    }));

  // "Never remind me": hides every update of this package, now and later.
  const neverRemind = (candidate: UpdateCandidate) =>
    hide(candidate, (current) => ({
      ...current,
      ignored_updates: [...current.ignored_updates, candidate.key],
    }));

  // "Copy command", and a word in the header about whether it worked:
  // the clipboard can refuse, and a menu item that did nothing must not
  // look as if it had.
  function copyCommand(command: string) {
    const say = (status: "copied" | "failed") => {
      setCopyStatus(status);
      window.clearTimeout(copyTimerRef.current);
      copyTimerRef.current = window.setTimeout(() => setCopyStatus(null), 2500);
    };
    if (navigator.clipboard === undefined) {
      say("failed");
      return;
    }
    navigator.clipboard.writeText(command).then(
      () => say("copied"),
      () => say("failed"),
    );
  }

  /**
   * The row's ⋯ menu: the two ways to stop seeing this update, the lighter
   * one first, and -- with technical details on -- the command its chip
   * talks about. "Skip this version" hides it until the source offers
   * another version; "Never remind me" hides every update of this package
   * until the user undoes it in Settings. Each item's hint -- a tooltip,
   * and its accessible description -- says what it does. A row whose
   * `target` does not name one release gets only "Never remind me"
   * (`canSkipVersion`): one Canager could not check, whose `target` is its
   * installed version, and a Homebrew cask declared `version :latest`,
   * every release of which is offered as "latest", so that a skip of it
   * would never end. "Copy command" only where the command is known
   * without asking the backend for a plan: a blocked row's unpin command
   * or launcher.
   */
  const menuItems = (
    candidate: UpdateCandidate,
    state: UpdateState,
    instance: ManagerInstance | undefined,
  ): MenuItem[] => {
    const items: MenuItem[] = [];
    if (canSkipVersion(candidate)) {
      items.push({
        id: "skip",
        label: t("updates.skipVersion"),
        hint: t("updates.skipVersionHint"),
        disabled: saveSettings.isPending,
        onSelect: () => skipVersion(candidate),
      });
    }
    items.push({
      id: "never",
      label: t("updates.neverRemind"),
      hint: t("updates.neverRemindHint"),
      disabled: saveSettings.isPending,
      onSelect: () => neverRemind(candidate),
    });
    if (settings?.show_technical_details && state.kind === "blocked") {
      const command = UPDATE_BLOCKED_KEYS[state.reason].command(candidate.key, instance);
      items.push({ id: "copy", label: t("updates.copyCommand"), onSelect: () => copyCommand(command) });
    }
    return items;
  };

  // Above every early return: hooks cannot be called conditionally, and
  // the returns below are reached before the list is drawn.
  //
  // `getItemKey` changes with `items`, which is what tells the virtualizer
  // to lay the list out again from its measured heights under the new keys.
  const getItemKey = useCallback((index: number) => listItemKey(items[index]), [items]);
  const rowVirtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => listRef.current,
    estimateSize: (index) => {
      const item = items[index];
      if (item?.type === "section") return SECTION_ESTIMATE;
      if (item?.type === "summary") return SUMMARY_ESTIMATE;
      return ROW_ESTIMATE;
    },
    getItemKey,
  });

  if (isLoading) {
    return <p className="p-4 text-sm text-[var(--color-muted)]">{t("common.loading")}</p>;
  }
  if (!snapshot || !settings) {
    return null;
  }

  const noticeLines =
    notices.length > 0 ? (
      <div className="flex flex-col gap-1.5 px-6 pb-3">
        <SourceNotices notices={notices} layout="line" />
      </div>
    ) : null;

  // Two different kinds of empty: the backend found no updates, or it found
  // some and the user has hidden every one (skipped the version it offers,
  // or asked never to be reminded about it). Only the first can mean the
  // machine is up to date -- and only when every source actually answered.
  //
  // The notices go *above* the sentence. "Everything is up to date" over a
  // stopped Ollama or a Homebrew whose catalogue could not be downloaded is
  // precisely the lie this page used to tell: no candidates is exactly
  // what an unreachable source produces, and the page read that silence as
  // good news. When a source did not answer, or carries a note that means
  // its updates were not fully checked (`NOTE_LEAVES_UPDATES_UNCHECKED`),
  // the sentence drops to what Canager can honestly claim -- nothing to
  // update *in the sources it managed to check*. Not for every notice: one
  // that is information only -- which copy runs when you type a tool's
  // name -- still goes above the sentence, and leaves the sentence alone.
  // A read-only source is one Canager *can* check. The rule is
  // `everySourceChecked` in src/lib/updateState.ts, which the Overview's
  // headline reads too: it may call the Mac up to date only when this page
  // would.
  if (visibleUpdates.length === 0) {
    const upToDate = snapshot.updates.length === 0 && everySourceChecked(snapshot.instances);
    const message =
      snapshot.updates.length > 0
        ? t("updates.allHidden")
        : upToDate
          ? t("updates.upToDate")
          : t("updates.noneCheckable");
    return (
      <div className="flex h-full flex-col">
        {noticeLines}
        <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 pb-10 text-center">
          {upToDate ? (
            <CheckCircleIcon size={44} className="text-success" />
          ) : (
            <InfoIcon size={36} className="text-muted" />
          )}
          <p
            className={
              upToDate ? "text-section text-foreground" : "max-w-sm text-body text-muted"
            }
          >
            {message}
          </p>
        </div>
      </div>
    );
  }

  const dialogOpen = batch !== null && batch.phase !== "planning" && hasIssuedPlan(batch);
  const submitting = batch?.phase === "submitting";
  // Every plan failed: there is nothing to confirm, so the reasons go on the
  // page rather than into an empty dialog. Cleared by the next batch.
  const pageErrors =
    batch !== null && batch.phase === "done" && !hasIssuedPlan(batch)
      ? batch.items.filter(hasPlanError)
      : [];

  // One row. Its checkbox and Update button come with an Update button's
  // state and no other (`actionable`); `checkable: false` in particular
  // must offer neither -- "Update" on a git-installed crate would run
  // `cargo install --force {name}` against the crates.io crate of the
  // same name, a different package. While an update of it is under way,
  // or has just finished, its progress stands where the button was.
  const updateRow = (candidate: UpdateCandidate) => {
    const instance = instancesById.get(candidate.key.instance_id);
    // Resolved once per row: the chips and the row's own actionability
    // must agree.
    const state = stateOf(candidate);
    const actionable = state.kind === "actionable";
    const name = nameOf(candidate);
    const source = sourceLabelFor(candidate.key.instance_id);
    const op = operationFor(candidate);
    const chips = statusChips(candidate, state, instance);
    return (
      <ToolRow
        adapterId={instance?.adapter_id ?? candidate.key.instance_id.split(":")[0]}
        sourceLabel={source}
        name={name}
        // A tool with its own installer is its own source: the chip would
        // only say its name again.
        nameChip={source === name ? undefined : source}
        description={artifactBlurb(
          t,
          artifactsById.get(artifactKeyId(candidate.key))?.description,
          instance?.adapter_id,
        )}
        selectable={
          actionable
            ? {
                checked: selectedUpdates.includes(artifactKeyId(candidate.key)),
                onToggle: () => toggleUpdate(candidate.key),
                ariaLabel: t("updates.selectRow", { name }),
              }
            : undefined
        }
        status={chips.length > 0 ? chips : undefined}
        version={versionOf(candidate)}
        action={
          op !== null ? (
            <UpdateProgress progress={progressOf(op)} name={name} onViewLog={viewLog} />
          ) : actionable ? (
            <button
              type="button"
              onClick={() => openConfirm([candidate])}
              disabled={dialogOpen}
              className="h-7 rounded-button bg-accent/10 px-3.5 text-body font-semibold text-accent-text outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50 disabled:hover:bg-accent/10 disabled:hover:text-accent-text"
            >
              {t("updates.update")}
            </button>
          ) : null
        }
        menu={
          <Menu
            label={t("updates.moreActions", { name })}
            items={menuItems(candidate, state, instance)}
          />
        }
      />
    );
  };

  const actionableCount = actionableUpdates.length;

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-x-4 gap-y-2 px-6 pb-3">
        <div className="flex min-w-0 items-baseline gap-3">
          {/* How many rows have an Update button. With none, not "0
              updates": the rows under "Can't update here" are real, and
              simply not Canager's to update. */}
          <p className="text-section text-foreground">
            {actionableCount === 0
              ? t("updates.noneActionable")
              : t("updates.count", { count: actionableCount })}
          </p>
          <p role="status" className="text-small text-muted">
            {copyStatus === "copied"
              ? t("updates.copied")
              : copyStatus === "failed"
                ? t("updates.copyFailed")
                : null}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {/* Select all, Invert selection and Update all act on the rows
              that show a checkbox (`actionableUpdates`) and on no others.
              A row in any other `UpdateState` -- read-only, could not be
              checked, blocked, its source not answering -- has no
              checkbox, and a row the user hid (skipped, or never to be
              reminded about) is not listed at all, so none of them selects
              one. `selectUpdates` and `invertUpdateSelection` change only
              the ids they are handed, so a row selected before a refresh
              took its checkbox away keeps its id in `selectedUpdates`,
              where `selectedVisible` already leaves it out. The words on
              the two are short; their accessible names also say which rows
              they act on. */}
          <button
            type="button"
            disabled={actionableCount === 0}
            onClick={() => selectUpdates(actionableUpdates.map((u) => u.key))}
            aria-label={t("updates.selectAllLabel")}
            className={HEADER_TEXT_BUTTON}
          >
            {t("updates.selectAll")}
          </button>
          <button
            type="button"
            disabled={actionableCount === 0}
            onClick={() => invertUpdateSelection(actionableUpdates.map((u) => u.key))}
            aria-label={t("updates.invertSelectionLabel")}
            className={HEADER_TEXT_BUTTON}
          >
            {t("updates.invertSelection")}
          </button>
          <button
            type="button"
            disabled={selectedVisible.length === 0 || dialogOpen}
            onClick={() => openConfirm(selectedVisible)}
            className="ml-2 rounded-button border border-border bg-surface px-3 py-1.5 text-body font-medium text-foreground outline-none transition-colors hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50 disabled:hover:bg-surface"
          >
            {selectedVisible.length === 0
              ? t("updates.updateSelected")
              : t("updates.updateSelectedCount", { number: selectedVisible.length })}
          </button>
          {/* Every row with a checkbox, ticked, into the same confirmation
              Update selected opens: one batch flow, not two. */}
          <button
            type="button"
            disabled={actionableCount === 0 || dialogOpen}
            onClick={() => {
              selectUpdates(actionableUpdates.map((u) => u.key));
              void openConfirm(actionableUpdates);
            }}
            className="rounded-button bg-accent px-4 py-1.5 text-body font-semibold text-accent-foreground outline-none transition-colors hover:bg-accent-hover focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-content disabled:opacity-50 disabled:hover:bg-accent"
          >
            {t("updates.updateAll")}
          </button>
        </div>
      </div>
      {noticeLines}
      {pageErrors.map((item) => (
        <p
          key={artifactKeyId(item.candidate.key)}
          role="alert"
          className="px-6 pb-2 text-body text-danger"
        >
          {t("updates.planFailed", { message: item.planError })}
        </p>
      ))}
      {saveSettings.isError ? (
        <p role="alert" className="px-6 pb-2 text-body text-danger">
          {t("updates.saveChoiceFailed", {
            message: settingsSaveErrorMessage(t, saveSettings.error.message),
          })}
        </p>
      ) : null}
      {/* Virtualized, like the Installed page. A source that cannot reach
          its registry reports one `checkable: false` candidate per
          installed package, so a Mac that is merely offline turns this
          into a list as long as everything it has installed -- and it
          would stall exactly when the user is already confused about why
          nothing could be checked. */}
      <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto px-3 pb-4">
        <div style={{ height: rowVirtualizer.getTotalSize(), position: "relative" }}>
          {rowVirtualizer.getVirtualItems().map((virtualRow) => {
            const item = items[virtualRow.index];
            return (
              // No fixed height on the slot: each reports its real height
              // back through `measureElement` instead.
              <div
                // `listItemKey`, through the virtualizer: the key its
                // measured height is filed under.
                key={virtualRow.key}
                data-index={virtualRow.index}
                data-list-slot=""
                ref={rowVirtualizer.measureElement}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              >
                {item.type === "section" ? (
                  <div className="pt-4">
                    <button
                      type="button"
                      aria-expanded={item.expanded}
                      onClick={() => setShowCantUpdate((shown) => !shown)}
                      className="flex w-full items-center gap-1.5 rounded-button px-3 py-2 text-left text-body font-semibold text-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-accent"
                    >
                      <ChevronIcon
                        size={14}
                        className={`shrink-0 transition-transform ${item.expanded ? "rotate-90" : ""}`}
                      />
                      {t("updates.cantUpdateHere", { number: item.count })}
                    </button>
                  </div>
                ) : item.type === "summary" ? (
                  <p className="px-3 pb-2 text-small text-muted">
                    {t("updates.cannotCheckSummary", {
                      count: item.count,
                      setting: t("settings.showTechnicalDetails.label"),
                    })}
                  </p>
                ) : (
                  updateRow(item.candidate)
                )}
              </div>
            );
          })}
        </div>
      </div>
      <Dialog
        open={dialogOpen}
        onOpenChange={(open) => {
          // Escape and overlay clicks arrive here. A submitting batch runs to
          // completion no matter what — closing early would leave the old
          // loop running against a dialog the user might reopen — so the
          // request is ignored until it has settled. The footer follows the
          // same rule: Cancel is disabled while submitting.
          if (!open && !submitting) setBatch(null);
        }}
        title={t("updates.confirmTitle")}
        footer={
          batch?.phase === "done" ? (
            <button
              type="button"
              onClick={() => setBatch(null)}
              className="rounded-md px-3 py-1 text-sm"
            >
              {t("common.close")}
            </button>
          ) : (
            <>
              <button
                type="button"
                onClick={() => setBatch(null)}
                disabled={submitting}
                className="rounded-md px-3 py-1 text-sm disabled:opacity-50"
              >
                {t("common.cancel")}
              </button>
              <button
                type="button"
                onClick={confirmAndSubmit}
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
              <div
                key={artifactKeyId(item.candidate.key)}
                className="flex flex-col gap-1"
              >
                <p className="text-sm font-medium text-[var(--color-foreground)]">{item.name}</p>
                {/* What you are moving to, spelled out (`dialogVersionJump`). */}
                {jump !== null ? (
                  <p className="text-sm text-[var(--color-muted)]">{jump}</p>
                ) : null}
                {item.planError !== null ? (
                  <p role="alert" className="text-sm text-[var(--color-danger)]">
                    {t("updates.planFailed", { message: item.planError })}
                  </p>
                ) : null}
                {item.issued !== null ? (
                  <CommandPreview action={item.issued.plan.action} />
                ) : null}
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
    </div>
  );
}
