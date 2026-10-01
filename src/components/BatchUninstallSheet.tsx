import { useEffect, useId, useMemo, useRef, useState, type ReactNode, type RefObject } from "react";
import { useTranslation } from "react-i18next";
import { usePlanOperation, useSettings, useSizes, useSnapshot, useSubmitOperation } from "../lib/queries";
import {
  batchSizeOf,
  classify,
  itemSizeOf,
  mergeKept,
  PLAN_CONCURRENCY,
  PLAN_FRESH_FOR_MS,
  sizeCaveats,
  terminalCommands,
  type BatchCandidate,
  type Classification,
  type Exclusion,
} from "../lib/batchUninstall";
import {
  adapterLabel,
  instanceLabels,
  namesInSentence,
  parseUninstallBlocked,
  parseUninstallUnsafe,
  planErrorDetail,
  planErrorMessage,
  refusalSentence,
  uninstallBlockedCopy,
} from "../lib/sources";
import { skipsTrash, warningLine, warningLines, type WarningLine } from "../lib/warnings";
import { sizeText } from "../lib/sizes";
import { formatBytes } from "../lib/format";
import { twinsByArtifact } from "../lib/commands";
import { modelPath } from "../lib/names";
import { artifactKeyId, useUiStore } from "../store/ui";
import type { InstalledArtifact, ManagerInstance, OpRequest, PlanAction, Snapshot } from "../lib/types";
import { commandText } from "./CommandPreview";
import { KeptDataGroup } from "./KeptDataGroup";
import { installedBy, twinUninstallLine } from "./TwinAdvice";
import { hostedLines, hostedThroughLines } from "./hostedLines";
import { TextWithInfo } from "./InfoDetail";
import {
  Refusal,
  SheetIcon,
  SheetLines,
  SheetPending,
  SheetReason,
  SheetSection,
  SheetText,
  SheetTool,
  SheetToolList,
  sheetMeta,
} from "./SheetParts";
import { COMMAND_SLOT, withCommand } from "./withCommand";
import { CheckIcon, DisclosureIcon } from "./icons";
import { detailLines } from "./updateDetails";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";
import { SMALL_WRAPPING } from "./ui/group";

/** One ticked row, as the page hands it over: the tool, its source, and the name its row showed. */
export interface BatchTool {
  artifact: InstalledArtifact;
  instance: ManagerInstance;
  name: string;
}

/** One ticked tool's way through a batch (`useBatchUninstall`). */
export interface BatchEntry extends BatchCandidate {
  /** Its preview's id, for `submit_operation`, once issued. */
  planId: string | null;
  /** When its preview came back (`performance.now()`), for how fresh it still is. */
  plannedAt: number | null;
  /** Its operation, once it started. */
  opId: number | null;
  /** Why it did not start, in the backend's words. */
  submitError: string | null;
  /** The tools of the batch it was to run after that did not start: so it was not started either. */
  blockedBy: string[];
}

/**
 * A batch's whole state, kept explicitly as the update confirmation keeps
 * its own (`useUpdateConfirm`): a mutation's observer only knows its last
 * call.
 *
 *   planning ─(every preview back)─▶ ready ─(Uninstall)─▶ submitting ─▶ (closed | done)
 *
 * and back to planning from ready when the previews are too old to start
 * (`PLAN_FRESH_FOR_MS`). `id` is compared before anything an `await`
 * brought back is written: closing the sheet while it plans retires the
 * batch, and its late replies are dropped.
 */
export interface UninstallBatch {
  id: number;
  phase: "planning" | "ready" | "submitting" | "done";
  entries: BatchEntry[];
  /** Once every preview is back: what the batch includes, in the order it runs, and what not, and why. */
  classification: Classification | null;
  /** Planned again because the previews were too old to start: said beside the fresh ones. */
  reissued: boolean;
}

export interface BatchUninstall {
  batch: UninstallBatch | null;
  /** The sheet is up: 「卸载所选」 is off meanwhile. */
  sheetOpen: boolean;
  /**
   * Opens the sheet on `tools` at once, in the order given (the list's),
   * and previews each of them, at most `PLAN_CONCURRENCY` at a time.
   * `opener` gets the focus back when it closes; `onStarted` is where the
   * focus goes instead once everything started and the button pressed is
   * gone with what it counted.
   */
  open(tools: BatchTool[], opener: HTMLElement | null, onStarted?: VoidFunction): void;
  /** Uninstall: one operation per included tool, in order (spec §6.1). */
  confirm(): Promise<void>;
  /** Cancel, Escape, Close, OK: nothing while it submits. */
  close(): void;
  /** For the dialog once it has closed: `onStarted`, where everything started. */
  afterClose(): void;
  returnFocusTo: RefObject<HTMLElement | null>;
}

function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

function requestOf(entry: BatchEntry): OpRequest {
  return {
    kind: "Uninstall",
    instance_id: entry.artifact.key.instance_id,
    artifact_kind: entry.artifact.key.kind,
    name: entry.artifact.key.name,
  };
}

function blank(tool: BatchTool): BatchEntry {
  return {
    id: artifactKeyId(tool.artifact.key),
    artifact: tool.artifact,
    instance: tool.instance,
    name: tool.name,
    plan: null,
    planError: null,
    planId: null,
    plannedAt: null,
    opId: null,
    submitError: null,
    blockedBy: [],
  };
}

/** Whether `entry`'s preview has come back, issued or refused. */
function settled(entry: BatchEntry): boolean {
  return entry.plan !== null || entry.planError !== null;
}

/**
 * Uninstalling the Installed page's ticked rows (spec §5, §6): previews
 * each tool exactly as its own Uninstall would, says once which of them go
 * and which do not (`classify`), and on confirmation submits one uninstall
 * per included tool, in the order they run -- a Homebrew formula's ticked
 * dependents first -- through the same `submit_operation`, so the
 * operation bar's one run, its Cancel All and the completion notification
 * cover them all. No new command, call or file.
 */
export function useBatchUninstall(): BatchUninstall {
  const planMutation = usePlanOperation();
  const submitMutation = useSubmitOperation();
  const { data: snapshot } = useSnapshot();
  // Read after an `await`: the snapshot as it is then, not as it was when the batch opened.
  const snapshotRef = useRef<Snapshot | undefined>(snapshot);
  snapshotRef.current = snapshot;
  const [batch, setBatch] = useState<UninstallBatch | null>(null);
  const batchIdRef = useRef(0);
  const openerRef = useRef<HTMLElement | null>(null);
  const onStartedRef = useRef<VoidFunction | null>(null);
  const afterCloseRef = useRef<VoidFunction | null>(null);
  // Set before the first `await` of a confirm: `phase` reaches the button
  // only with the next draw, so a second click in the same turn would
  // otherwise start the batch twice.
  const submitLatch = useRef(false);

  const isCurrent = (id: number) => batchIdRef.current === id;

  async function planAll(id: number, entries: BatchEntry[], reissued: boolean): Promise<void> {
    const results = [...entries];
    let next = 0;
    // At most `PLAN_CONCURRENCY` previews in flight, in the list's order;
    // one refused never stops another (each settles on its own), and a
    // batch closed meanwhile starts no more.
    const worker = async () => {
      while (next < results.length) {
        if (!isCurrent(id)) return;
        const index = next;
        next += 1;
        const entry = results[index];
        let done: BatchEntry;
        try {
          const issued = await planMutation.mutateAsync(requestOf(entry));
          done = { ...entry, plan: issued.plan, planId: issued.id, plannedAt: performance.now(), planError: null };
        } catch (e) {
          done = { ...entry, plan: null, planId: null, plannedAt: null, planError: errorMessage(e) };
        }
        if (!isCurrent(id)) return;
        results[index] = done;
        setBatch((was) =>
          was !== null && was.id === id
            ? { ...was, entries: was.entries.map((other, at) => (at === index ? done : other)) }
            : was,
        );
      }
    };
    await Promise.all(Array.from({ length: Math.min(PLAN_CONCURRENCY, results.length) }, worker));
    if (!isCurrent(id)) return;
    setBatch({
      id,
      phase: "ready",
      entries: results,
      classification: classify(results, snapshotRef.current?.artifacts ?? []),
      reissued,
    });
  }

  function open(tools: BatchTool[], opener: HTMLElement | null, onStarted?: VoidFunction) {
    const id = batchIdRef.current + 1;
    batchIdRef.current = id;
    openerRef.current = opener;
    onStartedRef.current = onStarted ?? null;
    afterCloseRef.current = null;
    submitLatch.current = false;
    const entries = tools.map(blank);
    setBatch({ id, phase: "planning", entries, classification: null, reissued: false });
    void planAll(id, entries, false);
  }

  async function confirm(): Promise<void> {
    const current = batch;
    if (current === null || current.phase !== "ready" || current.classification === null || submitLatch.current) return;
    const { id, classification } = current;
    const { included } = classification;
    if (included.length === 0) return;
    submitLatch.current = true;
    try {
      const at = new Map(current.entries.map((entry, index) => [entry.id, index]));
      const entries = [...current.entries];
      // A preview older than `PLAN_FRESH_FOR_MS` would be refused as
      // expired: everything is planned again, and the sheet asks again.
      const oldest = Math.min(...included.map(({ candidate }) => entries[at.get(candidate.id)!].plannedAt ?? -Infinity));
      if (performance.now() - oldest > PLAN_FRESH_FOR_MS) {
        const fresh = entries.map((entry) => blank(entry));
        setBatch({ id, phase: "planning", entries: fresh, classification: null, reissued: false });
        await planAll(id, fresh, true);
        return;
      }
      setBatch({ ...current, phase: "submitting", reissued: false });
      const started = new Set<string>();
      for (const { candidate, after } of included) {
        const index = at.get(candidate.id)!;
        const entry = entries[index];
        // Its dependents in the batch run first; one that did not start
        // still needs it, and Homebrew would refuse it: not started either.
        const blockedBy = after.filter((dependent) => !started.has(dependent));
        if (blockedBy.length > 0 || entry.planId === null) {
          entries[index] = { ...entry, blockedBy };
        } else {
          try {
            const opId = await submitMutation.mutateAsync(entry.planId);
            entries[index] = { ...entry, opId };
            started.add(entry.id);
            // The name its row showed, for the operation bar and the log
            // after the row has gone.
            useUiStore.getState().rememberOpNames({ [opId]: entry.name });
            // A started tool leaves the ticks at once, so a second batch
            // after a partial one never queues it again.
            if (isCurrent(id)) useUiStore.getState().deselectUninstalls([entry.artifact.key]);
          } catch (e) {
            entries[index] = { ...entry, submitError: errorMessage(e) };
          }
        }
        if (!isCurrent(id)) return;
        setBatch((was) => (was !== null && was.id === id ? { ...was, entries: [...entries] } : was));
      }
      useUiStore.getState().setUninstallBatch({
        id,
        items: included.map(({ candidate, after }) => {
          const entry = entries[at.get(candidate.id)!];
          return { key: entry.artifact.key, name: entry.name, opId: entry.opId, after };
        }),
      });
      const allStarted = included.every(({ candidate }) => entries[at.get(candidate.id)!].opId !== null);
      if (allStarted) {
        afterCloseRef.current = onStartedRef.current;
        setBatch(null);
      } else {
        setBatch({ id, phase: "done", entries, classification, reissued: false });
      }
    } finally {
      submitLatch.current = false;
    }
  }

  function close() {
    if (batch?.phase === "submitting") return;
    // Retired: its previews still on their way open nothing, and no more start.
    batchIdRef.current += 1;
    setBatch(null);
  }

  function afterClose() {
    const then = afterCloseRef.current;
    afterCloseRef.current = null;
    then?.();
  }

  return { batch, sheetOpen: batch !== null, open, confirm, close, afterClose, returnFocusTo: openerRef };
}

/** The window's words (`useTranslation`'s `t`). */
type Translate = ReturnType<typeof useTranslation>["t"];

/** `names` as a sentence says them, each in quotation marks in Chinese: 「“pipx”和“poetry”」. */
function quotedNames(t: Translate, names: readonly string[]): string {
  return namesInSentence(
    t,
    names.map((name) => t("batchUninstall.quoted", { name })),
  );
}

/** At most three names, then how many in all: 「git、gh、jq等7个」 (`commands.names`). */
function someNames(t: Translate, names: readonly string[]): string {
  const separator = t("common.listSeparator");
  if (names.length <= 3) return namesInSentence(t, [...names]);
  return t("commands.names", { names: names.slice(0, 3).join(separator), count: names.length, rest: names.length - 3 });
}

/**
 * A preview's refusal, said as the single uninstall says it
 * (`UninstallDialog`'s `refusal`): a pinned package's with its unpin
 * command set apart as code, a path-list preview's refused path in its own
 * sentence, any other in 「无法检查影响：…」 without the backend's words
 * unless "Show technical details" is on.
 */
function refusedText(
  t: Translate,
  raw: string,
  entry: BatchEntry,
  sourceLabel: string,
  technical: boolean,
): { text: ReactNode; detail: string | null; title: string } {
  const detail = planErrorDetail(t, raw);
  if (parseUninstallUnsafe(raw) !== null) {
    const text = planErrorMessage(t, raw, sourceLabel, technical) ?? refusalSentence(t, "uninstall.planError", raw, sourceLabel, technical);
    return { text, detail, title: text };
  }
  const blocked = parseUninstallBlocked(raw);
  if (blocked === null) {
    const text = refusalSentence(t, "uninstall.planError", raw, sourceLabel, technical);
    return { text, detail, title: text };
  }
  const copy = uninstallBlockedCopy(blocked, entry.instance.adapter_id);
  const sentence = t(copy.refused, { command: COMMAND_SLOT, source: sourceLabel });
  return {
    text: withCommand(sentence, copy.command(entry.artifact.key, entry.instance)),
    detail: null,
    title: sentence,
  };
}

/**
 * The confirmation `useBatchUninstall` drives, a sheet in the manner of
 * the uninstall alert (spec §5.5): 「要卸载这3个工具吗？」, what goes -- each
 * tool with what its uninstall deletes and keeps, in the order they run --
 * what does not and why, what stays after, the exact commands and paths
 * one click away, and Cancel, which has the focus, beside 「卸载这3个」.
 * Nothing on it deletes anything kept.
 */
export function BatchUninstallSheet({ uninstall }: { uninstall: BatchUninstall }) {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: sizes } = useSizes();
  const { data: settings } = useSettings();
  const technical = settings?.show_technical_details ?? false;
  const { batch } = uninstall;
  const cancelRef = useRef<HTMLButtonElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const okRef = useRef<HTMLButtonElement>(null);
  const textId = useId();

  const labels = useMemo(() => instanceLabels(t, snapshot?.instances ?? []), [t, snapshot]);
  const sourceLabelOf = (instance: ManagerInstance): string =>
    labels.get(instance.id) ?? adapterLabel(t, instance.adapter_id);
  const twins = useMemo(() => twinsByArtifact(snapshot?.artifacts ?? []), [snapshot]);

  const phase = batch?.phase;
  const noneIncluded = batch?.classification?.included.length === 0;
  // Done with something not started: Close takes the focus, as the update
  // confirmation's does; and OK, where nothing could be included, from the
  // Cancel that had it while the sheet was checking.
  useEffect(() => {
    if (phase === "done") closeRef.current?.focus();
    if (phase === "ready" && noneIncluded) okRef.current?.focus();
  }, [phase, noneIncluded]);

  const entries = batch?.entries ?? [];
  const byId = new Map(entries.map((entry) => [entry.id, entry]));
  const classification = batch?.classification ?? null;
  const included = (classification?.included ?? []).map(({ candidate, after }) => ({
    entry: byId.get(candidate.id)!,
    after,
  }));
  const excluded = (classification?.excluded ?? []).map(({ candidate, reason }) => ({
    entry: byId.get(candidate.id)!,
    reason,
  }));
  const planning = phase === "planning";
  const submitting = phase === "submitting";
  const nameOf = (id: string) => byId.get(id)?.name ?? id;
  // A name ticked from two sources says which is which (spec R3).
  const names = entries.map((entry) => entry.name);
  const twice = new Set(names.filter((name, index) => names.indexOf(name) !== index));

  // What the sheet asks about: everything ticked while it plans, then what it includes.
  const asked = planning ? entries : included.map(({ entry }) => entry);
  const only = asked.length === 1 ? asked[0] : null;
  const title =
    !planning && included.length === 0
      ? entries.length === 1
        ? t("reviewFixes.cannotUninstallOne", { name: entries[0].name })
        : t("batchUninstall.titleNone")
      : only !== null
        ? t("uninstall.title", { name: only.name })
        : t("batchUninstall.title", { count: asked.length });
  const onlyVersion =
    only === null || only.artifact.key.kind === "Model" || only.artifact.version === "" ? null : only.artifact.version;

  // What the included tools say together.
  const includedArtifacts = included.map(({ entry }) => entry.artifact);
  const { measured: total, unknown } = batchSizeOf(sizes, includedArtifacts);
  const caveats = sizeCaveats(
    included.map(({ entry }) => ({ artifact: entry.artifact, instance: entry.instance, plan: entry.plan! })),
    unknown,
  );
  const commands = terminalCommands(includedArtifacts, snapshot?.artifacts ?? []);
  const keptAi = included.filter(({ entry }) =>
    entry.plan!.warnings.some((warning) => typeof warning !== "string" && "KeepsData" in warning),
  ).length;
  const asksPassword = included.filter(({ entry }) => entry.plan!.needs_password).map(({ entry }) => entry.name);
  const batchNotes: WarningLine[] = [
    ...(commands.lost.length > 0
      ? [
          {
            text: t("batchUninstall.commandsLost", { names: someNames(t, commands.lost), count: commands.lost.length }),
            // Every one of them, where the line names only the first three.
            detail: commands.lost.length > 3 ? namesInSentence(t, commands.lost) : null,
            caution: true,
          },
        ]
      : []),
    ...(commands.unjudged.length > 0
      ? [
          {
            text: t("batchUninstallMore.commandsGo", {
              names: someNames(t, commands.unjudged),
              count: commands.unjudged.length,
            }),
            detail: commands.unjudged.length > 3 ? namesInSentence(t, commands.unjudged) : null,
            caution: false,
          },
        ]
      : []),
    ...(keptAi > 0 ? [{ text: t("batchUninstall.keptAi", { count: keptAi }), detail: null, caution: false }] : []),
    // Which of them may ask, by name: every cask uninstall may, not every app does.
    ...(asksPassword.length > 0
      ? [
          {
            text: t("reviewFixes.passwordNamed", { names: quotedNames(t, asksPassword), count: asksPassword.length }),
            detail: null,
            caution: false,
          },
        ]
      : []),
  ];
  const kept = mergeKept(included.map(({ entry }) => ({ name: entry.name, warnings: entry.plan!.warnings })));

  /** What one included tool says under its name (spec §5.5, item 4). */
  const linesOf = (entry: BatchEntry, after: string[]): WarningLine[] => {
    const plan = entry.plan!;
    const lines = warningLines(t, plan.warnings, plan.affected, entry.name);
    const scope = lines.scope.map((line) => line.text);
    const said =
      scope.length > 0 && skipsTrash(plan.warnings)
        ? [...scope.slice(0, -1), t("uninstall.endsSkipsTrash", { sentence: scope[scope.length - 1] })]
        : scope;
    const plain = (text: string): WarningLine => ({ text, detail: null, caution: false });
    const order =
      after.length === 0
        ? []
        : [
            {
              text: t("batchUninstall.after", { names: quotedNames(t, after.map(nameOf)) }),
              detail: t("batchUninstall.afterDetail", { names: quotedNames(t, after.map(nameOf)), count: after.length }),
              caution: false,
            },
          ];
    const trash =
      "TrashPaths" in plan.action
        ? [
            plain(t("uninstall.trashPreview", { count: plan.action.TrashPaths.paths.length })),
            ...plan.warnings.flatMap((warning) => {
              if (typeof warning === "string" || !("AlreadyGone" in warning)) return [];
              const line = warningLine(t, warning);
              return line === null ? [] : [line];
            }),
          ]
        : [];
    // The other copies of the tool the batch leaves: what stays, and what
    // runs instead (`twinUninstallLine`).
    const going = new Set(included.map(({ entry: other }) => other.id));
    const others = (twins.get(entry.id) ?? []).filter((twin) => !going.has(artifactKeyId(twin.artifact.key)));
    const labelFor = (instanceId: string) => {
      const instance = snapshot?.instances.find((candidate) => candidate.id === instanceId);
      return instance === undefined ? instanceId : sourceLabelOf(instance);
    };
    const twin = twinUninstallLine(t, entry.artifact, others, labelFor);
    const takenOver = (commands.takenOver.get(entry.id) ?? []).map(({ command, by }) =>
      plain(
        by.length === 1
          ? t("batchUninstall.takenOver", { command, by: installedBy(t, by[0].key.instance_id, labelFor) })
          : t("batchUninstall.takenOverAnother", { command }),
      ),
    );
    const frees =
      entry.artifact.key.kind === "Model" && entry.artifact.size_bytes !== null
        ? [plain(t("clarity.freesModel", { size: formatBytes(entry.artifact.size_bytes) }))]
        : [];
    // What other sources installed through it: none of those tools is in
    // the batch (X5 leaves a program out where one is), so each stays.
    const hosted = [
      ...hostedLines(t, entry.artifact, snapshot?.instances ?? [], snapshot?.artifacts ?? []),
      // And what those it runs after leave behind, which may have used it.
      ...hostedThroughLines(
        t,
        after.flatMap((id) => {
          const host = byId.get(id);
          return host === undefined ? [] : [host.artifact];
        }),
        snapshot?.instances ?? [],
        snapshot?.artifacts ?? [],
      ),
    ];
    return [
      ...said.map(plain),
      ...order,
      ...trash,
      ...hosted,
      ...lines.note.filter((line) => line.caution),
      ...lines.note.filter((line) => !line.caution),
      ...(twin === null ? [] : [plain(twin)]),
      ...takenOver,
      ...frees,
    ];
  };

  /** What became of an included tool once submitted: started, or why not. */
  const outcomeOf = (entry: BatchEntry): ReactNode => {
    if (entry.opId !== null) {
      return (
        <p className="flex items-center gap-1 text-small text-foreground">
          <CheckIcon size={12} className="shrink-0 text-success" />
          {t("updates.started")}
        </p>
      );
    }
    if (entry.submitError !== null) {
      const text = refusalSentence(t, "uninstall.submitError", entry.submitError, sourceLabelOf(entry.instance), technical);
      return <Refusal text={text} detail={planErrorDetail(t, entry.submitError)} detailTitle={text} size="small" />;
    }
    if (entry.blockedBy.length > 0) {
      const text = t("batchUninstall.notStartedAfter", {
        names: quotedNames(t, entry.blockedBy.map(nameOf)),
        count: entry.blockedBy.length,
      });
      return <SheetReason text={text} caution />;
    }
    return null;
  };

  /** Why a ticked tool is left out (spec §5.3). */
  const reasonOf = (entry: BatchEntry, reason: Exclusion): ReactNode => {
    // An explanation, in the secondary colour: only a preview Banager could
    // not make (X1) is said as a refusal.
    const plainRefusal = (text: string) => <SheetReason text={text} />;
    switch (reason.kind) {
      case "refused": {
        const { text, detail, title: detailTitle } = refusedText(t, reason.raw, entry, sourceLabelOf(entry.instance), technical);
        return <Refusal text={text} detail={detail} detailTitle={detailTitle} size="small" />;
      }
      case "noCancel":
        return plainRefusal(t("batchUninstall.reason.noCancel"));
      case "permanent":
        return plainRefusal(t("batchUninstall.reason.permanent"));
      case "unseen":
        return plainRefusal(t("batchUninstall.reason.unseen"));
      case "cycle":
        return plainRefusal(t("batchUninstall.reason.cycle"));
      case "host": {
        const sources = [...new Set(reason.by.map((id) => byId.get(id)).flatMap((other) => (other ? [sourceLabelOf(other.instance)] : [])))];
        return plainRefusal(t("batchUninstall.reason.host", { source: namesInSentence(t, sources) }));
      }
      case "stillNeeded":
        return plainRefusal(t("batchUninstall.reason.stillNeeded", { names: namesInSentence(t, reason.names) }));
      case "neededByExcluded":
        return plainRefusal(
          t("batchUninstall.reason.neededByExcluded", {
            names: quotedNames(t, reason.ids.map(nameOf)),
            count: reason.ids.length,
          }),
        );
      case "afterOthers":
        return plainRefusal(t("batchUninstall.reason.afterOthers", { names: quotedNames(t, reason.ids.map(nameOf)) }));
    }
  };

  const toolOf = (entry: BatchEntry, children: ReactNode, aside: ReactNode = null) => (
    <SheetTool
      key={entry.id}
      adapterId={entry.instance.adapter_id}
      sourceLabel={sourceLabelOf(entry.instance)}
      showSource={twice.has(entry.name)}
      iconKey={entry.artifact.key}
      name={entry.name}
      shownName={modelPath(entry.artifact.key, entry.name)?.name}
      aside={aside}
    >
      {children}
    </SheetTool>
  );

  // The sheet's text: what goes and what they take, then how many do not.
  const texts: Array<{ id: string; node: ReactNode }> = [];
  if (!planning && included.length > 1) {
    // What goes, and what it takes together -- 「…共占约3.2 GB。」, the
    // hedge the weakest size needs -- in one sentence's key, as each
    // language joins two sentences its own way; the why behind its ⓘ.
    const said =
      total === null
        ? t("batchUninstall.willGo", { count: included.length })
        : t(
            total.at_least
              ? "batchUninstall.willGoTakesAtLeast"
              : total.partial
                ? "batchUninstall.willGoTakesPartial"
                : "batchUninstall.willGoTakes",
            { count: included.length, size: formatBytes(total.bytes) },
          );
    texts.push({
      id: `${textId}-go`,
      node:
        total === null || caveats.length === 0 ? (
          said
        ) : (
          <TextWithInfo text={said} label={t("batchUninstall.takesLabel")}>
            {detailLines(caveats.map((key) => t(key, key === "batchUninstall.takesUnknown" ? { count: unknown } : {})))}
          </TextWithInfo>
        ),
    });
  }
  if (!planning && excluded.length > 0 && included.length > 0) {
    texts.push({ id: `${textId}-stay`, node: t("batchUninstall.notIncluded", { count: excluded.length }) });
  }

  const done = entries.filter(settled).length;
  // While it checks, as many as it asks about; then as many as it includes.
  const going = planning ? entries.length : included.length;
  const confirmLabel = going === 1 ? t("uninstall.confirm") : t("batchUninstall.confirm", { count: going });

  return (
    <Dialog
      open={batch !== null}
      onOpenChange={(open) => {
        if (!open && !submitting) uninstall.close();
      }}
      title={title}
      width="several"
      icon={
        only !== null && (planning || included.length > 0) ? (
          <SheetIcon adapterId={only.instance.adapter_id} sourceLabel={sourceLabelOf(only.instance)} iconKey={only.artifact.key} />
        ) : undefined
      }
      subtitle={only !== null && (planning || included.length > 0) ? sheetMeta(only.name, sourceLabelOf(only.instance), onlyVersion) : undefined}
      describedBy={texts.length > 0 ? texts.map((text) => text.id).join(" ") : undefined}
      initialFocus={phase === "done" ? closeRef : !planning && included.length === 0 ? okRef : cancelRef}
      returnFocusTo={uninstall.returnFocusTo}
      onClosed={uninstall.afterClose}
      footer={
        phase === "done" ? (
          <button ref={closeRef} type="button" onClick={uninstall.close} className={BUTTON.large.default}>
            {t("common.close")}
          </button>
        ) : !planning && included.length === 0 ? (
          <button ref={okRef} type="button" onClick={uninstall.close} className={BUTTON.large.default}>
            {t("batchUninstall.ok")}
          </button>
        ) : (
          <>
            <button ref={cancelRef} type="button" onClick={uninstall.close} disabled={submitting} className={BUTTON.large.grey}>
              {t("common.cancel")}
            </button>
            <button
              type="button"
              onClick={() => void uninstall.confirm()}
              disabled={phase !== "ready" || included.length === 0}
              className={BUTTON.large.default}
            >
              {confirmLabel}
            </button>
          </>
        )
      }
    >
      {texts.map((text) => (
        <SheetText key={text.id} id={text.id} className="mb-1">
          {text.node}
        </SheetText>
      ))}

      {/* There, empty and out of sight, from the time the sheet opens, as
          the single uninstall's note is (`UninstallDialog`): a status put
          in the page with its words is one a screen reader may never read. */}
      <p role="status" className={batch?.reissued && phase === "ready" ? "mt-1 mb-2 text-body text-muted" : "sr-only"}>
        {batch?.reissued && phase === "ready" ? t("uninstall.reissuedConfirmAgain") : null}
      </p>

      {planning ? (
        <>
          <SheetToolList label={t("batchUninstall.goTitle")} contained={false}>{entries.map((entry) => toolOf(entry, null))}</SheetToolList>
          <SheetPending text={t("batchUninstall.checking", { done, total: entries.length })} />
        </>
      ) : (
        <>
          {batchNotes.length > 0 ? (
            <SheetSection title={t("uninstall.warningsTitle")} titleHidden>
              <SheetLines lines={batchNotes} />
            </SheetSection>
          ) : null}
          {included.length > 0 ? (
            <div className="mt-3">
              <SheetToolList label={t("batchUninstall.goTitle")} contained={false}>
                {included.map(({ entry, after }) => {
                  const size = itemSizeOf(sizes, entry.artifact);
                  return toolOf(
                    entry,
                    <>
                      <SheetLines lines={linesOf(entry, after)} />
                      {outcomeOf(entry)}
                    </>,
                    size === null ? null : sizeText(t, size),
                  );
                })}
              </SheetToolList>
            </div>
          ) : null}
          {excluded.length > 0 ? (
            <SheetSection title={t("batchUninstall.stayTitle")}>
              <SheetToolList label={t("batchUninstall.stayTitle")} contained={false}>
                {excluded.map(({ entry, reason }) => toolOf(entry, reasonOf(entry, reason)))}
              </SheetToolList>
            </SheetSection>
          ) : null}
          {/* What stays: each path once, whose it is, Copy Path -- and no
              way here to delete any of it (`KeptDataGroup`). */}
          <KeptDataGroup warnings={kept.warnings} ownersOf={included.length > 1 ? (path) => kept.owners.get(path) ?? [] : undefined} />
          {included.length > 0 ? (
            <BatchPlanDetails
              plans={included.map(({ entry }) => ({ id: entry.id, name: entry.name, action: entry.plan!.action }))}
            />
          ) : null}
        </>
      )}
    </Dialog>
  );
}

/**
 * The exact commands and the exact paths a batch's previews carry, one
 * click away (spec §5.5, item 7), in the order the tools run: under each
 * tool's name, its command as Terminal would take it (`commandText`), or
 * the paths a tool with its own installer moves to the Trash. Open from
 * the start while "Show technical details" is on, as `CommandPreview` is.
 * 「查看命令」, or 「查看命令和路径」 where any of them moves paths.
 */
function BatchPlanDetails({ plans }: { plans: Array<{ id: string; name: string; action: PlanAction }> }) {
  const { t } = useTranslation();
  const { data: settings } = useSettings();
  const [chosen, setChosen] = useState<boolean | null>(null);
  const open = chosen ?? settings?.show_technical_details ?? false;
  const panelId = useId();
  const paths = plans.some(({ action }) => "TrashPaths" in action);
  return (
    <div className="mt-3">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        onClick={() => setChosen(!open)}
        className="-ml-1 flex h-7 items-center gap-1.5 rounded-control px-1 text-body text-muted"
      >
        <DisclosureIcon size={10} className={`shrink-0 ${open ? "rotate-90" : ""}`} />
        {paths ? t("batchUninstall.showCommandsAndPaths") : t("commandPreview.show", { count: plans.length })}
      </button>
      {open ? (
        <div id={panelId} data-batch-plans="" className="mt-1 flex flex-col gap-2">
          {plans.map(({ id, name, action }) => (
            <div key={id}>
              <p className={`mb-1 text-muted ${SMALL_WRAPPING}`}>{name}</p>
              <code className="block select-text whitespace-pre-wrap break-words rounded-control bg-group px-2.5 py-2 font-mono text-small text-foreground">
                {"Command" in action ? commandText(action) : action.TrashPaths.paths.join("\n")}
              </code>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
