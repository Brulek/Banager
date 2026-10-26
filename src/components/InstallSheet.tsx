import { useEffect, useId, useRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { catalogRefusal, entryName, installTargetOf, ITEM_KEYS, wordsOf } from "../lib/catalog";
import { usePlanInstall, usePlanInstallFound, useSettings, useSubmitOperation } from "../lib/queries";
import { planErrorDetail, planErrorMessage } from "../lib/sources";
import { formatBytes } from "../lib/format";
import { warningLines } from "../lib/warnings";
import { useUiStore } from "../store/ui";
import type { ArtifactKey, CatalogEntry, Found } from "../lib/types";
import { CommandPreview } from "./CommandPreview";
import { Refusal, SheetIcon, SheetLines, SheetPending, SheetText, sheetMeta } from "./SheetParts";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";

export interface InstallSheetProps {
  /** The entry whose Install opened it, or null. */
  entry: CatalogEntry | null;
  /** The search result whose Install opened it, or null (step E). Closed while both are. */
  found?: Found | null;
  /** What it installs' logo (`iconKeyOf`), as its row draws it. */
  iconKey: ArtifactKey | undefined;
  /** For a search result: what it is, in the window's words where it has them (`useTranslatedDescription`). */
  description?: string | null;
  onClose: () => void;
}

/**
 * Install on a Discover row (phase 5 step B): the preview of installing a
 * catalog entry, as an update's or an uninstall's is -- 「要安装“Gemini
 * CLI”吗？」, the tool's logo, Homebrew under it; who needs it and how it
 * is used once installed; that Homebrew checks the download,
 * and installs what the tool needs, maybe updating some of what is
 * installed (said in general: nothing runs to find out which, docs/
 * superpowers/plans/2026-10-26-phase-5-step-b-install.md); what the plan
 * itself says, a cask's password among it; and the command, one click
 * away (`CommandPreview`). Cancel and Install, the default button.
 *
 * It asks for the plan itself (`plan_install`), naming the entry and
 * never a package: Rust works out what installs it, so nothing runs
 * without this preview, and Install submits that plan and nothing else.
 * For a search result (step E) the same, by the result's id
 * (`plan_install_found`), and it says what the result is and that it is
 * not one of the suggestions.
 * Once submitted it closes, and the row and the operation bar take it from
 * there; the check that follows a finished operation says whether it is
 * installed.
 */
export function InstallSheet({ entry, found = null, iconKey, description = null, onClose }: InstallSheetProps) {
  const { t } = useTranslation();
  const { data: settings } = useSettings();
  const planEntry = usePlanInstall();
  const planFound = usePlanInstallFound();
  const planMutation = found !== null ? planFound : planEntry;
  const submitMutation = useSubmitOperation();
  const installRef = useRef<HTMLButtonElement>(null);
  const textId = useId();
  const open = entry !== null || found !== null;
  // What the plan is asked for by: the entry's id, or the result's.
  const id = found?.id ?? entry?.id ?? null;
  const target =
    found !== null
      ? { adapterId: "brew" as const, kind: found.kind, name: found.name }
      : entry === null
        ? null
        : installTargetOf(entry);
  // Homebrew installs it, or Ollama pulls it (the starter model).
  const sourceLabel = t(target?.adapterId === "ollama" ? "adapters.ollama" : "adapters.brew");
  // As the uninstall's: "the sheet as it is open now, for this entry".
  const sessionRef = useRef(0);
  const submitLatch = useRef(false);

  useEffect(() => {
    sessionRef.current += 1;
    submitLatch.current = false;
    planEntry.reset();
    planFound.reset();
    submitMutation.reset();
    if (id !== null) planMutation.mutate(id);
    // The mutations are stable; planned again as it opens for an entry or a result.
  }, [id]);

  // A preview of another entry, from before it was opened again, is not this one's.
  const issued =
    target !== null &&
    planMutation.data?.plan.request.kind === "Install" &&
    planMutation.data.plan.request.artifact_kind === target.kind &&
    planMutation.data.plan.request.name === target.name
      ? planMutation.data
      : undefined;
  const plan = issued?.plan;
  const keys = entry === null || found !== null ? undefined : ITEM_KEYS[entry.id];
  // For the model: the room free, and whether its download would not fit,
  // with a tenth to spare for what Ollama writes beside it.
  const room =
    found === null &&
    entry !== null &&
    typeof entry.install === "object" &&
    "OllamaModel" in entry.install &&
    typeof entry.free_bytes === "number"
      ? {
          free: formatBytes(entry.free_bytes),
          short: entry.free_bytes < entry.install.OllamaModel.download_bytes * 1.1,
        }
      : null;
  const name = found !== null ? found.name : entry === null ? "" : entryName(t, entry.id);

  function refusal(raw: string, framed: "discover.sheet.planError" | "discover.sheet.submitError"): ReactNode {
    // This Mac cannot install it now: said in the row's own terms, alone.
    const state = catalogRefusal(t, raw);
    if (state !== null) return <Refusal text={state} detail={null} detailTitle={state} />;
    const message = planErrorMessage(t, raw, sourceLabel, settings?.show_technical_details ?? false);
    const plain = framed === "discover.sheet.planError" ? "discover.sheet.planErrorPlain" : "discover.sheet.submitErrorPlain";
    const text = message === null ? t(plain) : t(framed, { message });
    return <Refusal text={text} detail={planErrorDetail(t, raw)} detailTitle={text} />;
  }

  function handleConfirm() {
    if (!issued || submitLatch.current) return;
    submitLatch.current = true;
    const session = sessionRef.current;
    submitMutation.mutate(issued.id, {
      onSuccess: (opId) => {
        // The operation bar names it as the row does -- 「Gemma 3」, not
        // `gemma3:4b`; 「Ollama」, not `ollama-app` -- until the check after it
        // lists the tool by its own name.
        useUiStore.getState().rememberOpNames({ [opId]: name });
        if (sessionRef.current !== session) return;
        onClose();
      },
      onError: () => {
        // A plan id is spent once submitted, whatever the answer: a fresh
        // preview, so Install does not send a dead one.
        if (sessionRef.current !== session || id === null) return;
        planMutation.mutate(id);
      },
      onSettled: () => {
        submitLatch.current = false;
      },
    });
  }

  // What the plan says, in the confirmations' words, and a cask's password:
  // the brew adapter marks every cask, though not every one then asks
  // (spec §6: a password is never a surprise).
  const lines =
    plan === undefined
      ? []
      : (() => {
          const said = warningLines(t, plan.warnings);
          return [
            ...said.scope,
            ...said.trash,
            ...said.keep,
            ...said.data,
            ...said.note,
            ...(plan.needs_password ? [{ text: t("commandPreview.needsPassword"), detail: null, caution: false }] : []),
          ];
        })();

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
      title={t("discover.sheet.title", { name })}
      // A question to answer before anything else, as NSAlert's.
      alert
      icon={
        target === null ? undefined : <SheetIcon adapterId={target.adapterId} sourceLabel={sourceLabel} iconKey={iconKey} />
      }
      subtitle={open ? sheetMeta(name, sourceLabel, target?.adapterId === "ollama" ? target.name : null) : undefined}
      describedBy={textId}
      initialFocus={installRef}
      footer={
        <>
          <button type="button" onClick={onClose} className={BUTTON.large.grey}>
            {t("common.cancel")}
          </button>
          <button
            ref={installRef}
            type="button"
            onClick={handleConfirm}
            disabled={!issued || submitMutation.isPending}
            className={BUTTON.large.default}
          >
            {t("discover.install")}
          </button>
        </>
      }
    >
      {/* Who needs it and how it is used once installed: the alert's text.
          A search result: what it is, where anything says, and that it is
          none of the suggestions. */}
      {found !== null && found.catalog_id !== null && ITEM_KEYS[found.catalog_id] !== undefined ? (
        // One of the suggestions, found by searching: its own words.
        <SheetText id={textId}>{t(ITEM_KEYS[found.catalog_id].more)}</SheetText>
      ) : found !== null ? (
        <SheetText id={textId}>
          {description === null
            ? t("discover.search.notSuggested")
            : t("noAnswer.sheet.then", { first: description, then: t("discover.search.notSuggested") })}
        </SheetText>
      ) : keys === undefined || entry === null ? null : (
        <SheetText id={textId}>{t(keys.more, wordsOf(entry))}</SheetText>
      )}
      <SheetText className="mt-3">
        {target?.adapterId === "ollama" && entry !== null
          ? t("discover.sheet.fromOllama", wordsOf(entry))
          : t("noAnswer.sheet.then", { first: t("discover.sheet.from"), then: t("discover.sheet.alsoInstalls") })}
      </SheetText>
      {/* The model's download beside the room there is for it. */}
      {room === null ? null : (
        <div className="mt-3">
          <SheetLines
            lines={[
              room.short
                ? { text: t("discover.sheet.roomShort", { free: room.free }), detail: null, caution: true }
                : { text: t("discover.sheet.room", { free: room.free }), detail: null, caution: false },
            ]}
          />
        </div>
      )}

      {planMutation.isPending ? <SheetPending text={t("discover.sheet.checking")} /> : null}
      {planMutation.isError ? <div className="mt-3">{refusal(planMutation.error.message, "discover.sheet.planError")}</div> : null}
      {submitMutation.isError ? (
        <div className="mt-3">{refusal(submitMutation.error.message, "discover.sheet.submitError")}</div>
      ) : null}

      {plan && lines.length > 0 ? (
        <div className="mt-3">
          <SheetLines lines={lines} />
        </div>
      ) : null}
      {plan && issued ? <CommandPreview plans={[{ id: issued.id, action: plan.action }]} /> : null}
    </Dialog>
  );
}
