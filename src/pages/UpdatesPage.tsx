import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  useSnapshot,
  useSettings,
  useSaveSettings,
  usePlanOperation,
  useSubmitOperation,
} from "../lib/queries";
import { useUiStore, artifactKeyId } from "../store/ui";
import { ADAPTER_LABEL_KEYS, READ_ONLY_NOTICE_KEYS, sourceNoticesFor } from "../lib/sources";
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
  key: ArtifactKey;
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

  // What the sources themselves have to say, for the top of this page.
  // Only the *state* axis: a read-only source's advice is already on every
  // one of its rows (see `readOnlyNoticeKeyFor`), and repeating it as a
  // banner would say the same thing twice. Whether Canager could reach a
  // source at all is not on any row, because a source it could not reach
  // may well have no rows.
  const instanceNotices = useMemo(
    () =>
      (snapshot?.instances ?? []).flatMap((instance) => {
        const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
        return sourceNoticesFor(instance, labelKey ? t(labelKey) : instance.adapter_id).filter(
          (notice) => notice.axis === "state",
        );
      }),
    [snapshot, t],
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

  const readOnlyReasonFor = (candidate: UpdateCandidate): ReadOnlyReason | undefined =>
    readOnlyReasons.get(candidate.key.instance_id);

  // The `sourceNotice.*` prefix whose copy explains why this row cannot be
  // updated here, or undefined when it can. `undefined` is the whole
  // "actionable" answer for the capability axis, so the row's description
  // and badge both branch on this one value.
  const readOnlyNoticeKeyFor = (candidate: UpdateCandidate): string | undefined => {
    const reason = readOnlyReasonFor(candidate);
    return reason ? READ_ONLY_NOTICE_KEYS[reason] : undefined;
  };

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

  // Only rows that are selected, still visible *and* still checkable count.
  // The store keeps a selection for a row that has since been ignored;
  // without this intersection "Update selected" would be enabled for nothing
  // and open an empty dialog. `checkable` is in the same intersection
  // because a selection outlives the row that made it: a candidate selected
  // while it was checkable stays selected after a refresh flips the flag,
  // and the batch would then plan the very row whose Update button was just
  // taken away.
  const selectedVisible = useMemo(
    () =>
      visibleUpdates.filter(
        (u) =>
          u.checkable &&
          !readOnlyReasons.has(u.key.instance_id) &&
          !unavailableInstances.has(u.key.instance_id) &&
          selectedUpdates.includes(artifactKeyId(u.key)),
      ),
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
   * What an uncheckable row says about *why* it is uncheckable.
   *
   * Two kinds of text arrive in `warnings`. A warning with a key of its
   * own (`NonRegistrySource`) was written for this audience and already
   * reads as a whole sentence, so it is rendered as-is. A `Message` is
   * raw text off the wire -- a tool's stderr, an HTTP error -- kept
   * verbatim on purpose (spec §6 backlogs localising it), which on its
   * own makes the row's entire description a line of somebody's stderr.
   * Wrapping it in one localised sentence is what turns that into
   * something a non-programmer can read: first what happened, then the
   * detail they can pass on to someone who can act on it.
   */
  const cannotCheckText = (candidate: UpdateCandidate): string =>
    candidate.warnings
      .map((warning) => {
        const raw = warningMessage(warning);
        return raw === null
          ? warningText(t, warning)
          : t("updates.cannotCheckDetail", { message: raw });
      })
      .filter((text): text is string => text !== null && text !== "")
      .join(" ");

  /**
   * The row's description. The two axes are independent and both can be
   * true at once, so both get said: a read-only source's advice explains
   * why this row will never have a button, and an uncheckable row's
   * reason explains why it has no version information either. Showing
   * only the first -- which is what happened before, because pip is
   * read-only *and* reaches PyPI -- left a row that looks exactly like a
   * good day's row while Canager had in fact learned nothing about it.
   */
  const rowDescription = (candidate: UpdateCandidate, noticeKey: string | undefined): string => {
    const parts: string[] = [];
    if (noticeKey) parts.push(t(`${noticeKey}.description`));
    if (!candidate.checkable) parts.push(cannotCheckText(candidate));
    else if (!noticeKey) parts.push(descriptionFor(candidate));
    return parts.filter((part) => part !== "").join(" ");
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

  async function openConfirm(candidates: UpdateCandidate[]) {
    // A new id retires whatever batch was still planning. Planning has no
    // side effect beyond issuing PlanIds that expire on their own, so the
    // newest click wins and the older batch's late replies are dropped by
    // `isCurrent`. Submitting is different — see the lock in the dialog.
    const id = batchIdRef.current + 1;
    batchIdRef.current = id;
    const blank = (c: UpdateCandidate): BatchItem => ({
      key: c.key,
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
        planError: result.status === "rejected" ? errorMessage(result.reason) : null,
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
        if (isCurrent(id)) deselect(item.key);
      } catch (e) {
        // A PlanId is single-use and expires after 10 minutes. Whatever the
        // backend said (`Expired`, `Unknown`, anything else), this id is
        // spent: keep the message verbatim and carry on with the next item.
        items[i] = { ...item, submitError: errorMessage(e) };
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

  return (
    <div className="flex h-full flex-col">
      {instanceNotices.length > 0 ? (
        <div className="px-4 pt-4">
          <SourceNotices notices={instanceNotices} />
        </div>
      ) : null}
      {pageErrors.map((item) => (
        <p
          key={artifactKeyId(item.key)}
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
      <div className="flex-1 overflow-y-auto">
        {visibleUpdates.map((candidate) => {
          // Resolved once per row: the row's description and its badge must
          // agree about whether this source is read-only, and about which
          // reason it is.
          const noticeKey = readOnlyNoticeKeyFor(candidate);
          return (
            <ArtifactRow
              key={artifactKeyId(candidate.key)}
              name={candidate.key.name}
              // `checkable: false` means the adapter could not establish what
              // the remote version is -- a cargo crate installed from git or a
              // path, an Ollama model whose manifest could not be read, any
              // source whose registry lookup could not be made. Such a row
              // must offer no action and no selection: "Update" on a
              // git-sourced crate would run `cargo install --force {name}`
              // against the crates.io crate of the same name, which is a
              // different package. The reason lives in `warnings`, and
              // `rowDescription` is what puts it somewhere the user reads
              // -- read-only guidance included, since a read-only source
              // can fail a lookup too.
              description={rowDescription(candidate, noticeKey)}
              // Capability first when both apply: "Read-only" is the fact
              // that no button will ever appear on this row, whatever the
              // next refresh finds. That a lookup also failed is on the
              // row already, in words, via `rowDescription`.
              badgeText={
                noticeKey
                  ? t("updates.readOnly")
                  : !candidate.checkable
                    ? t("updates.cannotCheck")
                    : candidate.warnings.length > 0
                      ? t("updates.warnings", { count: candidate.warnings.length })
                      : t("updates.available")
              }
              badgeVariant={
                noticeKey || !candidate.checkable
                  ? "neutral"
                  : candidate.warnings.length > 0
                    ? "warning"
                    : "info"
              }
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
        })}
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
              <div key={artifactKeyId(item.key)} className="flex flex-col gap-1">
                <p className="text-sm font-medium text-[var(--color-foreground)]">
                  {item.key.name}
                </p>
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
