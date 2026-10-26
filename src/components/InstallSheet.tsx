import { useEffect, useId, useRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { catalogRefusal, entryName, installTargetOf, ITEM_KEYS, wordsOf } from "../lib/catalog";
import { usePlanInstall, useSettings, useSubmitOperation } from "../lib/queries";
import { planErrorDetail, planErrorMessage } from "../lib/sources";
import { warningLines } from "../lib/warnings";
import type { ArtifactKey, CatalogEntry } from "../lib/types";
import { CommandPreview } from "./CommandPreview";
import { Refusal, SheetIcon, SheetLines, SheetPending, SheetText, sheetMeta } from "./SheetParts";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";

export interface InstallSheetProps {
  /** The entry whose Install opened it, or null while it is closed. */
  entry: CatalogEntry | null;
  /** The entry's logo (`iconKeyOf`), as its row draws it. */
  iconKey: ArtifactKey | undefined;
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
 * Once submitted it closes, and the row and the operation bar take it from
 * there; the check that follows a finished operation says whether it is
 * installed.
 */
export function InstallSheet({ entry, iconKey, onClose }: InstallSheetProps) {
  const { t } = useTranslation();
  const { data: settings } = useSettings();
  const planMutation = usePlanInstall();
  const submitMutation = useSubmitOperation();
  const installRef = useRef<HTMLButtonElement>(null);
  const textId = useId();
  const open = entry !== null;
  const id = entry?.id ?? null;
  const target = entry === null ? null : installTargetOf(entry);
  // Homebrew installs it, or Ollama pulls it (the starter model).
  const sourceLabel = t(target?.adapterId === "ollama" ? "adapters.ollama" : "adapters.brew");
  // As the uninstall's: "the sheet as it is open now, for this entry".
  const sessionRef = useRef(0);
  const submitLatch = useRef(false);

  useEffect(() => {
    sessionRef.current += 1;
    submitLatch.current = false;
    if (id !== null) {
      planMutation.mutate(id);
    } else {
      planMutation.reset();
      submitMutation.reset();
    }
    // The mutations are stable; planned again as it opens for an entry.
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
  const keys = id === null ? undefined : ITEM_KEYS[id];
  const name = id === null ? "" : entryName(t, id);

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
      onSuccess: () => {
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
        entry === null || target === null ? undefined : (
          <SheetIcon adapterId={target.adapterId} sourceLabel={sourceLabel} iconKey={iconKey} />
        )
      }
      subtitle={entry === null ? undefined : sheetMeta(name, sourceLabel, target?.adapterId === "ollama" ? target.name : null)}
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
      {/* Who needs it and how it is used once installed: the alert's text. */}
      {keys === undefined || entry === null ? null : <SheetText id={textId}>{t(keys.more, wordsOf(entry))}</SheetText>}
      <SheetText className="mt-3">
        {target?.adapterId === "ollama"
          ? t("discover.sheet.fromOllama", wordsOf(entry!))
          : t("noAnswer.sheet.then", { first: t("discover.sheet.from"), then: t("discover.sheet.alsoInstalls") })}
      </SheetText>

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
