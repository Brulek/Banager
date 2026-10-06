import { useTranslation } from "react-i18next";
import { exportPathLine } from "../lib/pathLine";
import { CopyButton } from "./CopyButton";
import { GROUP_ROW_TWO_LINES, SMALL_WRAPPING } from "./ui/group";
import { COMMAND_CODE } from "./withCommand";

/**
 * The row under a 「终端找不到它」 line of the details' 「在终端里输入时」
 * group (`CommandsGroup`), for a folder Terminal does not search: what to
 * do, the line that does it -- `export PATH="$HOME/.grok/bin:$PATH"`
 * (`exportPathLine`) -- and Copy Line. The author's decision U15 (a),
 * 2026-10-06: Banager knows the folder exactly, and a line someone adds can
 * be taken out again; it shows and copies the line and never writes it
 * anywhere itself. Nothing for a folder no line can name.
 */
export function PathLineRow({ dir }: { dir: string }) {
  const { t } = useTranslation();
  const line = exportPathLine(dir);
  if (line === null) return null;
  return (
    <li data-path-line="" className={GROUP_ROW_TWO_LINES}>
      <div className="min-w-0 flex-1">
        <p data-path-line-text="" className={`break-words ${SMALL_WRAPPING} text-muted`}>
          {t("pathLine.sentence")}
        </p>
        <p className={`mt-1 break-all ${SMALL_WRAPPING}`}>
          <code className={COMMAND_CODE}>{line}</code>
        </p>
      </div>
      <CopyButton text={line} label={t("pathLine.copy")} ariaLabel={t("pathLine.copyLabel", { dir })} />
    </li>
  );
}
