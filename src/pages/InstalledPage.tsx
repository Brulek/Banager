import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useSnapshot, useSettings } from "../lib/queries";
import { useUiStore, artifactKeyId } from "../store/ui";
import { ArtifactRow } from "../components/ArtifactRow";
import { UninstallDialog } from "../components/UninstallDialog";
import type { InstalledArtifact, OpRequest } from "../lib/types";

const ADAPTER_LABEL_KEYS: Record<string, string> = {
  brew: "adapters.brew",
};

type ListItem =
  | { type: "group"; instanceId: string; label: string }
  | { type: "artifact"; artifact: InstalledArtifact }
  | { type: "toggle"; instanceId: string; hiddenCount: number };

export function InstalledPage() {
  const { t } = useTranslation();
  const { data: snapshot, isLoading } = useSnapshot();
  const { data: settings } = useSettings();
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
      const artifacts = byInstance.get(instance.id);
      if (!artifacts || artifacts.length === 0) continue;
      const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
      result.push({
        type: "group",
        instanceId: instance.id,
        label: labelKey ? t(labelKey) : instance.adapter_id,
      });
      const primary = artifacts.filter((a) => a.reason === "Requested");
      const dependencies = artifacts.filter((a) => a.reason !== "Requested");
      for (const artifact of primary) {
        result.push({ type: "artifact", artifact });
      }
      if (dependencies.length > 0) {
        if (showDependencies) {
          for (const artifact of dependencies) {
            result.push({ type: "artifact", artifact });
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
    estimateSize: () => 56,
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
              <div
                key={virtualRow.key}
                data-index={virtualRow.index}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  height: `${virtualRow.size}px`,
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              >
                {item.type === "group" ? (
                  <p className="px-4 py-2 text-xs font-semibold uppercase text-[var(--color-muted)]">
                    {item.label}
                  </p>
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
                      settings?.show_technical_details
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
                    primaryActionLabel={t("installed.uninstall")}
                    onPrimaryAction={() =>
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
