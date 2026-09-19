import { useTranslation } from "react-i18next";
import { displayToken } from "../lib/format";

export interface CommandPreviewProps {
  program: string;
  args: string[];
}

/**
 * The exact command a Plan will run, one token per `displayToken`: a plain
 * `join(" ")` cannot tell `/Users/Alice Smith/bin/brew` apart from a
 * program called `/Users/Alice` with an argument `Smith/bin/brew`, and both
 * destructive paths (updates here, uninstall in Task 14) rely on this
 * component as the operator's only view of what is about to run.
 */
export function CommandPreview({ program, args }: CommandPreviewProps) {
  const { t } = useTranslation();
  return (
    <div>
      <p className="text-xs font-medium uppercase text-[var(--color-muted)]">
        {t("commandPreview.label")}
      </p>
      <code className="mt-1 block overflow-x-auto rounded-md bg-[var(--color-hover)] px-3 py-2 text-xs text-[var(--color-foreground)]">
        {[program, ...args].map(displayToken).join(" ")}
      </code>
    </div>
  );
}
