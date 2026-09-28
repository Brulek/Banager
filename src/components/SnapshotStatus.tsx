import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useCheckAgain, useSnapshot } from "../lib/queries";
import { isStartupSnapshot } from "../lib/events";
import { failedSourceNames, hasSourceNotice, namesInSentence } from "../lib/sources";
import { useUiStore } from "../store/ui";
import { EmptyState, type EmptyStateDetail } from "./EmptyState";

export interface SnapshotStatusProps {
  children: ReactNode;
  /**
   * The page says "Checking…" itself while the first check runs -- the
   * Overview's headline -- so `children` are rendered then in place of
   * "Loading…". Every other branch below applies to it as to any page.
   */
  showsFirstCheck?: boolean;
}

export function SnapshotStatus({ children, showsFirstCheck = false }: SnapshotStatusProps) {
  const { t } = useTranslation();
  const snapshotQuery = useSnapshot();
  // Both load-failed states' button is the header's Check again, and off
  // when it is: pressed while a check runs, it would queue a second one.
  const { checkAgain, checking, error: checkError } = useCheckAgain();
  const startupRefreshError = useUiStore((s) => s.startupRefreshError);
  const snapshot = snapshotQuery.data;

  // What Canager works with, and where it looks, behind "Details" on the
  // two states that found nothing. "Not found", never "not installed": a
  // tool with its own installer is looked for in its default location
  // only, so one somewhere else is not found although it is there.
  const supportedList = (title: string): EmptyStateDetail => ({
    label: t("common.details"),
    ariaLabel: t("common.detailsLabel", { title }),
    content: t("emptyStates.supportedList"),
  });

  if (snapshotQuery.isError) {
    // get_snapshot itself failed. InstalledPage renders null without data,
    // so without this branch the user would face a blank page and no way
    // out. The message is the backend's own text (Task 10's call()); a
    // failed check from here replaces it with that check's message. Its
    // button runs the check the header's does, is called what that one
    // is, and is off while a check runs, as that one is.
    return (
      <EmptyState
        title={t("emptyStates.loadFailed.title")}
        description={t("emptyStates.loadFailed.description", {
          message: (checkError ?? snapshotQuery.error).message,
        })}
        action={{ label: t("header.checkAgain"), onClick: checkAgain, disabled: checking }}
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
        action={{ label: t("header.checkAgain"), onClick: checkAgain, disabled: checking }}
      />
    );
  }

  if (isStartupSnapshot(snapshot)) {
    // The startup snapshot: Task 10's useStartupRefresh has not resolved
    // yet, so this is still Snapshot::empty() (`isStartupSnapshot` says
    // why its three fields, and only they, mean that). Judging it here
    // would flash "Canager found nothing it can manage" at every launch. A
    // page that says "Checking…" itself is shown instead of "Loading…".
    return showsFirstCheck ? (
      <>{children}</>
    ) : (
      <p className="p-4 text-sm text-[var(--color-muted)]">{t("common.loading")}</p>
    );
  }

  if (snapshot.detect === "Missing") {
    // `detect` is Missing only when *every* adapter's detect() came back
    // with no instances -- every source, not Homebrew alone, which is what
    // the old `noHomebrew` copy claimed.
    return (
      <EmptyState
        title={t("emptyStates.noSources.title")}
        description={t("emptyStates.noSources.description")}
        detail={supportedList(t("emptyStates.noSources.title"))}
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
    // identity, and the banner below therefore always has a source to
    // name. A source that
    // is merely unavailable is not stale and gets no banner -- it says so
    // itself, in its own words, through its own `SourceNotice` on this
    // page and on the Updates page.
    //
    // It names the sources whose check did not finish (`failedSourceNames`)
    // -- "2 checks didn't finish" said neither which nor what that meant --
    // and says only that what is shown for them was not refreshed: their
    // rows may be last round's, or, on the first check since Canager
    // opened, none. It has no button: the header's Check again, right
    // above it, runs the same check (`useCheckAgain`), and says so when
    // that check fails.
    //
    // The banner variant is meant to "sit above still-visible content"
    // without hiding any of it, but `children` (e.g. InstalledPage) sizes
    // itself with `h-full` — 100% of the nearest ancestor with a definite
    // height, which is the page's box under the header in App.tsx, not this
    // banner's sibling slot. Stacked as plain siblings in that box, the
    // banner's own height plus `children`'s 100%-of-the-box height would
    // overflow it, forcing an extra scroll to reach content that would
    // otherwise be fully visible. Constraining both to a local `h-full` flex
    // column — banner sized to its own content, `children` wrapped in the
    // remaining `flex-1 min-h-0` space with its own scroll — keeps the
    // total height exactly at the box's height, so nothing overflows.
    const failed = failedSourceNames(t, snapshot.errors, snapshot.instances);
    return (
      <div className="flex h-full flex-col overflow-hidden">
        <EmptyState
          variant="banner"
          title={t("emptyStates.refreshFailed.title")}
          description={t("emptyStates.refreshFailed.description", {
            count: failed.length,
            sources: namesInSentence(t, failed),
          })}
        />
        <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>
      </div>
    );
  }

  // Nothing installed *and* nothing any source wants to say. This branch
  // replaces `children` outright, so the second half is load-bearing: the
  // pages show a source's notice line even with nothing of it installed --
  // an Ollama that is installed but not running being the case it was
  // written for. Judging only the global artifact count hid exactly that:
  // on a Mac whose only source is a stopped Ollama, the user saw "Nothing
  // installed yet" and the "Open Ollama" button was unreachable.
  // `hasSourceNotice` lives in lib/sources.ts so this gate and the pages it
  // gates cannot disagree about which sources have something to show.
  if (
    snapshot.artifacts.length === 0 &&
    !snapshot.instances.some((instance) => hasSourceNotice(instance))
  ) {
    return (
      <EmptyState
        title={t("emptyStates.nothingInstalled.title")}
        description={t("emptyStates.nothingInstalled.description")}
        detail={supportedList(t("emptyStates.nothingInstalled.title"))}
      />
    );
  }

  return <>{children}</>;
}
