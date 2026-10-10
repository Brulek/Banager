import { useState, type ReactNode } from "react";
import { ToolbarSlotProvider } from "../components/Toolbar";
import { useUpdatesHeadline } from "../pages/UpdatesPage";

/**
 * What `App` draws over the Updates page, for a test that renders the page
 * alone: the toolbar's subtitle -- the page's headline, 「10个可更新」
 * (`useUpdatesHeadline`) -- and the box the page puts its own actions in
 * (`ToolbarItems`), Update all and Update selected. Nothing else of the
 * toolbar: its Check Again would be one more button of that name beside a
 * notice's.
 */
export function UpdatesToolbar({ children }: { children: ReactNode }) {
  const [slot, setSlot] = useState<HTMLDivElement | null>(null);
  const headline = useUpdatesHeadline();
  return (
    <>
      <header>
        {headline !== null ? <p data-toolbar-subtitle="">{headline}</p> : null}
        <div ref={setSlot} data-toolbar-slot="" />
      </header>
      <ToolbarSlotProvider value={slot}>{children}</ToolbarSlotProvider>
    </>
  );
}
