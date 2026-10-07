import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { FollowUpWarning } from "../lib/types";
import { copyStatusText, useCopyCommand } from "../lib/clipboard";
import { displayToken } from "../lib/format";
import { useUiStore } from "../store/ui";
import { namesInSentence } from "../lib/sources";
import { noteText } from "./LogDrawer";
import { CopyButton } from "./CopyButton";
import { CommandCode } from "./PasswordCommand";
import { OutcomeIcon } from "./OutcomeIcon";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";

/**
 * One saved warning, in the dialog's own type: a plain sentence -- and,
 * for a keg-only formula left unlinked after its update (`NoLongerLinked`),
 * the Terminal command that links it back set apart as code, with Copy
 * Command beside a word on whether it worked (as the link-fix sheet's and
 * the password steps'), the sentence before it saying what and why, and
 * the one after it what the command says if a file is in the way. Text
 * only: Banager never runs it here.
 */
function SavedWarning({ note }: { note: FollowUpWarning }) {
  const { t } = useTranslation();
  if ("NoLongerLinked" in note) {
    const { name, commands } = note.NoLongerLinked;
    const command = ["brew", "link", "--formula", "--force", name].map(displayToken);
    return (
      <div className="flex flex-col gap-2">
        <p className="break-words text-body text-foreground">
          {t("kegLinks.logNoLongerLinkedLead", { name, commands: namesInSentence(t, commands, "or") })}
        </p>
        <CommandCode label={t("noAnswer.sheet.commandLabel")} command={command} />
        <div className="flex items-center justify-start">
          <CopyButton text={command.join(" ")} label={t("common.copyCommand")} size="regular" />
        </div>
        <p className="break-words text-small text-muted">{t("kegLinks.logNoLongerLinkedAfter")}</p>
      </div>
    );
  }
  return <p className="break-words text-body text-foreground">{noteText(t, note)}</p>;
}

/**
 * View Log on a 「最近的更新记录」 line whose update worked but whose
 * cleanup or relink after it did not end as planned: this launch's
 * operation log (`LogDrawer`), or, for an update the history kept from
 * before, the warnings it saved, in a dialog headed as the log is -- the
 * tool as its title, 「已更新，有警告」 with the attention sign under it
 * (as `PasswordRecovery` heads a recorded password stop) -- that the full
 * log is gone, then each warning (`SavedWarning`). Copy Log copies them
 * as the log says them, a sentence each.
 */
export function FollowUpWarnings({ warnings, opId, name }: {
  warnings: FollowUpWarning[]; opId: number | null; name: string;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const openLogRun = useUiStore((s) => s.openLogRun);
  const { status: copyStatus, copy } = useCopyCommand();
  const savedId = useId();
  const text = warnings.map((note) => noteText(t, note)).join("\n");
  return <>
    {/* Named by its tool, as an update row's View Log is: the list can hold several. */}
    <button type="button" className={BUTTON.small.grey} aria-label={t("updates.progress.viewLogLabel", { name })} onClick={() => {
      if (opId === null) setOpen(true);
      else openLogRun([opId], opId);
    }}>{t("common.viewLog")}</button>
    <Dialog
      open={open}
      onOpenChange={setOpen}
      title={name}
      subtitle={
        <span className="inline-flex items-start gap-1">
          <OutcomeIcon tone="attention" size={12} className="mt-px" />
          <span className="min-w-0 break-words">{t("followUpWarning.succeeded", { count: warnings.length })}</span>
        </span>
      }
      describedBy={savedId}
      width="log"
      // Copy Log at the foot's left and Done on the right, as the log's.
      footerStart={
        <>
          <button type="button" onClick={() => copy(text)} className={BUTTON.large.grey}>
            {t("operations.copyLog")}
          </button>
          <span role="status" className="text-small text-muted">
            {copyStatusText(t, copyStatus)}
          </span>
        </>
      }
      footer={
        <button type="button" className={BUTTON.large.default} onClick={() => setOpen(false)}>
          {t("common.done")}
        </button>
      }
    >
      <p id={savedId} className="mb-3 break-words text-body text-foreground">
        {t("followUpWarning.saved", { count: warnings.length })}
      </p>
      <div className="flex flex-col gap-3">
        {warnings.map((note, index) => (
          <SavedWarning key={index} note={note} />
        ))}
      </div>
    </Dialog>
  </>;
}
