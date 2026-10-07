import { useTranslation } from "react-i18next";
import type { SourceNoticeSpec } from "../lib/sources";
import { FAILURE_CAUSE_KEYS } from "../lib/failureCause";
import { CopyButton } from "./CopyButton";

/** Only the runner's bounded, redacted diagnostic reaches this control. */
export function SourceDiagnostic({ notice }: { notice: SourceNoticeSpec }) {
  const { t } = useTranslation();
  if (!notice.diagnostic) return null;
  return (
    <details className="mt-1 text-small text-foreground">
      <summary>{t("sourceDiagnostic.label")}</summary>
      {notice.diagnosticCause ? <p>{t(FAILURE_CAUSE_KEYS[notice.diagnosticCause].line)}</p> : null}
      <p>{t("sourceDiagnostic.next")}</p>
      <pre className="my-2 max-h-48 overflow-auto whitespace-pre-wrap break-words">{notice.diagnostic}</pre>
      <CopyButton text={notice.diagnostic} label={t("sourceDiagnostic.copy")} />
    </details>
  );
}
