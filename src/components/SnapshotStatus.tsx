import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useRefresh, useSnapshot } from "../lib/queries";
import { hasSourceNotice } from "../lib/sources";
import { useUiStore } from "../store/ui";
import { EmptyState } from "./EmptyState";

export interface SnapshotStatusProps {
  children: ReactNode;
}

export function SnapshotStatus({ children }: SnapshotStatusProps) {
  const { t } = useTranslation();
  const snapshotQuery = useSnapshot();
  const refreshMutation = useRefresh();
  const startupRefreshError = useUiStore((s) => s.startupRefreshError);
  const snapshot = snapshotQuery.data;

  if (snapshotQuery.isError) {
    // get_snapshot itself failed. InstalledPage renders null without data,
    // so without this branch the user would face a blank page and no way
    // out. The message is the backend's own text (Task 10's call()); a
    // failed retry replaces it with the retry's message.
    return (
      <EmptyState
        title={t("emptyStates.loadFailed.title")}
        description={t("emptyStates.loadFailed.description", {
          message: (refreshMutation.error ?? snapshotQuery.error).message,
        })}
        action={{
          label: t("emptyStates.refreshFailed.retry"),
          onClick: () => refreshMutation.mutate(),
        }}
      />
    );
  }

  if (!snapshot) {
    return <>{children}</>;
  }

  if (snapshot.generation === 0 && startupRefreshError) {
    // The startup refresh (Task 10's useStartupRefresh) resolved to a
    // rejection rather than a Snapshot, so the cached snapshot is still
    // whatever `get_snapshot` returned. Without this branch the app would
    // sit on the loading branch below forever, with no error and no way out.
    //
    // `generation === 0`, not `refreshed_at === null`: a refresh only stamps
    // `refreshed_at` when *every* source answered, so on a Mac with one
    // permanently broken source the timestamp is null for the rest of the
    // machine's life. Gated on that, a single rejected refresh would replace
    // six sources' worth of real data with a full-page error. `generation`
    // is 0 only while the snapshot is still `Snapshot::empty()` -- nothing
    // has ever been committed -- which is exactly when a failed refresh
    // leaves us with nothing to show.
    return (
      <EmptyState
        title={t("emptyStates.loadFailed.title")}
        description={t("emptyStates.loadFailed.description", { message: startupRefreshError })}
        action={{
          label: t("emptyStates.refreshFailed.retry"),
          onClick: () => refreshMutation.mutate(),
        }}
      />
    );
  }

  if (
    snapshot.generation === 0 &&
    snapshot.detect === "Missing" &&
    snapshot.refreshed_at === null
  ) {
    // The startup snapshot: Task 10's useStartupRefresh has not resolved
    // yet, so this is still Snapshot::empty() — generation 0, no
    // `refreshed_at`, no errors, and `detect` at its placeholder `Missing`.
    // Judging it here would flash "Nothing for Canager to manage yet" at
    // every launch.
    //
    // `detect === "Missing"` is what makes this the *placeholder* rather
    // than a real answer: only a completed refresh can report `Found` or
    // `RefusedAsRoot`, so those are never "still loading" no matter what the
    // timestamp says. Matching on the timestamp alone used to swallow the
    // root refusal entirely, and because a process's euid never changes, no
    // later refresh could undo it — the app sat on "Loading…" forever.
    //
    // `generation === 0` and `refreshed_at === null` are both needed, and
    // neither implies the other. `commit()`
    // (crates/canager-core/src/session/refresh.rs) bumps `generation` only
    // when the refresh's *content* differs from the previous snapshot, so a
    // Mac with no package manager at all refreshes successfully and stays at
    // generation 0 forever — only the stamped `refreshed_at` separates
    // "checked, found nothing" from "not checked yet". Conversely
    // `refreshed_at` stays null for the life of a Mac with one permanently
    // broken source, while `generation` climbs. Together they mean what this
    // branch needs: nothing has been committed *and* nothing has been
    // checked.
    return <p className="p-4 text-sm text-[var(--color-muted)]">{t("common.loading")}</p>;
  }

  if (snapshot.refreshed_at === null && snapshot.errors.length > 0) {
    // No refresh has ever had *every* source answer, and the latest one
    // didn't either. `refresh()` (crates/canager-core/src/session/refresh.rs)
    // stamps `refreshed_at` only when nothing went stale, and carries the
    // previous value forward otherwise; a null one therefore means exactly
    // "no complete check has ever happened", and nothing more. The refresh
    // promise resolved (so this isn't a load-failed case) and `detect` may
    // well be "Found" (so this isn't the no-sources case either).
    //
    // This is *not* the generic stale banner below: with no complete check
    // behind it there is no known-good earlier state for the data to be out
    // of date against. What the user has is a partial answer, so the copy
    // says incomplete, not stale.
    //
    // It deliberately no longer says "first". The front end cannot tell a
    // genuine first launch from the thousandth launch of a Mac with one
    // permanently broken source — both carry a null `refreshed_at`, a
    // non-empty `errors` and no way to distinguish them — and `generation`
    // cannot stand in: `errors` takes part in `same_content`, so any refresh
    // that produced errors has already bumped `generation` past 0, making
    // `generation === 0 && errors.length > 0` unreachable. Saying only what
    // is true of both is the honest option. Laid out the same way as the
    // generic stale banner below (a local `h-full` flex column, not plain
    // siblings under `<main>`) so this banner-above-content combination
    // doesn't overflow `<main>` either — see that branch's comment for why.
    return (
      <div className="flex h-full flex-col overflow-hidden">
        <EmptyState
          variant="banner"
          title={t("emptyStates.incompleteCheck.title")}
          description={
            refreshMutation.isError
              ? t("emptyStates.refreshFailed.retryFailed", {
                  message: refreshMutation.error.message,
                })
              : t("emptyStates.incompleteCheck.description", {
                  count: snapshot.errors.length,
                })
          }
          action={{
            label: t("emptyStates.refreshFailed.retry"),
            onClick: () => refreshMutation.mutate(),
          }}
        />
        <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>
      </div>
    );
  }

  if (snapshot.detect === "Missing") {
    // `detect` is Missing only when *every* adapter's detect() came back
    // with no instances -- all seven sources, not Homebrew alone, which is
    // what the old `noHomebrew` copy claimed.
    return (
      <EmptyState
        title={t("emptyStates.noSources.title")}
        description={t("emptyStates.noSources.description")}
      />
    );
  }

  if (snapshot.detect === "RefusedAsRoot") {
    return (
      <EmptyState
        title={t("emptyStates.refusedAsRoot.title")}
        description={t("emptyStates.refusedAsRoot.description")}
      />
    );
  }

  if (snapshot.stale && snapshot.errors.length > 0) {
    // The banner variant is meant to "sit above still-visible content"
    // without hiding any of it, but `children` (e.g. InstalledPage) sizes
    // itself with `h-full` — 100% of the nearest positioned ancestor with a
    // definite height, which is `<main>` in App.tsx, not this banner's
    // sibling slot. Stacked as plain siblings under `<main>`, the banner's
    // own height plus `children`'s 100%-of-`<main>` height would overflow
    // `<main>`'s box, forcing an extra scroll to reach content that would
    // otherwise be fully visible. Constraining both to a local `h-full` flex
    // column — banner sized to its own content, `children` wrapped in the
    // remaining `flex-1 min-h-0` space with its own scroll — keeps the
    // total height exactly at `<main>`'s height, so nothing overflows.
    return (
      <div className="flex h-full flex-col overflow-hidden">
        <EmptyState
          variant="banner"
          title={t("emptyStates.refreshFailed.title")}
          description={
            refreshMutation.isError
              ? t("emptyStates.refreshFailed.retryFailed", {
                  message: refreshMutation.error.message,
                })
              : t("emptyStates.refreshFailed.description", { count: snapshot.errors.length })
          }
          action={{
            label: t("emptyStates.refreshFailed.retry"),
            onClick: () => refreshMutation.mutate(),
          }}
        />
        <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>
      </div>
    );
  }

  // Nothing installed *and* nothing any source wants to say. This branch
  // replaces `children` outright, so the second half is load-bearing:
  // `InstalledPage` deliberately renders a group header and a `SourceNotice`
  // for a source that needs one even with no artifacts under it -- an Ollama
  // that is installed but not running being the case it was written for.
  // Judging only the global artifact count hid exactly that: on a Mac whose
  // only source is a stopped Ollama, the user saw "Nothing installed yet"
  // and the "Open Ollama" button was unreachable. `hasSourceNotice` lives in
  // lib/sources.ts so this gate and the page it gates cannot disagree about
  // which sources have something to show.
  if (
    snapshot.artifacts.length === 0 &&
    !snapshot.instances.some((instance) => hasSourceNotice(instance))
  ) {
    return (
      <EmptyState
        title={t("emptyStates.nothingInstalled.title")}
        description={t("emptyStates.nothingInstalled.description")}
      />
    );
  }

  return <>{children}</>;
}
