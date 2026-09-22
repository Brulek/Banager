import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useSnapshot, useSettings, useOpenOllamaApp } from "../lib/queries";
import { useUiStore, artifactKeyId } from "../store/ui";
import { ArtifactRow } from "../components/ArtifactRow";
import { SourceNotice } from "../components/SourceNotice";
import { UninstallDialog } from "../components/UninstallDialog";
import {
  ADAPTER_LABEL_KEYS,
  READ_ONLY_NOTICE_KEYS,
  canWrite,
  hasSourceNotice,
} from "../lib/sources";
import type { InstalledArtifact, OpRequest, ReadOnlyReason } from "../lib/types";

// A group header on its own is one line. A group header that also carries a
// SourceNotice (a read-only source's explanation, an unhealthy source's
// can't-reach-it warning) is a title plus a banner -- a title line, a
// description line and, for Ollama, a button.
// Both numbers are only the virtualizer's first guess: every row reports its
// real height through `measureElement` as soon as it is in the DOM.
const ROW_ESTIMATE = 56;
const NOTICE_GROUP_ESTIMATE = 120;

type ListItem =
  | {
      type: "group";
      instanceId: string;
      label: string;
      adapterId: string;
      healthy: boolean;
      // Why this source cannot be changed, or null when it can. Carried on
      // the item rather than re-derived at render time so the notice, the
      // row's Uninstall button and the virtualizer's height estimate all
      // answer from the same value.
      readOnlyReason: ReadOnlyReason | null;
      // `hasSourceNotice(instance)`, decided once while the list is built.
      // The virtualizer's `estimateSize` needs it and has only the
      // `ListItem`, and recomputing the rule there is how the two would
      // drift.
      hasNotice: boolean;
      // Task 4's unverified-version badge. Kept here deliberately: this
      // task edits Task 4's file rather than replacing it.
      unverifiedVersion: string | null;
    }
  | { type: "artifact"; artifact: InstalledArtifact; writable: boolean }
  | { type: "toggle"; instanceId: string; hiddenCount: number };

export function InstalledPage() {
  const { t } = useTranslation();
  const { data: snapshot, isLoading } = useSnapshot();
  const { data: settings } = useSettings();
  const openOllamaApp = useOpenOllamaApp();
  const query = useUiStore((s) => s.query);
  const setQuery = useUiStore((s) => s.setQuery);
  const showDependencies = useUiStore((s) => s.showDependencies);
  const toggleDependencies = useUiStore((s) => s.toggleDependencies);
  const setFocusedOpId = useUiStore((s) => s.setFocusedOpId);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const parentRef = useRef<HTMLDivElement>(null);

  // Uninstall is destructive, so the row's button only *targets* an artifact;
  // UninstallDialog is what plans it, shows the exact command and what would
  // break, and submits (Global Constraints, spec §6).
  const [uninstallTarget, setUninstallTarget] = useState<{
    request: OpRequest;
    displayName: string;
  } | null>(null);

  const updatableIds = useMemo(
    () => new Set((snapshot?.updates ?? []).map((u) => artifactKeyId(u.key))),
    [snapshot],
  );

  const items = useMemo<ListItem[]>(() => {
    if (!snapshot) return [];
    const needle = query.trim().toLowerCase();
    const filtered = needle
      ? snapshot.artifacts.filter((a) => a.display_name.toLowerCase().includes(needle))
      : snapshot.artifacts;
    const byInstance = new Map<string, InstalledArtifact[]>();
    for (const artifact of filtered) {
      const list = byInstance.get(artifact.key.instance_id) ?? [];
      list.push(artifact);
      byInstance.set(artifact.key.instance_id, list);
    }
    const result: ListItem[] = [];
    for (const instance of snapshot.instances) {
      const artifacts = byInstance.get(instance.id) ?? [];
      // A source can need a notice (a read-only source's explanation, a
      // source Canager cannot reach) even with nothing installed to list
      // under it -- most visibly, an unhealthy Ollama daemon that has
      // nothing to report yet. An unhealthy instance never reaches the
      // artifact fan-out at all, so its notice is the *only* thing its
      // group ever has to show.
      const needsNotice = hasSourceNotice(instance);
      if (artifacts.length === 0 && !needsNotice) continue;
      const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
      const writable = canWrite(instance);
      result.push({
        type: "group",
        instanceId: instance.id,
        label: labelKey ? t(labelKey) : instance.adapter_id,
        adapterId: instance.adapter_id,
        healthy: instance.healthy,
        readOnlyReason: instance.read_only_reason,
        hasNotice: needsNotice,
        unverifiedVersion: instance.unverified_version,
      });
      // `!== "Dependency"`, not `=== "Requested"`: pip can only ever report
      // Unknown or Dependency (its `--not-required` marks a leaf, which is
      // not the same as "the user asked for it"), so keying off "Requested"
      // would collapse every pip package behind "Show N dependencies" and
      // render the pip group as a header and a notice with no visible rows.
      const primary = artifacts.filter((a) => a.reason !== "Dependency");
      const dependencies = artifacts.filter((a) => a.reason === "Dependency");
      for (const artifact of primary) {
        result.push({ type: "artifact", artifact, writable });
      }
      if (dependencies.length > 0) {
        if (showDependencies) {
          for (const artifact of dependencies) {
            result.push({ type: "artifact", artifact, writable });
          }
        } else {
          result.push({ type: "toggle", instanceId: instance.id, hiddenCount: dependencies.length });
        }
      }
    }
    return result;
  }, [snapshot, query, showDependencies, t]);

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => {
      const item = items[index];
      return item?.type === "group" && item.hasNotice ? NOTICE_GROUP_ESTIMATE : ROW_ESTIMATE;
    },
  });

  if (isLoading) {
    return <p className="p-4 text-sm text-[var(--color-muted)]">{t("common.loading")}</p>;
  }
  if (!snapshot) {
    return null;
  }

  return (
    <div className="flex h-full flex-col">
      <div className="p-4">
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("installed.filterPlaceholder")}
          aria-label={t("installed.filterLabel")}
          className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-background)] px-3 py-2 text-sm"
        />
      </div>
      <div ref={parentRef} className="flex-1 overflow-y-auto">
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((virtualRow) => {
            const item = items[virtualRow.index];
            return (
              // The row reports its own height back to the virtualizer, and
              // carries no fixed one. A group header plus a SourceNotice is
              // far taller than a plain row, so a fixed height would let the
              // banner overflow its slot -- and the next row, later in DOM
              // order and therefore painted on top, would cover its tail,
              // including the Ollama notice's "Open Ollama" button.
              <div
                key={virtualRow.key}
                data-index={virtualRow.index}
                ref={virtualizer.measureElement}
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
                      {item.unverifiedVersion ? (
                        <span className="ml-2 normal-case text-[var(--color-danger)]">
                          {t("installed.unverifiedVersion", { version: item.unverifiedVersion })}
                        </span>
                      ) : null}
                    </p>
                    {/* Two read-only reasons, two pieces of advice. pip
                        cannot be driven at all, so the answer is pipx or
                        uv; an npm with a root-owned prefix works fine and
                        the answer is to reinstall Node with Homebrew.
                        `READ_ONLY_NOTICE_KEYS` keeps the pairing in one
                        place, shared with the Updates page. */}
                    {item.readOnlyReason ? (
                      <SourceNotice
                        variant="info"
                        title={t(`${READ_ONLY_NOTICE_KEYS[item.readOnlyReason]}.title`)}
                        description={t(
                          `${READ_ONLY_NOTICE_KEYS[item.readOnlyReason]}.description`,
                        )}
                      />
                    ) : null}
                    {/* An unhealthy instance means the same thing for every
                        adapter: the CLI is there but Canager could not talk
                        to it. Ollama is the one source the user can do
                        something about from here, so it keeps its own copy
                        and its start button; every other source gets the
                        general notice, named through ADAPTER_LABEL_KEYS so
                        it reads in the user's language. */}
                    {!item.healthy ? (
                      item.adapterId === "ollama" ? (
                        <SourceNotice
                          variant="warning"
                          title={t("sourceNotice.ollamaNotRunning.title")}
                          description={t("sourceNotice.ollamaNotRunning.description")}
                          action={{
                            label: t("sourceNotice.ollamaNotRunning.action"),
                            onClick: () => openOllamaApp.mutate(),
                          }}
                        />
                      ) : (
                        <SourceNotice
                          variant="warning"
                          title={t("sourceNotice.unreachable.title", { source: item.label })}
                          description={t("sourceNotice.unreachable.description", {
                            source: item.label,
                          })}
                        />
                      )
                    ) : null}
                  </div>
                ) : item.type === "toggle" ? (
                  <button
                    type="button"
                    onClick={toggleDependencies}
                    className="px-4 py-2 text-left text-sm text-[var(--color-accent)]"
                  >
                    {t("installed.showDependencies", { count: item.hiddenCount })}
                  </button>
                ) : (
                  <ArtifactRow
                    name={
                      // A Model's `version` is the local manifest digest
                      // Ollama's /api/tags reported, not a version number:
                      // appending it rendered every model as
                      // "qwen3:8b · 5642e97495e1a0888838…". No hash goes in
                      // front of this audience, so the suffix is suppressed
                      // for models whatever the setting says; every other
                      // kind still carries its real version.
                      settings?.show_technical_details && item.artifact.key.kind !== "Model"
                        ? t("installed.nameWithVersion", {
                            name: item.artifact.display_name,
                            version: item.artifact.version,
                          })
                        : item.artifact.display_name
                    }
                    description={item.artifact.description ?? t("installed.noDescription")}
                    badgeText={
                      updatableIds.has(artifactKeyId(item.artifact.key))
                        ? t("installed.updateAvailable")
                        : t("installed.upToDate")
                    }
                    badgeVariant={
                      updatableIds.has(artifactKeyId(item.artifact.key)) ? "info" : "neutral"
                    }
                    primaryActionLabel={item.writable ? t("installed.uninstall") : undefined}
                    onPrimaryAction={
                      !item.writable
                        ? undefined
                        : () =>
                            setUninstallTarget({
                              request: {
                                kind: "Uninstall",
                                instance_id: item.artifact.key.instance_id,
                                artifact_kind: item.artifact.key.kind,
                                name: item.artifact.key.name,
                              },
                              displayName: item.artifact.display_name,
                            })
                    }
                  />
                )}
              </div>
            );
          })}
        </div>
      </div>
      {uninstallTarget ? (
        <UninstallDialog
          open
          onOpenChange={(open) => {
            if (!open) setUninstallTarget(null);
          }}
          request={uninstallTarget.request}
          displayName={uninstallTarget.displayName}
          onSubmitted={(opId) => {
            setUninstallTarget(null);
            setFocusedOpId(opId);
            setDrawerOpen(true);
          }}
        />
      ) : null}
    </div>
  );
}
