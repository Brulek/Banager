import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useCheckAgain, useSettings, useSnapshot } from "../lib/queries";
import { isStartupSnapshot } from "../lib/events";
import { NOTHING_FOUND_KEYS, nothingFound } from "../lib/sources";
import { useUiStore } from "../store/ui";
import { FAILURE_CAUSE_KEYS, failureCause } from "../lib/failureCause";
import { EmptyState, type EmptyStateDetail } from "./EmptyState";
import { FirstCheck } from "./StatusRing";

export interface SnapshotStatusProps {
  children: ReactNode;
  /**
   * The page shows the first check itself while it runs -- the Overview,
   * which shows `FirstCheck` from before `get_snapshot` answers until its
   * settings are in too -- so `children` are rendered then in its place.
   * Every other branch below applies to it as to any page.
   */
  showsFirstCheck?: boolean;
  /**
   * The page says itself that the check found nothing to show
   * (`nothingFound`) -- the Overview, in its status row, where every other
   * state of it is said (spec R1) -- so `children` are rendered then, not
   * the empty state a list's area gets.
   */
  showsNothingFound?: boolean;
}

export function SnapshotStatus({ children, showsFirstCheck = false, showsNothingFound = false }: SnapshotStatusProps) {
  const { t } = useTranslation();
  const snapshotQuery = useSnapshot();
  // Both load-failed states' button is the header's Check again, and off
  // when it is: pressed while a check runs, it would queue a second one.
  const { checkAgain, checking, error: checkError } = useCheckAgain();
  const startupRefreshError = useUiStore((s) => s.startupRefreshError);
  const snapshot = snapshotQuery.data;
  const { data: settings } = useSettings();
  const technical = settings?.show_technical_details ?? false;

  // What Canager works with, and where it looks, behind "Details" on the
  // two states that found nothing. "Not found", never "not installed": a
  // tool with its own installer is looked for in its default location
  // only, so one somewhere else is not found although it is there.
  // Why loading failed: in a person's words where the message says
  // (`failureCause`, spec R10); else the message itself with "Show
  // technical details" on, as every other raw error is, and without it
  // what to do next.
  const whyFailed = (message: string): string => {
    const cause = failureCause(message);
    if (cause !== null) return t(FAILURE_CAUSE_KEYS[cause].line);
    return technical
      ? t("emptyStates.loadFailed.description", { message })
      : t("emptyStates.loadFailed.nextStep");
  };

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
        description={whyFailed((checkError ?? snapshotQuery.error).message)}
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
    // data and then fails a refresh keeps showing that data, the toolbar
    // saying the check could not finish. Generation 0 is the one case where a
    // failed refresh leaves nothing at all to show.
    return (
      <EmptyState
        title={t("emptyStates.loadFailed.title")}
        description={whyFailed(startupRefreshError)}
        action={{ label: t("header.checkAgain"), onClick: checkAgain, disabled: checking }}
      />
    );
  }

  if (isStartupSnapshot(snapshot)) {
    // The startup snapshot: Task 10's useStartupRefresh has not resolved
    // yet, so this is still Snapshot::empty() (`isStartupSnapshot` says
    // why its three fields, and only they, mean that). Judging it here
    // would flash "Canager found nothing it can manage" at every launch.
    // The first check's spinner and why it takes a while (`FirstCheck`) are
    // shown instead, by the page itself where it draws them. The Updates
    // and Installed pages said a small grey "Loading…" in a corner here,
    // for as long as the first check took.
    return showsFirstCheck ? <>{children}</> : <FirstCheck />;
  }

  // Found nothing: no source at all -- *every* adapter's detect() came
  // back with no instances, not Homebrew's alone, which is what the old
  // `noHomebrew` copy claimed -- or nothing installed and nothing any
  // source, or a check that did not finish, wants to say (`nothingFound`,
  // which says why that second half is load-bearing). In the list's
  // place; the Overview says it in its status row instead.
  const found = nothingFound(t, snapshot);
  if (found !== null && !showsNothingFound) {
    const title = t(NOTHING_FOUND_KEYS[found].title);
    return (
      <EmptyState
        title={title}
        description={t(NOTHING_FOUND_KEYS[found].description)}
        detail={supportedList(title)}
      />
    );
  }

  return <>{children}</>;
}
