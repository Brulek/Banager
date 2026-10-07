import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { usePlanOperation, useSettings, useSnapshot, useSubmitOperation } from "../lib/queries";
import { adapterLabel, instanceLabels, planErrorDetail, planErrorMessage } from "../lib/sources";
import { linkFixesOf, saidNoAnswer } from "../lib/noAnswer";
import { warningText } from "../lib/warnings";
import type { LinkFix, OpRequest, Warning } from "../lib/types";
import { CommandPreview } from "./CommandPreview";
import { Refusal, SheetIcon, SheetLines, SheetPending, SheetSection, SheetText, sheetMeta } from "./SheetParts";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";
import { PopupButton } from "./ui/PopupButton";

/** The `OpKind.Link` request for one offered formula: `brew link --force <name>`. */
export function linkRequest(fix: LinkFix): OpRequest {
  return { kind: "Link", instance_id: fix.key.instance_id, artifact_kind: "Formula", name: fix.key.name };
}

/** Whether a link's preview found files in its way, which Homebrew would not replace. */
function isLinkConflicts(warning: Warning): warning is { LinkConflicts: { paths: string[] } } {
  return typeof warning !== "string" && "LinkConflicts" in warning;
}

export interface LinkFixSheetProps {
  /** The source whose notice's Fix… opened it, or null while it is closed. */
  instanceId: string | null;
  onClose: () => void;
}

/**
 * Fix… on a source whose launcher could not find a program a keg-only
 * Homebrew formula has (src/lib/noAnswer.ts): the preview of `brew link
 * --force <formula>`, as an alert -- 「要链接“node@22”吗？」, the formula's
 * icon, Homebrew and its version under that; what linking it does, in a
 * sentence; with more than one formula that has the program, a popup to
 * choose, newest first and chosen to begin with; the command, one click
 * away (`CommandPreview`); and Cancel and Link, the default button.
 *
 * It plans the operation itself (`OpKind.Link`, through
 * `Session::issue_listed_plan`, which plans only a formula a source's
 * reason offers), so nothing runs without this preview, and Link submits
 * that plan and nothing else. Where the preview found files already in the
 * way (`Warning.LinkConflicts` -- npm's own `npm`, after npm was updated
 * through itself), it says so, with ⚠︎, and Link stays off: Homebrew would
 * link nothing. Once submitted, it closes, and the operation bar takes it
 * from there; the check that follows a finished operation says whether
 * the source answers now.
 */
export function LinkFixSheet({ instanceId, onClose }: LinkFixSheetProps) {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: settings } = useSettings();
  const planMutation = usePlanOperation();
  const submitMutation = useSubmitOperation();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const popupId = useId();
  const textId = useId();
  const open = instanceId !== null;
  const instance = snapshot?.instances.find((candidate) => candidate.id === instanceId);
  const why = instance === undefined ? null : saidNoAnswer(instance);
  const fixes = linkFixesOf(instance);
  const [chosen, setChosen] = useState<string | null>(null);
  const fix = fixes.find((candidate) => candidate.key.name === chosen) ?? fixes[0];
  const labels = useMemo(() => instanceLabels(t, snapshot?.instances ?? []), [t, snapshot]);
  const sourceOf = (id: string, adapterId: string) => labels.get(id) ?? adapterLabel(t, adapterId);
  const source = instance === undefined ? "" : sourceOf(instance.id, instance.adapter_id);
  const homebrew = fix === undefined ? "" : sourceOf(fix.key.instance_id, "brew");
  const request = fix === undefined ? null : linkRequest(fix);
  // As the uninstall's: "the sheet as it is open now, for this formula".
  const sessionRef = useRef(0);
  const submitLatch = useRef(false);

  useEffect(() => {
    sessionRef.current += 1;
    submitLatch.current = false;
    if (open && request !== null) {
      planMutation.mutate(request);
    } else {
      planMutation.reset();
      submitMutation.reset();
    }
    // The mutations are stable; planned again for another formula, or as
    // the sheet opens or closes.
  }, [open, request?.instance_id, request?.name]);

  // Chosen again from the newest each time it opens.
  useEffect(() => {
    if (!open) setChosen(null);
  }, [open]);

  const issued = planMutation.data;
  const plan = issued?.plan;
  const conflicts = plan?.warnings.filter(isLinkConflicts) ?? [];
  // In the confirmations' words for a caution: ⚠︎, then what is in the way.
  const conflictLines = conflicts.map((warning) => ({
    text: warningText(t, warning) ?? "",
    detail: null,
    caution: true,
  }));

  function refusal(raw: string, framed: "noAnswer.sheet.planError" | "noAnswer.sheet.submitError"): ReactNode {
    const message = planErrorMessage(t, raw, homebrew, settings?.show_technical_details ?? false);
    const plain = framed === "noAnswer.sheet.planError" ? "noAnswer.sheet.planErrorPlain" : "noAnswer.sheet.submitErrorPlain";
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
        // preview, so Link does not send a dead one.
        if (sessionRef.current !== session || request === null) return;
        planMutation.mutate(request);
      },
      onSettled: () => {
        submitLatch.current = false;
      },
    });
  }

  const title = fix === undefined ? t("noAnswer.fix") : t("noAnswer.sheet.title", { formula: fix.key.name });
  return (
    <Dialog
      open={open && fix !== undefined}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
      title={title}
      alert
      icon={fix === undefined ? undefined : <SheetIcon adapterId="brew" sourceLabel={homebrew} iconKey={fix.key} />}
      subtitle={fix === undefined ? undefined : sheetMeta(fix.key.name, homebrew, fix.version)}
      describedBy={textId}
      initialFocus={cancelRef}
      footer={
        <>
          <button ref={cancelRef} type="button" onClick={onClose} className={BUTTON.large.grey}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            onClick={handleConfirm}
            disabled={!plan || conflicts.length > 0 || submitMutation.isPending}
            className={BUTTON.large.default}
          >
            {t("noAnswer.sheet.confirm")}
          </button>
        </>
      }
    >
      {fix !== undefined && why !== null ? (
        <SheetText id={textId}>
          {t("noAnswer.sheet.text", { formula: fix.key.name, program: why.missing_program ?? "", source })}
        </SheetText>
      ) : null}

      {fixes.length > 1 && fix !== undefined ? (
        <div className="mt-3 flex items-center justify-between gap-3">
          <label htmlFor={popupId} className="text-body text-foreground">
            {t("noAnswer.sheet.version")}
          </label>
          <PopupButton
            id={popupId}
            value={fix.key.name}
            options={fixes.map((candidate) => ({
              value: candidate.key.name,
              label: `${candidate.key.name} · ${candidate.version}`,
            }))}
            onChange={setChosen}
          />
        </div>
      ) : null}

      {planMutation.isPending ? <SheetPending text={t("noAnswer.sheet.checking")} /> : null}
      {planMutation.isError ? <div className="mt-3">{refusal(planMutation.error.message, "noAnswer.sheet.planError")}</div> : null}
      {submitMutation.isError ? (
        <div className="mt-3">{refusal(submitMutation.error.message, "noAnswer.sheet.submitError")}</div>
      ) : null}

      {plan && issued ? (
        <>
          {conflictLines.length > 0 ? (
            <SheetSection title={t("uninstall.warningsTitle")} titleHidden>
              <SheetLines lines={conflictLines} />
            </SheetSection>
          ) : null}
          <CommandPreview plans={[{ id: issued.id, action: plan.action }]} />
        </>
      ) : null}
    </Dialog>
  );
}

/**
 * Fix… wherever a source's notice is drawn -- the lists' lines, a tool's
 * inspector, the Overview's problems: what opens the sheet, and the sheet,
 * to render beside the notice.
 */
export function useLinkFixSheet(): { openLinkFix: (instanceId: string) => void; linkFixSheet: ReactNode } {
  const [instanceId, setInstanceId] = useState<string | null>(null);
  return {
    openLinkFix: setInstanceId,
    linkFixSheet: <LinkFixSheet instanceId={instanceId} onClose={() => setInstanceId(null)} />,
  };
}
