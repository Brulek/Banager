import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useRefresh, useSnapshot } from "../lib/queries";
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

  if (snapshot.refreshed_at === null && startupRefreshError) {
    // The startup refresh (Task 10's useStartupRefresh) resolved to a
    // rejection rather than a Snapshot, so refreshed_at will never be set by
    // it. Without this branch the app would sit on the loading branch below
    // forever, with no error and no way out.
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
    snapshot.detect === "Missing" &&
    snapshot.refreshed_at === null &&
    snapshot.errors.length === 0
  ) {
    // The startup snapshot: Task 10's useStartupRefresh has not resolved
    // yet, so this is still Snapshot::empty() — generation 0, no
    // `refreshed_at`, no errors, and `detect` at its placeholder `Missing`.
    // Judging it here would flash "Homebrew isn't installed yet" at every
    // launch.
    //
    // All three conditions are load-bearing. `detect === "Missing"` is what
    // makes this the *placeholder* rather than a real answer: only a
    // completed refresh can report `Found` or `RefusedAsRoot`, so those are
    // never "still loading" no matter what the timestamp says. Matching on
    // the timestamp alone used to swallow the root refusal entirely, and
    // because a process's euid never changes, no later refresh could undo
    // it — the app sat on "Loading…" forever.
    //
    // The timestamp and error conditions then bound how long this can last.
    // Task 5's `refresh()` (crates/canager-core/src/session/mod.rs) leaves
    // `refreshed_at` null only when it carries the previous value forward,
    // which happens on a per-instance failure — i.e. exactly when `errors`
    // is non-empty. So with `errors.length === 0` a completed refresh always
    // sets `refreshed_at` and this branch ends on its own. A refresh that
    // completes with per-instance errors on the very first attempt
    // (`errors.length > 0`, still no prior `refreshed_at`) is handled by the
    // dedicated branch below instead, rather than falling through to here or
    // to the generic stale banner.
    return <p className="p-4 text-sm text-[var(--color-muted)]">{t("common.loading")}</p>;
  }

  if (snapshot.refreshed_at === null && snapshot.errors.length > 0) {
    // First-ever refresh (previous.refreshed_at was None from
    // Snapshot::empty()) that hit a per-instance error. The refresh promise
    // resolved (so this isn't a load-failed case) and `detect` may well be
    // "Found" (so this isn't the no-Homebrew case either), but there is no
    // prior successful refresh — unlike the generic stale-banner case below,
    // nothing here is actually "out of date"; the first check itself simply
    // didn't finish. Distinguishing the copy avoids implying stale prior
    // data exists on what is, in fact, a first launch. Laid out the same
    // way as the generic stale banner below (a local `h-full` flex column,
    // not plain siblings under `<main>`) so this banner-above-content
    // combination doesn't overflow `<main>` either — see that branch's
    // comment for why.
    return (
      <div className="flex h-full flex-col overflow-hidden">
        <EmptyState
          variant="banner"
          title={t("emptyStates.firstRefreshFailed.title")}
          description={
            refreshMutation.isError
              ? t("emptyStates.refreshFailed.retryFailed", {
                  message: refreshMutation.error.message,
                })
              : t("emptyStates.firstRefreshFailed.description", {
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
    return (
      <EmptyState
        title={t("emptyStates.noHomebrew.title")}
        description={t("emptyStates.noHomebrew.description")}
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

  if (snapshot.artifacts.length === 0) {
    return (
      <EmptyState
        title={t("emptyStates.nothingInstalled.title")}
        description={t("emptyStates.nothingInstalled.description")}
      />
    );
  }

  return <>{children}</>;
}
