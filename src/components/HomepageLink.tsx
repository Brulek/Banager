import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { openHomepage } from "../lib/api";
import { SHOWN_FOR_MS } from "../lib/clipboard";
import { LINK } from "./ui/controls";

/**
 * A tool's homepage in the Installed page's details (`homepageFact`), as a
 * link: the site's host, in the accent as a link in a Mac window's text
 * (`LINK`), the whole address its tooltip. A click has the default browser
 * open the whole address, through Banager's own `open_homepage`
 * (`openHomepage`; src-tauri/src/homepage.rs), which opens only a homepage
 * the current snapshot lists for a tool -- the window never leaves
 * Banager's page.
 *
 * A button that says it is a link (`role="link"`), not an `<a href>`: the
 * web view would follow an address, or offer to open it in a window of its
 * own, and neither is the browser. When the browser was not asked -- the
 * snapshot has moved on, or macOS refused -- 「无法打开」 stands before it for
 * a moment (`SHOWN_FOR_MS`), as 「无法拷贝」 stands before 「拷贝链接」.
 */
export function HomepageLink({ address, host }: { address: string; host: string }) {
  const { t } = useTranslation();
  const [failed, setFailed] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const open = () => {
    window.clearTimeout(timer.current);
    setFailed(false);
    openHomepage(address).catch(() => {
      setFailed(true);
      timer.current = window.setTimeout(() => setFailed(false), SHOWN_FOR_MS);
    });
  };
  return (
    <span className="flex max-w-full flex-wrap items-baseline justify-end gap-x-2">
      <span role="status" data-open-status="" className="text-small text-muted empty:hidden">
        {failed ? t("homepageLink.openFailed") : null}
      </span>
      <button
        type="button"
        role="link"
        title={address}
        data-homepage=""
        onClick={open}
        className={`${LINK} max-w-full break-words text-right`}
      >
        {host}
      </button>
    </span>
  );
}
