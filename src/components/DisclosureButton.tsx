import type { ReactNode } from "react";
import { DisclosureIcon } from "./icons";

/**
 * A disclosure row, as a Mac list's (spec §3.6): a 10 triangle, turned
 * down while open, and the words in 13 muted -- an uninstall's commands,
 * a batch's commands and paths, Homebrew's caveats. `panelId` is what it
 * opens, named while that is shown. `describedBy`, the id of what it is
 * about, where its words alone are said once for each of several: a
 * source's 「查看错误详情」 on the Overview, described by its row's title.
 */
export function DisclosureButton({
  open,
  panelId,
  onToggle,
  describedBy,
  children,
}: {
  open: boolean;
  panelId: string;
  onToggle: () => void;
  describedBy?: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-expanded={open}
      aria-controls={open ? panelId : undefined}
      aria-describedby={describedBy}
      onClick={onToggle}
      className="-ml-1 flex h-7 items-center gap-1.5 rounded-control px-1 text-body text-muted"
    >
      <DisclosureIcon size={10} className={`shrink-0 ${open ? "rotate-90" : ""}`} />
      {children}
    </button>
  );
}
