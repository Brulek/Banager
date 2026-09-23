import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  useSnapshot,
  useSettings,
  useSaveSettings,
  usePlanOperation,
  useSubmitOperation,
} from "../lib/queries";
import { useUiStore, artifactKeyId } from "../store/ui";
import { ADAPTER_LABEL_KEYS, planErrorMessage, sourceNoticesFor } from "../lib/sources";
import type { SourceNoticeSpec } from "../lib/sources";
import { warningMessage, warningText, warningTexts } from "../lib/warnings";
import { ArtifactRow } from "../components/ArtifactRow";
import { SourceNotices } from "../components/SourceNotices";
import { CommandPreview } from "../components/CommandPreview";
import { Dialog } from "../components/ui/Dialog";
import type {
  ArtifactKey,
  InstalledArtifact,
  IssuedPlan,
  OpRequest,
  ReadOnlyReason,
  UpdateCandidate,
} from "../lib/types";

// The virtualizer's first guesses: a row, and a source's heading when it
// also carries a banner (a title line, a description line and, for
// Ollama, a button) -- the same two numbers as the Installed page. Every
// item measures itself through `measureElement` as soon as it is in the
// DOM, which matters here more than there: an uncheckable row wraps its
// explanation over as many lines as the tool's error text needs.
const ROW_ESTIMATE = 56;
const NOTICE_GROUP_ESTIMATE = 120;

/**
 * One slot in the virtualized list: a source's heading, or one of its
 * rows. The page is grouped by source, like the Installed page, because a
 * source's notice describes *its* rows and nobody else's. It used to be one
 * flat list with every notice hoisted above it, and a row carries no
 * source name -- so "Ollama didn't answer; what's listed here is what
 * Canager saw last time" sat over five rows of which three were this
 * minute's Homebrew data, and nothing on screen said which two it meant.
 */
type ListItem =
  | {
      type: "group";
      instanceId: string;
      label: string;
      // What `sourceNoticesFor` decided this source needs, carried on the
      // item for the same reason as on the Installed page: `estimateSize`
      // has only the item to ask whether this heading has a banner.
      notices: SourceNoticeSpec[];
    }
  | { type: "update"; candidate: UpdateCandidate };

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

export function UpdatesPage() {
  const { t } = useTranslation();
  const { data: snapshot, isLoading } = useSnapshot();
  const { data: settings } = useSettings();
  const saveSettings = useSaveSettings();
  // Used only for their promise-returning `mutateAsync` — which keeps
  // `useSubmitOperation`'s operations-query invalidation — never for their
  // `isPending`/`isError`/`error`; every flag the UI needs comes from `batch`.
  const planMutation = usePlanOperation();
  const submitMutation = useSubmitOperation();
  const selectedUpdates = useUiStore((s) => s.selectedUpdates);
  const toggleUpdate = useUiStore((s) => s.toggleUpdate);

  const listRef = useRef<HTMLDivElement>(null);
  const [batch, setBatch] = useState<Batch | null>(null);
  // Monotonic. The batch whose id equals this is the only one allowed to
  // write state; every async continuation checks `isCurrent` after `await`.
  const batchIdRef = useRef(0);

  const visibleUpdates = useMemo(() => {
    if (!snapshot || !settings) return [];
    const ignored = new Set(settings.ignored_updates.map((k) => artifactKeyId(k)));
    return snapshot.updates.filter((u) => !ignored.has(artifactKeyId(u.key)));
  }, [snapshot, settings]);

  // A candidate carries no adapter of its own; the only route from an
  // UpdateCandidate to the source that produced it is its key's
  // `instance_id`, joined back to the snapshot's instances. What comes back
  // is the *reason* rather than a yes/no, because the two reasons need
  // different advice and giving the wrong one is worse than giving none.
  const readOnlyReasons = useMemo(() => {
    const byInstance = new Map<string, ReadOnlyReason>();
    for (const instance of snapshot?.instances ?? []) {
      if (instance.read_only_reason !== null) {
        byInstance.set(instance.id, instance.read_only_reason);
      }
    }
    return byInstance;
  }, [snapshot]);

  // The page, one source at a time: each source's rows, and what that
  // source has to say about them, decided once here.
  //
  // The *state* axis always: whether Canager could reach a source at all
  // is not on any row, because a source it could not reach may well have
  // no rows -- a stopped Ollama on the first refresh after launch is a
  // heading and a banner with nothing under it, exactly as on the
  // Installed page.
  //
  // The *capability* axis only for a source that has rows here: "pip is
  // read-only" is not news on a page listing two Homebrew updates. When it
  // does apply it is said once, under the source's own heading, rather
  // than on each of its rows -- that advice runs to about two hundred
  // characters, and six outdated pip packages used to mean six identical
  // paragraphs displacing the six descriptions that tell the rows apart.
  //
  // How many rows a source has is also part of what its notice *says*: a
  // silent source's "what's listed here is last time's" is true only over
  // rows it actually has. Because the notice now sits directly above those
  // rows and no others, "here" means exactly them.
  //
  // Iterates `snapshot.instances`, which is every source any candidate can
  // come from: `refresh` builds `updates` only from instances it also puts
  // in `instances` (crates/canager-core/src/session/refresh.rs).
  const groups = useMemo(() => {
    const byInstance = new Map<string, UpdateCandidate[]>();
    for (const update of visibleUpdates) {
      const list = byInstance.get(update.key.instance_id) ?? [];
      list.push(update);
      byInstance.set(update.key.instance_id, list);
    }
    return (snapshot?.instances ?? []).map((instance) => {
      const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
      const label = labelKey ? t(labelKey) : instance.adapter_id;
      const rows = byInstance.get(instance.id) ?? [];
      const notices = sourceNoticesFor(instance, label, rows.length).filter(
        (notice) => notice.axis === "state" || rows.length > 0,
      );
      return { instanceId: instance.id, label, rows, notices };
    });
  }, [snapshot, visibleUpdates, t]);

  // What the two early returns below show above their one sentence. Both
  // run with no visible rows, so every group is empty there and this list
  // is state-only -- which is what decides between "Everything is up to
  // date" and "No updates in the sources Canager could check". A read-only
  // source is one Canager *can* check. With no rows there is nothing for a
  // notice to be mistaken as describing, so they can stand together.
  const instanceNotices = groups.flatMap((group) => group.notices);

  // The list proper: each source's heading, then its rows. A source with
  // neither rows nor anything to say is left out altogether.
  //
  // A source with something to say and no rows goes first. When every
  // notice sat at the top of the page, a stopped Ollama's Open Ollama
  // button was the first thing anyone saw; in `snapshot.instances` order it
  // would sit under however many Homebrew rows there are, scrolled out of
  // sight. Its notice describes no rows, so the top is still an honest
  // place for it. `sort` is stable, so both halves keep the instances'
  // order.
  const items = useMemo<ListItem[]>(
    () =>
      [...groups]
        .sort((a, b) => Number(a.rows.length > 0) - Number(b.rows.length > 0))
        .flatMap((group): ListItem[] =>
          group.rows.length === 0 && group.notices.length === 0
            ? []
            : [
                {
                  type: "group",
                  instanceId: group.instanceId,
                  label: group.label,
                  notices: group.notices,
                },
                ...group.rows.map((candidate): ListItem => ({ type: "update", candidate })),
              ],
        ),
    [groups],
  );

  // A source that did not answer keeps the candidates it reported last
  // time (`refresh` carries them forward), so these rows are on screen --
  // and none of them may offer a button. `Session::issue_plan` refuses
  // them in Rust whatever this says; offering an action and then refusing
  // it is the exact pattern this phase exists to remove.
  const unavailableInstances = useMemo(() => {
    const ids = new Set<string>();
    for (const instance of snapshot?.instances ?? []) {
      if (instance.status.unavailable !== null) ids.add(instance.id);
    }
    return ids;
  }, [snapshot]);

  /**
   * Whether this row may offer an Update button and a checkbox. Three
   * independent reasons it may not, and the wire now carries all three:
   *
   * - `checkable: false` -- the adapter could not establish what the remote
   *   version is.
   * - a read-only source -- its `plan()` refuses every operation, yet its
   *   candidates can still be built with `checkable: true` because the tool
   *   genuinely *can* check. Offering Update here produced nothing but a
   *   raw "unsupported: pip is read-only in Canager" string in a dialog.
   * - a source that is not answering -- the two axes are independent, and
   *   this one is new: a stopped Ollama is perfectly writable, and its
   *   candidates are still listed because `refresh` carries the last
   *   round's forward. `ollama pull` against a daemon that is not
   *   listening cannot succeed.
   *
   * `read_only_reason` replaced a hardcoded list of adapter ids on the
   * front end. `Session::issue_plan` applies the same conjunction in Rust
   * (spec §2.5), so a stale snapshot costs an error message, not a wrong
   * command.
   */
  const isActionable = (candidate: UpdateCandidate): boolean =>
    candidate.checkable &&
    !readOnlyReasons.has(candidate.key.instance_id) &&
    !unavailableInstances.has(candidate.key.instance_id);

  // Whether this row's source refuses every operation. The badge is all
  // the row says about it; *why*, and what to do instead, is the source's
  // own notice under its heading, said once.
  const isReadOnly = (candidate: UpdateCandidate): boolean =>
    readOnlyReasons.has(candidate.key.instance_id);

  // Two numbers, not one. Folding unactionable rows out of a single count
  // told a user with six outdated pip packages "0 updates available" above
  // six listed rows; folding them in would promise six Update buttons that
  // are not there.
  //
  // The split is `isActionable` itself, so the count and the buttons can
  // only ever agree. It used to test two of that predicate's three parts
  // and leave out `checkable`, which counted a row Canager had failed to
  // check as an available update -- "6 updates available" over six rows
  // with no buttons, on a machine where the registry had not answered at
  // all. A row from a source that is not answering counts as unmanageable
  // too: it is listed, it is real, and Canager cannot act on it right now
  // either.
  const actionableCount = visibleUpdates.filter(isActionable).length;
  const unmanageableCount = visibleUpdates.length - actionableCount;

  // Only rows that are selected, still visible *and* still actionable
  // count. The store keeps a selection for a row that has since been
  // ignored; without this intersection "Update selected" would be enabled
  // for nothing and open an empty dialog. Actionability is in the same
  // intersection because a selection outlives the row that made it: a
  // candidate selected while it was actionable stays selected after a
  // refresh takes that away, and the batch would then plan the very row
  // whose Update button has just gone.
  //
  // `isActionable` itself, not a second copy of its conditions. This used
  // to re-spell all three of them forty lines below where the predicate is
  // defined, which agreed with it exactly and would have stopped agreeing
  // the moment a fourth condition arrived (per-package actionability,
  // spec §8): the button and the count would drop the row, a selection
  // made before that refresh would still reach the batch, `issue_plan`
  // would pass it -- its gate is per *instance* -- and the tool's refusal
  // would come back as raw English.
  const selectedVisible = useMemo(
    () =>
      visibleUpdates.filter(
        (u) => isActionable(u) && selectedUpdates.includes(artifactKeyId(u.key)),
      ),
    // `isActionable` is rebuilt every render and so cannot be a dependency;
    // these are the three values it closes over, which is the same thing.
    [visibleUpdates, selectedUpdates, readOnlyReasons, unavailableInstances],
  );

  // One lookup table instead of a `snapshot.artifacts.find` per row: that
  // scan made every render O(updates × artifacts).
  const artifactsById = useMemo(() => {
    const byId = new Map<string, InstalledArtifact>();
    for (const artifact of snapshot?.artifacts ?? []) {
      byId.set(artifactKeyId(artifact.key), artifact);
    }
    return byId;
  }, [snapshot]);

  // Default view hides version numbers (Global Constraints); the row falls
  // back to the artifact's description, exactly as the Installed page does.
  const descriptionFor = (candidate: UpdateCandidate): string => {
    if (candidate.channel === "Digest") {
      // Ollama. `current` is the local manifest digest that /api/tags
      // reported and `target` is the registry manifest's config digest:
      // **different hash spaces**, not two readings of one identifier, and
      // they will not be equal even after a successful pull. The adapter's
      // own comment (crates/canager-core/src/adapters/ollama/mod.rs) says
      // never to render them as a version jump, and a 64-hex string is not
      // something to put in front of this audience either way. The channel
      // is the discriminator, so this holds whether or not technical
      // details are on -- a Digest row is a "changed / not changed" marker
      // and that is all it can honestly say.
      return t("updates.newBuild");
    }
    if (settings?.show_technical_details) {
      return t("updates.versionChange", { current: candidate.current, target: candidate.target });
    }
    return (
      artifactsById.get(artifactKeyId(candidate.key))?.description ?? t("installed.noDescription")
    );
  };

  /**
   * The version jump for the confirmation dialog, or null when there is no
   * honest one to show.
   *
   * A `Digest` candidate is Ollama: `current` is the local manifest digest
   * and `target` the registry manifest's config digest -- different hash
   * spaces, unequal even after a successful pull, and two 64-hex strings
   * are not something to put in front of this audience. It says "there is
   * a newer build" instead, exactly as the row does. An empty `current` or
   * `target` (a source that could name only one side) yields null rather
   * than a dangling arrow.
   */
  const versionJump = (candidate: UpdateCandidate): string | null => {
    if (candidate.channel === "Digest") return t("updates.newBuild");
    if (candidate.current === "" || candidate.target === "") return null;
    return t("updates.versionChange", { current: candidate.current, target: candidate.target });
  };

  /**
   * What an uncheckable row says about *why* it is uncheckable.
   *
   * Two kinds of text arrive in `warnings`. A warning with a key of its
   * own (`NonRegistrySource`) was written for this audience and already
   * reads as a whole sentence, so it is rendered as-is. A `Message` is
   * raw text off the wire -- a tool's stderr, an HTTP error -- kept
   * verbatim on purpose, which on its own makes the row's entire
   * description a line of somebody's stderr.
   *
   * That line is behind `show_technical_details`, which is exactly what
   * spec §6 says the right shape is: a localised sentence by default, the
   * raw string behind the switch that already promises to reveal "the
   * commands Canager actually runs". It matters more than it looks.
   * Before this branch an index outage produced *one* banner -- npm
   * returned an empty list, pip/uv/pipx returned `Err` -- and now it
   * produces one row per installed package, so leaving the stderr on
   * meant dozens of identical English sentences down the page and
   * nothing anywhere that reads as "you appear to be offline".
   */
  const cannotCheckText = (candidate: UpdateCandidate): string => {
    const parts = candidate.warnings.map((warning) => {
      const raw = warningMessage(warning);
      if (raw === null) return warningText(t, warning);
      return settings?.show_technical_details
        ? t("updates.cannotCheckDetail", { message: raw })
        : t("updates.cannotCheckPlain", { setting: t("settings.showTechnicalDetails.label") });
    });
    // Distinct, because with the switch off every `Message` on a row
    // collapses to the same sentence and a row carrying two of them would
    // otherwise say it twice.
    return [...new Set(parts)]
      .filter((text): text is string => text !== null && text !== "")
      .join(" ");
  };

  /**
   * The row's description: why Canager could not check this one, or --
   * when it could -- what the package is.
   *
   * Read-only-ness is deliberately not in here. The two axes are
   * independent and both can be true at once, and both used to be said on
   * the row; but the capability half is a property of the *source*, not of
   * this package, and six rows from one read-only source repeated it six
   * times while displacing the six blurbs that tell them apart. It is the
   * source's notice under its heading now, and the row keeps its own
   * description back.
   */
  const rowDescription = (candidate: UpdateCandidate): string =>
    candidate.checkable ? descriptionFor(candidate) : cannotCheckText(candidate);

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

  // The source's name in the user's language, for the one refusal that can
  // reach a real person verbatim otherwise (`planErrorMessage`'s
  // NotActionable case): a stale snapshot's own read-only/unavailable maps
  // (`readOnlyReasons`/`unavailableInstances` above) cannot be trusted for
  // *which* reason applies -- that is exactly what went stale -- but the
  // instance's adapter, and therefore its label, does not change underneath
  // it, so this is safe to read from the same snapshot.
  function sourceLabelFor(instanceId: string): string {
    const instance = snapshot?.instances.find((i) => i.id === instanceId);
    if (!instance) return instanceId;
    const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
    return labelKey ? t(labelKey) : instance.adapter_id;
  }

  async function openConfirm(candidates: UpdateCandidate[]) {
    // A new id retires whatever batch was still planning. Planning has no
    // side effect beyond issuing PlanIds that expire on their own, so the
    // newest click wins and the older batch's late replies are dropped by
    // `isCurrent`. Submitting is different — see the lock in the dialog.
    const id = batchIdRef.current + 1;
    batchIdRef.current = id;
    const blank = (c: UpdateCandidate): BatchItem => ({
      candidate: c,
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

  function ignore(candidate: UpdateCandidate) {
    // One save at a time. A second Ignore while the first is pending would
    // build its settings from the same stale base, and the later save would
    // overwrite the earlier one. The buttons are disabled meanwhile; this
    // guard covers a click that was already queued.
    if (!settings || saveSettings.isPending) return;
    if (selectedUpdates.includes(artifactKeyId(candidate.key))) {
      toggleUpdate(candidate.key);
    }
    saveSettings.mutate({
      ...settings,
      ignored_updates: [...settings.ignored_updates, candidate.key],
    });
  }

  // Above every early return: hooks cannot be called conditionally, and
  // three of the returns below are reached before the list is drawn.
  const rowVirtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => listRef.current,
    estimateSize: (index) => {
      const item = items[index];
      return item?.type === "group" && item.notices.length > 0
        ? NOTICE_GROUP_ESTIMATE
        : ROW_ESTIMATE;
    },
  });

  if (isLoading) {
    return <p className="p-4 text-sm text-[var(--color-muted)]">{t("common.loading")}</p>;
  }
  if (!snapshot || !settings) {
    return null;
  }

  // Two different kinds of empty: the backend found no updates, or it found
  // some and every one is on the ignore list. Only the first can mean the
  // machine is up to date -- and only when every source actually answered.
  //
  // The notices go *above* the early return, not after it. "Everything is
  // up to date" over a stopped Ollama or a Homebrew whose catalogue could
  // not be downloaded is precisely the lie this page used to tell: no
  // candidates is exactly what an unreachable source produces, and the
  // page read that silence as good news. When a source has something to
  // say, the headline drops to what Canager can honestly claim -- nothing
  // to update *in the sources it managed to check*.
  if (snapshot.updates.length === 0) {
    return (
      <div className="p-4">
        <SourceNotices notices={instanceNotices} />
        <p className="text-sm text-[var(--color-muted)]">
          {instanceNotices.length === 0 ? t("updates.upToDate") : t("updates.noneCheckable")}
        </p>
      </div>
    );
  }
  if (visibleUpdates.length === 0) {
    return (
      <div className="p-4">
        <SourceNotices notices={instanceNotices} />
        <p className="text-sm text-[var(--color-muted)]">{t("updates.allIgnored")}</p>
      </div>
    );
  }

  const dialogOpen = batch !== null && batch.phase !== "planning" && hasIssuedPlan(batch);
  const submitting = batch?.phase === "submitting";
  // Every plan failed: there is nothing to confirm, so the reasons go on the
  // page rather than into an empty dialog. Cleared by the next batch.
  const pageErrors =
    batch !== null && batch.phase === "done" && !hasIssuedPlan(batch) ? batch.items : [];

  // One update's row. A function rather than inline in the list only so
  // that the list's `map` can stay about slots -- headings and rows -- while
  // this stays about what one candidate offers.
  const updateRow = (candidate: UpdateCandidate) => {
    // Resolved once per row: the badge and the row's own actionability
    // must agree about whether this source is read-only.
    const readOnly = isReadOnly(candidate);
    return (
      <ArtifactRow
        name={candidate.key.name}
        // `checkable: false` means the adapter could not establish what
        // the remote version is -- a cargo crate installed from git or a
        // path, an Ollama model whose manifest could not be read, any
        // source whose registry lookup could not be made. Such a row
        // must offer no action and no selection: "Update" on a
        // git-sourced crate would run `cargo install --force {name}`
        // against the crates.io crate of the same name, which is a
        // different package. The reason lives in `warnings`, and
        // `rowDescription` is what puts it somewhere the user reads.
        description={rowDescription(candidate)}
        // An explanation has to be readable end to end or it has not
        // been given. The reason a lookup failed can run to a few
        // hundred characters and the detail comes last, so one
        // clipped line would hide precisely the part such a row
        // exists to say. A package's own blurb keeps the single
        // line: it is a nicety, not something the user is being
        // asked to act on.
        wrapDescription={!candidate.checkable}
        // Capability first when both apply: "Read-only" is the fact
        // that no button will ever appear on this row, whatever the
        // next refresh finds. That a lookup also failed is on the
        // row already, in words, via `rowDescription`.
        // Three states, and there is no fourth: nothing in production
        // builds a `checkable: true` candidate with a warning on it
        // any more. There used to be an "N warnings" badge here; brew's
        // `"pinned"` string and its per-candidate "brew update failed"
        // sentence were its only two producers, and this branch deleted
        // both (the second is now `InstanceNote::IndexMayBeStale`, a
        // notice on the source rather than a count on a row). The badge
        // outlived them, unreachable, which is the exact shape of defect
        // this phase keeps finding.
        badgeText={
          readOnly
            ? t("updates.readOnly")
            : !candidate.checkable
              ? t("updates.cannotCheck")
              : t("updates.available")
        }
        badgeVariant={readOnly || !candidate.checkable ? "neutral" : "info"}
        primaryActionLabel={isActionable(candidate) ? t("updates.update") : undefined}
        onPrimaryAction={
          isActionable(candidate) ? () => openConfirm([candidate]) : undefined
        }
        primaryActionDisabled={dialogOpen}
        selectable={
          isActionable(candidate)
            ? {
                checked: selectedUpdates.includes(artifactKeyId(candidate.key)),
                onToggle: () => toggleUpdate(candidate.key),
                ariaLabel: t("updates.selectRow", { name: candidate.key.name }),
              }
            : undefined
        }
        secondaryContent={
          <button
            type="button"
            onClick={() => ignore(candidate)}
            disabled={saveSettings.isPending}
            className="shrink-0 text-xs text-[var(--color-muted)] underline disabled:opacity-50"
          >
            {t("updates.ignore")}
          </button>
        }
      />
    );
  };

  return (
    <div className="flex h-full flex-col">
      {pageErrors.map((item) => (
        <p
          key={artifactKeyId(item.candidate.key)}
          role="alert"
          className="px-4 pt-4 text-sm text-[var(--color-danger)]"
        >
          {t("updates.planFailed", { message: item.planError ?? "" })}
        </p>
      ))}
      {saveSettings.isError ? (
        <p role="alert" className="px-4 pt-4 text-sm text-[var(--color-danger)]">
          {t("updates.ignoreFailed", { message: saveSettings.error.message })}
        </p>
      ) : null}
      <div className="flex items-center justify-between border-b border-[var(--color-border)] p-4">
        <div className="text-sm text-[var(--color-muted)]">
          {/* Two-part, always: the headline counts what Canager can act on,
              the second line counts what it cannot. "0 updates available" is
              a lie when the rows below exist and simply are not Canager's to
              update, so the headline says that instead -- but the second line
              stays, because "how many" is exactly what a user staring at six
              listed rows needs to know. */}
          <p>
            {actionableCount === 0 && unmanageableCount > 0
              ? t("updates.noneActionable")
              : t("updates.count", { count: actionableCount })}
          </p>
          {unmanageableCount > 0 ? (
            <p>{t("updates.countUnmanageable", { count: unmanageableCount })}</p>
          ) : null}
        </div>
        <button
          type="button"
          disabled={selectedVisible.length === 0 || dialogOpen}
          onClick={() => openConfirm(selectedVisible)}
          className="rounded-md bg-[var(--color-accent)] px-3 py-1 text-sm font-medium text-[var(--color-accent-foreground)] disabled:opacity-50"
        >
          {t("updates.updateSelected")}
        </button>
      </div>
      {/* Virtualized, like the Installed page. The entry that deferred this
          reasoned that the page only ever lists "a few to a few dozen"
          Homebrew updates; phase 3 killed that. A source that cannot reach
          its registry reports one `checkable: false` candidate per
          installed package, so a Mac that is merely offline turns this into
          a list as long as everything it has installed -- and it stalls
          exactly when the user is already confused about why nothing could
          be checked. */}
      <div ref={listRef} className="flex-1 overflow-y-auto">
        <div style={{ height: rowVirtualizer.getTotalSize(), position: "relative" }}>
          {rowVirtualizer.getVirtualItems().map((virtualRow) => {
            const item = items[virtualRow.index];
            return (
              // No fixed height on the slot: a heading with a banner is far
              // taller than a row, and an uncheckable row wraps its
              // explanation (`wrapDescription`) over as many lines as the
              // tool's error text needs, so each slot reports its real
              // height back through `measureElement` instead. A fixed height
              // would let the next slot -- later in DOM order, painted on
              // top -- cover the tail of the sentence this one exists to
              // say, or the Ollama banner's button.
              <div
                key={
                  item.type === "group"
                    ? `group:${item.instanceId}`
                    : artifactKeyId(item.candidate.key)
                }
                data-index={virtualRow.index}
                ref={rowVirtualizer.measureElement}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              >
                {item.type === "group" ? (
                  <div className="px-4 py-2">
                    <p className="text-xs font-semibold uppercase text-[var(--color-muted)]">
                      {item.label}
                    </p>
                    <SourceNotices notices={item.notices} />
                  </div>
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
            return (
              <div
                key={artifactKeyId(item.candidate.key)}
                className="flex flex-col gap-1"
              >
                <p className="text-sm font-medium text-[var(--color-foreground)]">
                  {item.candidate.key.name}
                </p>
                {/* What you are moving to, spelled out. Spec §6 asks this
                    screen to show the version jump, and unlike the row's
                    own description it is *not* behind
                    `show_technical_details`: a confirmation that names the
                    command but not the change is not a confirmation. */}
                {versionJump(item.candidate) !== null ? (
                  <p className="text-sm text-[var(--color-muted)]">
                    {versionJump(item.candidate)}
                  </p>
                ) : null}
                {item.planError !== null ? (
                  <p role="alert" className="text-sm text-[var(--color-danger)]">
                    {t("updates.planFailed", { message: item.planError })}
                  </p>
                ) : null}
                {item.issued !== null ? (
                  <CommandPreview program={item.issued.plan.program} args={item.issued.plan.args} />
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
