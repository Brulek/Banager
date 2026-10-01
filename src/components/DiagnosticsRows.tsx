import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useDiagnosticsReveal, useDiagnosticsText } from "../lib/diagnostics";
import { CopyButton } from "./CopyButton";
import { GROUP_ROW } from "./ui/group";

/**
 * Settings' 「拷贝诊断信息」, in its About group (`SettingsPage`): one row
 * named 「诊断信息」, and on its right first the checkbox that adds each
 * source's tools to the text -- before the button it changes, and off
 * whenever Settings opens: a private tap's or scope's name can say where
 * someone works, so the list goes only where it is asked for -- then the
 * button, which says 「已拷贝」 or 「无法拷贝」 beside itself for a moment,
 * as every copy button does (`CopyButton`). What the text holds is the
 * group's footnote.
 *
 * Help's 「拷贝诊断信息…」 opens Settings on this row: brought into view,
 * its button focused (`useDiagnosticsReveal`), so the copy is always made
 * by a click.
 */
export function DiagnosticsRows() {
  const { t } = useTranslation();
  const build = useDiagnosticsText();
  const reveal = useDiagnosticsReveal((s) => s.reveal);
  const row = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  // Opened by Help's item: the button in view and focused, at the foot of
  // a page that opens at its top.
  useEffect(() => {
    if (!reveal) return;
    row.current?.scrollIntoView?.({ block: "nearest" });
    button.current?.focus();
    useDiagnosticsReveal.setState({ reveal: false });
  }, [reveal]);
  const [includeTools, setIncludeTools] = useState(false);
  const checkboxId = useId();
  return (
    <div ref={row} className={GROUP_ROW}>
      <span className="block min-w-0 text-body text-foreground">{t("diagnostics.label")}</span>
      <div className="flex shrink-0 flex-wrap items-center justify-end gap-x-4 gap-y-1">
        <label htmlFor={checkboxId} className="flex items-center gap-1.5 text-body text-foreground">
          <input
            id={checkboxId}
            type="checkbox"
            checked={includeTools}
            onChange={(event) => setIncludeTools(event.target.checked)}
            className="h-3.5 w-3.5 shrink-0"
          />
          {t("diagnostics.includeTools")}
        </label>
        <CopyButton
          text={() => build(includeTools)}
          label={t("diagnostics.copy")}
          size="regular"
          buttonRef={button}
          data="copy-diagnostics"
        />
      </div>
    </div>
  );
}
