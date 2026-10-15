import { useTranslation } from "react-i18next";
import type { OutcomeTone } from "../lib/operations";
import { CheckIcon, DashIcon, WarningIcon } from "./icons";

export interface OutcomeIconProps {
  tone: OutcomeTone;
  size?: number;
  className?: string;
}

/**
 * How an operation ended, at a glance, before the words that say it: a
 * tick for a success, a dash for a cancel, a warning sign for anything to
 * look at -- in the danger colour when it failed. The sentences for an
 * outcome that needs attention no longer open with 「需要留意：」 (the
 * copy table's C2): the sign stands for it, and says it to a screen reader
 * in the words the Overview uses for the same thing.
 */
export function OutcomeIcon({ tone, size = 15, className = "" }: OutcomeIconProps) {
  const { t } = useTranslation();
  switch (tone) {
    case "success":
      return <CheckIcon size={size} className={`shrink-0 text-success ${className}`} />;
    case "cancelled":
      return <DashIcon size={size} className={`shrink-0 text-muted ${className}`} />;
    case "failure":
      return <WarningIcon size={size} className={`shrink-0 text-danger ${className}`} />;
    case "attention":
      return (
        <span role="img" aria-label={t("overview.attentionLabel")} className={`inline-flex shrink-0 ${className}`}>
          <WarningIcon size={size} className="text-warning" />
        </span>
      );
  }
}
