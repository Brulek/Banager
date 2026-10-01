import { useTranslation } from "react-i18next";
import { useCopyCommand } from "../lib/clipboard";
import { BUTTON } from "./ui/controls";

/**
 * A small grey copy button in a details pane -- the homepage's 「拷贝链接」,
 * a command folder's 「拷贝路径」 -- with its word on whether it worked,
 * 「已拷贝」 or 「无法拷贝」, beside it for a moment (`useCopyCommand`): the
 * house pattern for a button the user can see, as Copy Log and Copy
 * Command say it beside theirs. The page's own word in the toolbar is for
 * a copy made from a row's ⋯ menu, which has no button to stand beside.
 *
 * Each button has its own word, so two in one pane never speak for each
 * other. The word sits before the button, the pane's buttons being at the
 * right, and takes no room until it is said.
 */
export function CopyButton({ text, label, ariaLabel }: { text: string; label: string; ariaLabel?: string }) {
  const { t } = useTranslation();
  const { status, copy } = useCopyCommand();
  return (
    <span className="flex shrink-0 items-center justify-end gap-2">
      <span role="status" data-copy-status="" className="text-small text-muted empty:hidden">
        {status === "copied" ? t("common.copied") : status === "failed" ? t("common.copyFailed") : null}
      </span>
      <button type="button" aria-label={ariaLabel} onClick={() => copy(text)} className={BUTTON.small.grey}>
        {label}
      </button>
    </span>
  );
}
