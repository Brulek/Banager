import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { SourceNoticeSpec } from "../lib/sources";
import { FAILURE_CAUSE_KEYS } from "../lib/failureCause";
import { CopyButton } from "./CopyButton";
import { DisclosureButton } from "./DisclosureButton";

/**
 * What a source said when it did not answer at startup (`NoAnswer::diagnostic`),
 * behind a closed disclosure as Homebrew's caveats are: the cause's line
 * where one was read, the next step, the source's own words and Copy.
 * Only the runner's bounded, redacted diagnostic reaches this control.
 */
export function SourceDiagnostic({ notice }: { notice: SourceNoticeSpec }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const panelId = useId();
  if (!notice.diagnostic) return null;
  return (
    <div className="mt-1">
      <DisclosureButton open={open} panelId={panelId} onToggle={() => setOpen(!open)}>
        {t("sourceDiagnostic.label")}
      </DisclosureButton>
      {open ? (
        <div id={panelId} className="mt-1 flex flex-col gap-1 text-small text-foreground">
          {notice.diagnosticCause ? <p>{t(FAILURE_CAUSE_KEYS[notice.diagnosticCause].line)}</p> : null}
          <p>{t("sourceDiagnostic.next")}</p>
          <p
            data-source-diagnostic=""
            className="max-h-48 overflow-auto whitespace-pre-wrap break-words rounded-control bg-group px-2.5 py-2 text-small text-foreground"
          >
            {notice.diagnostic}
          </p>
          <CopyButton text={notice.diagnostic} label={t("sourceDiagnostic.copy")} />
        </div>
      ) : null}
    </div>
  );
}
