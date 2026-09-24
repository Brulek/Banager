import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useRefresh, useSnapshot } from "../lib/queries";
import { failedSourceCount, hasSourceNotice } from "../lib/sources";
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
    // `generation === 0`, not `refreshed_at === null`: only `generation`
    // says whether anything has ever been *committed*, and data in hand
    // beats a full-page error. A Mac with no package manager at all
    // refreshes successfully and commits nothing new, so it sits at
    // generation 0 with a stamped timestamp; a Mac that has committed real
    // data and then fails a refresh keeps showing that data, with the
    // stale banner below over it. Generation 0 is the one case where a
    // failed refresh leaves nothing at all to show.
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
    // than a real answer: only a completed refresh can report `Found`, so
    // that is never "still loading" no matter what the timestamp says.
    //
    // `generation === 0` and `refreshed_at === null` are both needed.
    // `commit()` (crates/canager-core/src/session/refresh.rs) bumps
    // `generation` only when the refresh's *content* differs from the
    // previous snapshot, so a Mac with no package manager at all refreshes
    // successfully and stays at generation 0 forever — only the stamped
    // `refreshed_at` separates "checked, found nothing" from "not checked
    // yet". `refresh()` stamps that timestamp whenever it ran, whatever
    // the sources said, so `Snapshot::empty()` is now the only snapshot
    // that can carry a null one: together the two still mean exactly what
    // this branch needs, nothing committed *and* nothing checked.
    return <p className="p-4 text-sm text-[var(--color-muted)]">{t("common.loading")}</p>;
  }

  if (snapshot.detect === "Missing") {
    // `detect` is Missing only when *every* adapter's detect() came back
    // with no instances -- every source, not Homebrew alone, which is what
    // the old `noHomebrew` copy claimed.
    return (
      <EmptyState
        title={t("emptyStates.noSources.title")}
        description={t("emptyStates.noSources.description")}
      />
    );
  }

  if (snapshot.stale) {
    // The only "something went wrong" banner there is. There used to be a
    // second one above, for `refreshed_at === null && errors.length > 0`:
    // "no check has ever finished, and this one didn't either". It is
    // unreachable now that `refresh()` stamps `refreshed_at` whenever it
    // ran (spec §2.4-1) -- only `Snapshot::empty()` carries a null one, and
    // it carries no errors either -- so it and its copy are gone rather
    // than left to rot.
    //
    // `stale` alone, with no `errors.length > 0` beside it: `refresh()`
    // sets `stale` to exactly `!errors.is_empty()`, so the second test was
    // identity, and the count below is therefore never zero. A source that
    // is merely unavailable is not stale and gets no banner -- it says so
    // itself, in its own words, through its own `SourceNotice` on this
    // page and on the Updates page.
    //
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
              : t("emptyStates.refreshFailed.description", {
                  count: failedSourceCount(snapshot.errors),
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
