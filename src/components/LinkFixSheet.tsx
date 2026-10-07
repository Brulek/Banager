import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useCheckAgain, usePlanOperation, useSettings, useSnapshot, useSubmitOperation } from "../lib/queries";
import { adapterLabel, instanceLabels, instanceNames, namesInSentence, planErrorDetail, planErrorMessage } from "../lib/sources";
import { linkFixesOf, saidNoAnswer } from "../lib/noAnswer";
import { warningText } from "../lib/warnings";
import { artifactKeyId } from "../store/ui";
import { displayToken } from "../lib/format";
import type { LinkFix, OpRequest, Plan, Warning } from "../lib/types";
import { CommandPreview, unbrokenTokens } from "./CommandPreview";
import { CopyButton } from "./CopyButton";
import { Refusal, SheetIcon, SheetLines, SheetPending, SheetSection, SheetText, sheetMeta } from "./SheetParts";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";
import { PopupButton } from "./ui/PopupButton";

/** The `OpKind.Link` request for one offered formula: `brew link --formula --force <name>`. */
export function linkRequest(fix: LinkFix): OpRequest {
  return { kind: "Link", instance_id: fix.key.instance_id, artifact_kind: "Formula", name: fix.key.name };
}

/** Whether a link's preview found files in its way, which Homebrew would not replace. */
function isLinkConflicts(warning: Warning): warning is { LinkConflicts: { paths: string[] } } {
  return typeof warning !== "string" && "LinkConflicts" in warning;
}

/**
 * Whether a link's preview found Homebrew's own links already there, its
 * link not recorded: a link that stopped would take them back (r11 F2).
 */
function isLinkRollbackRisk(warning: Warning): warning is { LinkRollbackRisk: { paths: string[] } } {
  return typeof warning !== "string" && "LinkRollbackRisk" in warning;
}

/**
 * The commands a link's preview says it links into the Homebrew prefix
 * (`Warning.LinkPutsCommands`), the program the source needed first --
 * 「node、corepack、npm和npx」 -- or none when the preview did not list them.
 */
export function linkedCommands(plan: Plan, program: string | null): string[] {
  const names = plan.warnings.flatMap((warning) =>
    typeof warning !== "string" && "LinkPutsCommands" in warning ? warning.LinkPutsCommands.names : [],
  );
  return program !== null && names.includes(program)
    ? [program, ...names.filter((name) => name !== program)]
    : names;
}

/**
 * What the person can run in Terminal where the sheet offers no Link: the
 * same `brew link --formula --force`, with `--overwrite`, which deletes
 * what is in the way, where there are files in its way
 * (`Warning.LinkConflicts`); without it where only links already there
 * are at risk (`Warning.LinkRollbackRisk`), which `--overwrite` would not
 * keep. Text only: Banager never runs it (docs/what-we-run.md, "Why a
 * source did not answer").
 */
export function overwriteCommand(plan: Plan, formula: string): string {
  return overwriteTokens(plan, formula).join(" ");
}

/** `overwriteCommand`'s tokens, each as `displayToken` spells it. */
function overwriteTokens(plan: Plan, formula: string): string[] {
  const program = "Command" in plan.action ? plan.action.Command.program : "brew";
  const overwrite = plan.warnings.some(isLinkConflicts) ? ["--overwrite"] : [];
  return [program, "link", "--formula", "--force", ...overwrite, formula].map(displayToken);
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
 * icon, Homebrew and its version under that; why, in a sentence, and once
 * the preview has read them, what linking changes: the formula's commands
 * (`Warning.LinkPutsCommands`) are linked into the selected Homebrew's
 * prefix, and Terminal uses them only where that folder is on its `PATH`
 * with no command of the same name ahead of it -- which Banager does not
 * check, so it is said as a condition, never as a promise; the check after
 * the link says whether the source can run. With more than one formula
 * that has the program, a popup to choose, newest first and chosen to
 * begin with, each choice naming its Homebrew once they come from more
 * than one, and chosen by the whole key (`artifactKeyId`); the command,
 * one click away (`CommandPreview`); and Cancel and Link, the default
 * button.
 *
 * It plans the operation itself (`OpKind.Link`, through
 * `Session::issue_listed_plan`, which plans only a formula a source's
 * reason offers), so nothing runs without this preview, and Link submits
 * that plan and nothing else. Once submitted, it closes, and the operation
 * bar takes it from there; the check that follows a finished operation
 * says whether the source answers now.
 *
 * Where the preview found files already in the way (`Warning.LinkConflicts`
 * -- npm's own `npm`, after npm was updated through itself), Homebrew would
 * link nothing, and the sheet becomes what to do instead: 「无法链接
 * “node@22”」, what is in the way with ⚠︎, the Terminal command that
 * deletes it and links (`overwriteCommand`) with Copy Command, and Close
 * and Check Again, for after it ran. No Link, and nothing Banager runs:
 * `--overwrite` deletes files, and is the person's to run. The same where
 * Homebrew's own links are already there, its link not recorded
 * (`Warning.LinkRollbackRisk`): a link that stopped would take them back
 * too, so the sheet names them and the command is without `--overwrite`
 * unless files are also in the way.
 */
export function LinkFixSheet({ instanceId, onClose }: LinkFixSheetProps) {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: settings } = useSettings();
  const planMutation = usePlanOperation();
  const submitMutation = useSubmitOperation();
  const { checkAgain, checking } = useCheckAgain();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const popupId = useId();
  const textId = useId();
  const linesId = useId();
  const open = instanceId !== null;
  const instance = snapshot?.instances.find((candidate) => candidate.id === instanceId);
  const why = instance === undefined ? null : saidNoAnswer(instance);
  const fixes = linkFixesOf(instance);
  const [chosen, setChosen] = useState<string | null>(null);
  const fix = fixes.find((candidate) => artifactKeyId(candidate.key) === chosen) ?? fixes[0];
  const labels = useMemo(() => instanceLabels(t, snapshot?.instances ?? []), [t, snapshot]);
  const sourceOf = (id: string, adapterId: string) => labels.get(id) ?? adapterLabel(t, adapterId);
  const source = instance === undefined ? "" : sourceOf(instance.id, instance.adapter_id);
  const homebrew = fix === undefined ? "" : sourceOf(fix.key.instance_id, "brew");
  const prefix = snapshot?.instances.find((candidate) => candidate.id === fix?.key.instance_id)?.prefix ?? homebrew;
  // Offered by more than one Homebrew: each choice says whose it is, as
  // choosing one also chooses the folder its links go in -- by where it is,
  // as the sidebar says it under "Homebrew" (「Apple silicon」, 「Intel」):
  // the whole 「Homebrew (Apple silicon)」 does not fit beside the label in
  // an alert's width, and is under the title for the one chosen.
  const manyHomebrews = new Set(fixes.map((candidate) => candidate.key.instance_id)).size > 1;
  const names = useMemo(() => instanceNames(t, snapshot?.instances ?? []), [t, snapshot]);
  const placeOf = (id: string) => names.get(id)?.place ?? sourceOf(id, "brew");
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

  // A previous preview cannot describe or submit a newly selected source.
  const planned = planMutation.data;
  const issued =
    planned?.plan.request.instance_id === request?.instance_id &&
    planned?.plan.request.artifact_kind === request?.artifact_kind &&
    planned?.plan.request.name === request?.name &&
    planned?.plan.request.kind === request?.kind
      ? planned
      : undefined;
  const plan = issued?.plan;
  // Files in the way, or links already there that a link which stopped
  // would take back: the sheet says what to do instead.
  const conflicts = plan?.warnings.filter((w) => isLinkConflicts(w) || isLinkRollbackRisk(w)) ?? [];
  const blocked = conflicts.length > 0;
  const handoff = !conflicts.some(isLinkRollbackRisk)
    ? "noAnswer.sheet.blockedText"
    : conflicts.some(isLinkConflicts)
      ? "linkRollback.handoffConflicts"
      : "linkRollback.handoff";
  // How many links already there a link that stopped would take back: the
  // sentence under them says "the link" for one, as the ⚠ line above it
  // does (r21 C10).
  const atRisk = conflicts.reduce((n, w) => n + (isLinkRollbackRisk(w) ? w.LinkRollbackRisk.paths.length : 0), 0);
  const program = why?.missing_program ?? null;
  const commands = plan === undefined ? [] : linkedCommands(plan, program);
  // What linking changes, once the preview has read the formula's commands.
  const puts =
    plan === undefined || fix === undefined
      ? null
      : commands.length > 0
        ? t("noAnswer.sheet.puts", {
            commands: namesInSentence(t, commands),
            prefix,
          })
        : t("noAnswer.sheet.putsUnlisted", { prefix });
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

  const title =
    fix === undefined
      ? t("noAnswer.fix")
      : t(blocked ? "noAnswer.sheet.blockedTitle" : "noAnswer.sheet.title", { formula: fix.key.name });
  const terminal = plan === undefined || fix === undefined ? null : overwriteCommand(plan, fix.key.name);
  const reason =
    fix === undefined ? "" : t("noAnswer.sheet.text", { formula: fix.key.name, program: program ?? "", source });
  // What the sheet says to a screen reader as it changes under the focus
  // (r27 A3): the preview arrives a moment after the alert opens, and
  // where it finds something in the way the title becomes the refusal and
  // the two buttons the focus is among are renamed where they stand --
  // Cancel to Close, Link to Check Again. So the refusal is said, its
  // title and what is in the way, 「无法链接“node@22”：…因此无法链接。」.
  // And where a choice in the version popup turns it back into a question
  // -- node@20, after node@22 could not be linked -- that question, whose
  // default button is Link again. Nothing while it checks, which
  // `SheetPending` says, and nothing as it opens on a question, which the
  // alert's own name says.
  const said =
    plan === undefined || fix === undefined
      ? ""
      : blocked
        ? t("noAnswer.sheet.blockedSaid", {
            title,
            lines: conflictLines
              .map((line) => line.text)
              .reduce((first, then) => t("noAnswer.sheet.then", { first, then })),
          })
        : chosen !== null
          ? title
          : "";
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
      // While it is a refusal, what is in the way too: the reason alone
      // still reads as the question it was.
      describedBy={blocked ? `${textId} ${linesId}` : textId}
      initialFocus={cancelRef}
      footer={
        blocked ? (
          <>
            <button ref={cancelRef} type="button" onClick={onClose} className={BUTTON.large.grey}>
              {t("common.close")}
            </button>
            <button
              type="button"
              onClick={() => {
                checkAgain();
                onClose();
              }}
              disabled={checking}
              className={BUTTON.large.default}
            >
              {t("header.checkAgain")}
            </button>
          </>
        ) : (
          <>
            <button ref={cancelRef} type="button" onClick={onClose} className={BUTTON.large.grey}>
              {t("common.cancel")}
            </button>
            <button
              type="button"
              onClick={handleConfirm}
              disabled={!plan || submitMutation.isPending}
              className={BUTTON.large.default}
            >
              {t("noAnswer.sheet.confirm")}
            </button>
          </>
        )
      }
    >
      {fix !== undefined && why !== null ? (
        <SheetText id={textId}>
          {puts === null ? reason : t("noAnswer.sheet.then", { first: reason, then: puts })}
        </SheetText>
      ) : null}

      {fixes.length > 1 && fix !== undefined ? (
        // With each choice's Homebrew, the label goes above the popup: beside
        // it, in an alert's width, it would break in the middle of a word.
        <div
          className={
            manyHomebrews ? "mt-3 flex flex-col items-start gap-1.5" : "mt-3 flex items-center justify-between gap-3"
          }
        >
          <label htmlFor={popupId} className="text-body text-foreground">
            {t("noAnswer.sheet.version")}
          </label>
          <PopupButton
            id={popupId}
            value={artifactKeyId(fix.key)}
            options={fixes.map((candidate) => ({
              value: artifactKeyId(candidate.key),
              label: manyHomebrews
                ? `${candidate.key.name} · ${candidate.version} · ${placeOf(candidate.key.instance_id)}`
                : `${candidate.key.name} · ${candidate.version}`,
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

      {plan && issued && blocked && terminal !== null && fix !== undefined ? (
        <>
          <SheetSection title={t("uninstall.warningsTitle")} titleHidden>
            <SheetLines id={linesId} lines={conflictLines} />
          </SheetSection>
          {/* What would link it instead, for the person to run: the
              Terminal command, set as code that selects whole, Copy
              Command, and what to do after (as `PasswordCommand`). */}
          <div className="mt-3 flex flex-col gap-2">
            <p className="break-words text-body text-foreground">
              {t(handoff, { formula: fix.key.name, count: atRisk })}
            </p>
            <div role="group" aria-label={t("noAnswer.sheet.commandLabel")}>
              {/* A line breaks between tokens, never inside one: 「--」 /
                  「overwrite」 reads as something else (`unbrokenTokens`). */}
              <code className="block select-all break-words rounded-control bg-group px-2.5 py-2 font-mono text-small text-foreground">
                {plan === undefined ? terminal : unbrokenTokens(overwriteTokens(plan, fix.key.name))}
              </code>
            </div>
            <div className="flex items-center justify-start">
              <CopyButton text={terminal} label={t("common.copyCommand")} size="regular" />
            </div>
            <p className="break-words text-small text-muted">{t("noAnswer.sheet.after")}</p>
          </div>
        </>
      ) : null}
      {plan && issued && !blocked ? <CommandPreview plans={[{ id: issued.id, action: plan.action }]} /> : null}

      {/* `said`, out of sight: what it says is on the sheet already. There
          from the time the sheet opens, empty: a status put in the page
          with its words is one a screen reader may never read (as
          `UninstallDialog`'s). */}
      <p role="status" className="sr-only">
        {said}
      </p>
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
