import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { FollowUpWarning } from "../lib/types";
import { useUiStore } from "../store/ui";
import { noteText } from "./LogDrawer";
import { CopyButton } from "./CopyButton";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";

/** A current log, or the structured warning log retained across launches. */
export function FollowUpWarnings({ warnings, opId, name }: {
  warnings: FollowUpWarning[]; opId: number | null; name: string;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const openLogRun = useUiStore((s) => s.openLogRun);
  const text = warnings.map((note) => noteText(t, note)).join("\n");
  return <>
    <button type="button" className={BUTTON.small.grey} onClick={() => {
      if (opId === null) setOpen(true);
      else openLogRun([opId], opId);
    }}>{t("common.viewLog")}</button>
    <Dialog open={open} onOpenChange={setOpen} title={name} description={t("followUpWarning.saved")}
      footer={<>
        <CopyButton text={text} label={t("operations.copyLog")} />
        <button type="button" className={BUTTON.regular.grey} onClick={() => setOpen(false)}>{t("common.done")}</button>
      </>}>
      <pre className="whitespace-pre-wrap break-words text-body">{text}</pre>
    </Dialog>
  </>;
}
