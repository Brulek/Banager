import { useTranslation } from "react-i18next";
import { READ_ONLY_DETAIL_KEYS } from "../lib/sources";
import type { ManagerInstance } from "../lib/types";

export interface ReadOnlySourceLineProps {
  /** The source the page shows, which Banager can only list (`read_only_reason`). */
  instance: ManagerInstance;
}

/**
 * The Installed page's list header on one read-only source's page -- pip,
 * or an npm whose folder the account cannot write -- in place of the
 * 全选 box (`InstalledSelectionHeader`), which there could tick nothing:
 * why the page is view only, in the words each row's 「仅供查看」 ⓘ says
 * (`READ_ONLY_DETAIL_KEYS`), once over the list, so the reason is not
 * hidden in a row's ⓘ. Where the header was, as quiet as its status text,
 * wrapping onto a second line rather than cut short.
 */
export function ReadOnlySourceLine({ instance }: ReadOnlySourceLineProps) {
  const { t } = useTranslation();
  const reason = instance.read_only_reason;
  if (reason === null) return null;
  return (
    <p
      data-read-only-line=""
      className="flex min-h-7 shrink-0 items-center border-b border-separator px-5 py-1 text-small text-muted"
    >
      {t(READ_ONLY_DETAIL_KEYS[reason])}
    </p>
  );
}
