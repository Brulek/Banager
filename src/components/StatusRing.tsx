import { useTranslation } from "react-i18next";
import { SpinnerIcon } from "./icons";

/**
 * What the Overview, Updates and Installed pages show until the first
 * check since Banager opened has answered, centred in the page, as an
 * empty state is (spec §3.10): a 32 spinner, "Checking…" under it in the
 * 15 title style, and a line saying why that takes a while, no wider than
 * 360. The first check looks up every tool's newest version online, and
 * the snapshot it answers with comes only once every source has answered
 * -- Homebrew's list update alone can hold it for up to two minutes. A
 * spinner with nothing else said looked like a window that had frozen,
 * and the Updates and Installed pages said less still: a small grey
 * "Loading…" in a corner. The Overview draws it itself (`SnapshotStatus`'s
 * `showsFirstCheck`), `SnapshotStatus` for the other two once the startup
 * snapshot is in, and the two pages themselves before it is.
 *
 * (The file keeps the name of the ring the Overview drew here before
 * polish 3; the ring is gone.)
 */
export function FirstCheck() {
  const { t } = useTranslation();
  return (
    <div data-first-check="" className="flex min-h-full flex-col items-center justify-center px-5 py-8 text-center">
      <SpinnerIcon size={32} className="text-muted" />
      <h2 className="mt-6 text-section text-foreground">{t("common.checking")}</h2>
      <p className="mt-2 max-w-90 text-section font-normal text-muted">{t("common.firstCheckDetail")}</p>
    </div>
  );
}
