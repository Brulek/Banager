import { Fragment, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { exportPathLine } from "../lib/pathLine";
import { CopyButton } from "./CopyButton";
import { SMALL_WRAPPING } from "./ui/group";
import { COMMAND_CODE } from "./withCommand";

/**
 * The row under a 「终端找不到它」 line of the details' 「在终端里输入时」
 * group (`CommandsGroup`), for a folder Terminal does not search: what to
 * do, with Copy Line beside it, and under both, across the group, the line
 * that does it -- `export PATH="$HOME/.grok/bin:$PATH"` (`exportPathLine`).
 * The author's decision U15 (a), 2026-10-06: Banager knows the folder
 * exactly, and a line someone adds can be taken out again; it shows and
 * copies the line and never writes it anywhere itself. It also says to
 * reopen the app to see the change here: the login shell's `PATH` the
 * group judges by is read once per run (crates/banager-core/src/runner/
 * login_path.rs), so Refresh alone would still say Terminal can't find
 * it. Nothing for a folder no line can name.
 */
export function PathLineRow({ dir }: { dir: string }) {
  const { t } = useTranslation();
  const line = exportPathLine(dir);
  if (line === null) return null;
  return (
    <li data-path-line="" className="px-2.5 py-2">
      <div className="flex items-center justify-between gap-4">
        <p data-path-line-text="" className={`min-w-0 flex-1 break-words ${SMALL_WRAPPING} text-muted`}>
          {t("pathLine.sentence")}
        </p>
        <CopyButton text={line} label={t("pathLine.copy")} ariaLabel={t("pathLine.copyLabel", { dir })} />
      </div>
      {/* The whole width of the group, so the line breaks only where it must. */}
      <p className={`mt-1.5 break-words ${SMALL_WRAPPING}`}>
        <code className={COMMAND_CODE}>{breakable(line)}</code>
      </p>
    </li>
  );
}

/**
 * `line` for a narrow pane: `export PATH="` held on one line, and the
 * folder free to break only after a `/` -- never at the space after
 * `export`, which would read as two lines to type. A `<wbr>` is not text,
 * so selecting the line copies it as it is.
 */
function breakable(line: string): ReactNode {
  const head = 'export PATH="';
  const rest = line.startsWith(head) ? line.slice(head.length) : line;
  const parts = rest.split(/(?<=\/)/);
  return (
    <>
      {rest === line ? null : <span className="whitespace-nowrap">{head}</span>}
      {parts.map((part, index) => (
        <Fragment key={index}>
          {index > 0 ? <wbr /> : null}
          {part}
        </Fragment>
      ))}
    </>
  );
}
