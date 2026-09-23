import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useSnapshot, useSettings } from "../lib/queries";
import { useUiStore, artifactKeyId } from "../store/ui";
import { ArtifactRow } from "../components/ArtifactRow";
import { SourceNotices } from "../components/SourceNotices";
import { UninstallDialog } from "../components/UninstallDialog";
import {
  ADAPTER_LABEL_KEYS,
  canWrite,
  isAvailable,
  sourceNoticesFor,
  UNINSTALL_BLOCKED_KEYS,
} from "../lib/sources";
import type { SourceNoticeSpec } from "../lib/sources";
import type { InstalledArtifact, ManagerInstance, OpRequest } from "../lib/types";
import { COMMAND_SLOT, withCommand } from "../components/withCommand";

// A group header on its own is one line. A group header that also carries a
// SourceNotice (a read-only source's explanation, a silent source's
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
      // Everything this source has to say, decided once while the list is
      // built by the one rule both pages share (`sourceNoticesFor`).
      // Carried on the item rather than re-derived at render time because
      // the virtualizer's `estimateSize` needs to know whether this group
      // has a banner and has only the `ListItem` to ask -- recomputing the
      // rule there is how the two would drift.
      notices: SourceNoticeSpec[];
      // Task 4's unverified-version badge. Kept here deliberately: this
      // task edits Task 4's file rather than replacing it.
      unverifiedVersion: string | null;
    }
  | {
      type: "artifact";
      artifact: InstalledArtifact;
      // Whether this row may offer Uninstall: writable *and* answering.
      // Both halves, because they are independent -- a stopped Ollama is
      // perfectly writable, and its rows are on screen only because
      // `refresh` carried them forward from the last time it answered.
      // `Session::issue_plan` enforces the same conjunction in Rust (spec
      // §2.5); this is what stops the button being offered in the first
      // place.
      actionable: boolean;
      // The source this row belongs to, and its name in the user's
      // language: a row the tool will not uninstall
      // (`uninstall_blocked`) says so in a sentence that names the source
      // and gives the command built from this instance's `exe_path`.
      instance: ManagerInstance;
      sourceLabel: string;
    }
  | { type: "toggle"; instanceId: string; hiddenCount: number; expanded: boolean };

export function InstalledPage() {
  const { t } = useTranslation();
  const { data: snapshot, isLoading } = useSnapshot();
  const { data: settings } = useSettings();
  const query = useUiStore((s) => s.query);
  const setQuery = useUiStore((s) => s.setQuery);
  const expandedDependencies = useUiStore((s) => s.expandedDependencies);
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
      const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
      const label = labelKey ? t(labelKey) : instance.adapter_id;
      // A source can need a notice (a read-only source's explanation, a
      // source Canager could not reach) even with nothing installed to
      // list under it -- most visibly an Ollama daemon that is not
      // running and has never been inventoried, whose notice is then the
      // only thing its group has to show.
      //
      // How many rows this group is about to draw is part of what the
      // notice says: "what's listed here is last time's data" is a lie
      // over an empty group, and an empty group is exactly what a silent
      // source has on the first refresh after every launch, because the
      // snapshot is never persisted. The count is the filtered one on
      // purpose -- it describes what is on screen, which is what the
      // sentence is about.
      const notices = sourceNoticesFor(instance, label, artifacts.length);
      if (artifacts.length === 0 && notices.length === 0) continue;
      const actionable = canWrite(instance) && isAvailable(instance);
      result.push({
        type: "group",
        instanceId: instance.id,
        label,
        notices,
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
        result.push({ type: "artifact", artifact, actionable, instance, sourceLabel: label });
      }
      if (dependencies.length > 0) {
        // The toggle row is pushed in *both* states, not only while the
        // group is folded: it used to disappear on expand, which left no
        // way to fold a group back up short of relaunching the app.
        const expanded = expandedDependencies.includes(instance.id);
        if (expanded) {
          for (const artifact of dependencies) {
            result.push({ type: "artifact", artifact, actionable, instance, sourceLabel: label });
          }
        }
        result.push({
          type: "toggle",
          instanceId: instance.id,
          hiddenCount: dependencies.length,
          expanded,
        });
      }
    }
    return result;
  }, [snapshot, query, expandedDependencies, t]);

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => {
      const item = items[index];
      return item?.type === "group" && item.notices.length > 0
        ? NOTICE_GROUP_ESTIMATE
        : ROW_ESTIMATE;
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
                    {/* Every banner this source needs, in one place and
                        from one rule, so the Installed and the Updates
                        page cannot disagree about what a source has to
                        say: which read-only reason applies (pip's advice
                        is not npm's), whether it answered at all, and
                        whether its answer can be trusted. */}
                    <SourceNotices notices={item.notices} />
                  </div>
                ) : item.type === "toggle" ? (
                  <button
                    type="button"
                    onClick={() => toggleDependencies(item.instanceId)}
                    aria-expanded={item.expanded}
                    className="px-4 py-2 text-left text-sm text-[var(--color-accent)]"
                  >
                    {t(
                      item.expanded ? "installed.hideDependencies" : "installed.showDependencies",
                      { count: item.hiddenCount },
                    )}
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
                    // A row the tool will not uninstall says why in place
                    // of its blurb, as a pinned row does on the Updates
                    // page: it is the one thing the user has to read to
                    // understand why there is no Uninstall button.
                    description={
                      item.artifact.uninstall_blocked !== null
                        ? withCommand(
                            t(UNINSTALL_BLOCKED_KEYS[item.artifact.uninstall_blocked].description, {
                              command: COMMAND_SLOT,
                              source: item.sourceLabel,
                            }),
                            UNINSTALL_BLOCKED_KEYS[item.artifact.uninstall_blocked].command(
                              item.artifact.key,
                              item.instance,
                            ),
                          )
                        : (item.artifact.description ?? t("installed.noDescription"))
                    }
                    wrapDescription={item.artifact.uninstall_blocked !== null}
                    badgeText={
                      updatableIds.has(artifactKeyId(item.artifact.key))
                        ? t("installed.updateAvailable")
                        : t("installed.upToDate")
                    }
                    badgeVariant={
                      updatableIds.has(artifactKeyId(item.artifact.key)) ? "info" : "neutral"
                    }
                    // The source's verdict and the package's own: a pinned
                    // Homebrew package is refused by `brew uninstall`
                    // (`UninstallBlocked::Pinned`), and `Session::issue_plan`
                    // refuses it in Rust whatever this page shows.
                    primaryActionLabel={
                      item.actionable && item.artifact.uninstall_blocked === null
                        ? t("installed.uninstall")
                        : undefined
                    }
                    onPrimaryAction={
                      !item.actionable || item.artifact.uninstall_blocked !== null
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
