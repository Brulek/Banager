import type { Ref } from "react";
import { useTranslation } from "react-i18next";
import { copyStatusText, useCopyCommand } from "../lib/clipboard";
import { BUTTON } from "./ui/controls";

/**
 * A grey copy button -- the homepage's 「拷贝链接」, a command folder's
 * 「拷贝路径」, Settings' 「拷贝诊断信息」 -- with its word on whether it
 * worked, 「已拷贝」 or 「无法拷贝」, beside it for a moment
 * (`useCopyCommand`): the house pattern for a button the user can see, as
 * Copy Log and Copy Command say it beside theirs. The page's own word in
 * the toolbar is for a copy made from a row's ⋯ menu, which has no button
 * to stand beside.
 *
 * Each button has its own word, so two in one pane never speak for each
 * other. The word sits before the button, the pane's buttons being at the
 * right, and takes no room until it is said.
 *
 * `text` is what is copied, or a function that builds it at the click --
 * the diagnostic text, which says what the window holds at that moment.
 * Always written from the click itself: a webview may refuse a clipboard
 * write that no click started. Small in a details pane, regular in a
 * Settings row.
 */
export function CopyButton({
  text,
  label,
  ariaLabel,
  size = "small",
  buttonRef,
  data,
}: {
  text: string | (() => string);
  label: string;
  ariaLabel?: string;
  size?: "small" | "regular";
  buttonRef?: Ref<HTMLButtonElement>;
  /** A `data-*` attribute's name for the button, to find it by (`copy-diagnostics`). */
  data?: string;
}) {
  const { t } = useTranslation();
  const { status, copy } = useCopyCommand();
  const dataProps = data === undefined ? {} : { [`data-${data}`]: "" };
  return (
    <span className="flex shrink-0 items-center justify-end gap-2">
      <span role="status" data-copy-status="" className="text-small text-muted empty:hidden">
        {copyStatusText(t, status)}
      </span>
      <button
        ref={buttonRef}
        type="button"
        aria-label={ariaLabel}
        onClick={() => copy(typeof text === "string" ? text : text())}
        className={size === "small" ? BUTTON.small.grey : BUTTON.regular.grey}
        {...dataProps}
      >
        {label}
      </button>
    </span>
  );
}
