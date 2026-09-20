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
import { READ_ONLY_ADAPTER_IDS } from "../lib/sources";
import { ArtifactRow } from "../components/ArtifactRow";
import { CommandPreview } from "../components/CommandPreview";
import { Dialog } from "../components/ui/Dialog";
import type {
  ArtifactKey,
  InstalledArtifact,
  IssuedPlan,
  OpRequest,
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
  // `instance_id`, joined back to the snapshot's instances.
  const readOnlyInstanceIds = useMemo(
    () =>
      new Set(
        (snapshot?.instances ?? [])
          .filter((instance) => READ_ONLY_ADAPTER_IDS.has(instance.adapter_id))
          .map((instance) => instance.id),
      ),
    [snapshot],
  );

  /**
   * Whether this row may offer an Update button and a checkbox. Two
   * independent reasons it may not, and the wire carries only one of them:
   *
   * - `checkable: false` -- the adapter could not establish what the remote
   *   version is.
   * - a read-only source -- pip's `plan()` refuses every operation, yet its
   *   candidates are built with `checkable: true` because pip genuinely
   *   *can* check. Offering Update here produced nothing but a raw
   *   "unsupported: pip is read-only in Canager" string in a dialog.
   *
   * Stopgap: the proper fix is a signal on the wire (an `actionable` flag,
   * or `Capabilities` surfaced over IPC) rather than the front end knowing
   * which adapter ids are read-only. That is a later wave; until then this
   * shares one list with the Installed page (src/lib/sources.ts) so the two
   * pages cannot disagree about it.
   */
  const isActionable = (candidate: UpdateCandidate): boolean =>
    candidate.checkable && !readOnlyInstanceIds.has(candidate.key.instance_id);

  const isReadOnly = (candidate: UpdateCandidate): boolean =>
    readOnlyInstanceIds.has(candidate.key.instance_id);

  // Counting a row the user cannot act on would promise work that does not
  // exist: "3 updates available" with one Update button under it.
  const actionableCount = visibleUpdates.filter((u) => !isReadOnly(u)).length;

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
          !readOnlyInstanceIds.has(u.key.instance_id) &&
          selectedUpdates.includes(artifactKeyId(u.key)),
      ),
    [visibleUpdates, selectedUpdates, readOnlyInstanceIds],
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
  // some and every one is on the ignore list. Only the first means the
  // machine is up to date.
  if (snapshot.updates.length === 0) {
    return <p className="p-4 text-sm text-[var(--color-muted)]">{t("updates.upToDate")}</p>;
  }
  if (visibleUpdates.length === 0) {
    return <p className="p-4 text-sm text-[var(--color-muted)]">{t("updates.allIgnored")}</p>;
  }

  const dialogOpen = batch !== null && batch.phase !== "planning" && hasIssuedPlan(batch);
  const submitting = batch?.phase === "submitting";
  // Every plan failed: there is nothing to confirm, so the reasons go on the
  // page rather than into an empty dialog. Cleared by the next batch.
  const pageErrors =
    batch !== null && batch.phase === "done" && !hasIssuedPlan(batch) ? batch.items : [];

  return (
    <div className="flex h-full flex-col">
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
        <p className="text-sm text-[var(--color-muted)]">
          {t("updates.count", { count: actionableCount })}
        </p>
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
        {visibleUpdates.map((candidate) => (
          <ArtifactRow
            key={artifactKeyId(candidate.key)}
            name={candidate.key.name}
            // `checkable: false` means the adapter could not establish what
            // the remote version is -- a cargo crate installed from git or a
            // path, an Ollama model whose manifest could not be read, a pipx
            // tool whose PyPI lookup failed. Such a row must offer no action
            // and no selection: "Update" on a git-sourced crate would run
            // `cargo install --force {name}` against the crates.io crate of
            // the same name, which is a different package. The reason lives
            // in `warnings`, so it becomes the row's description.
            description={
              isReadOnly(candidate)
                ? // The same guidance the Installed page's pip group carries:
                  // Canager can see it, cannot touch it, and pipx or uv is
                  // the way to get a Python tool it can manage.
                  t("sourceNotice.pipReadOnly.description")
                : candidate.checkable
                  ? descriptionFor(candidate)
                  : candidate.warnings.join(" ")
            }
            badgeText={
              isReadOnly(candidate)
                ? t("updates.readOnly")
                : !candidate.checkable
                  ? t("updates.cannotCheck")
                  : candidate.warnings.length > 0
                    ? t("updates.warnings", { count: candidate.warnings.length })
                    : t("updates.available")
            }
            badgeVariant={
              isReadOnly(candidate) || !candidate.checkable
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
        ))}
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
          {(batch?.items ?? []).map((item) => (
            <div key={artifactKeyId(item.key)} className="flex flex-col gap-1">
              <p className="text-sm font-medium text-[var(--color-foreground)]">{item.key.name}</p>
              {item.planError !== null ? (
                <p role="alert" className="text-sm text-[var(--color-danger)]">
                  {t("updates.planFailed", { message: item.planError })}
                </p>
              ) : null}
              {item.issued !== null ? (
                <CommandPreview program={item.issued.plan.program} args={item.issued.plan.args} />
              ) : null}
              {item.issued && item.issued.plan.warnings.length > 0 ? (
                <ul className="list-disc pl-5 text-sm text-[var(--color-foreground)]">
                  {item.issued.plan.warnings.map((warning) => (
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
          ))}
        </div>
      </Dialog>
    </div>
  );
}
