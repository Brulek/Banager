import { Component, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useNoBrowserContextMenu } from "../lib/contextMenu";
import { BUTTON } from "./ui/controls";

interface RootErrorBoundaryProps {
  children: ReactNode;
  /** What Reload does: loads the page afresh. A test's own in its tests. */
  reload?: () => void;
}

/**
 * What the window shows when drawing the page throws and nothing under it
 * catches the error: one line saying so, and Reload, which loads the page
 * afresh. Without it React takes the whole page down, and the window is
 * left blank. Wraps `App`, in main.tsx.
 *
 * Whatever runs carries on in Rust meanwhile, and the page loaded again
 * finds it there. The question before a quit goes with the page it
 * replaces, which tells Rust to stop asking (src/lib/quit.ts): until
 * Reload, a quit quits at once.
 */
export class RootErrorBoundary extends Component<RootErrorBoundaryProps, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError(): { failed: boolean } {
    // React reports the error itself, with where it came from.
    return { failed: true };
  }

  render() {
    if (!this.state.failed) return this.props.children;
    return <PageFailed reload={this.props.reload ?? (() => window.location.reload())} />;
  }
}

/**
 * The line and Reload, in the middle of the window, which it drags by
 * anything but the button, as it would by an empty title bar.
 */
function PageFailed({ reload }: { reload: () => void }) {
  const { t } = useTranslation();
  useNoBrowserContextMenu();
  return (
    <div
      data-tauri-drag-region="deep"
      className="flex h-screen flex-col items-center justify-center gap-4 bg-[var(--color-content)] px-6 text-[var(--color-foreground)]"
    >
      <p role="alert" className="max-w-[460px] break-words text-center text-body">
        {t("app.failed")}
      </p>
      <button type="button" onClick={reload} className={BUTTON.large.default}>
        {t("app.reload")}
      </button>
    </div>
  );
}
