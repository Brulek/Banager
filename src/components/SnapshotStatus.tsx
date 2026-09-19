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

  if (snapshot.refreshed_at === null && snapshot.errors.length === 0) {
    // The startup snapshot: Task 10's useStartupRefresh has not resolved
    // yet, and `detect` is still Snapshot::empty()'s placeholder `Missing`.
    // Judging it here would flash "Homebrew isn't installed yet" at every
    // launch. A completed refresh always sets refreshed_at (Task 5), even
    // when Homebrew really is missing, so this branch ends on its own.
    return <p className="p-4 text-sm text-[var(--color-muted)]">{t("common.loading")}</p>;
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
    return (
      <>
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
        {children}
      </>
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
