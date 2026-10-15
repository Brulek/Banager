import { useState, type ReactNode } from "react";
import { ToolbarSlotProvider } from "../components/Toolbar";

/**
 * The box `App`'s toolbar keeps for a page's own controls (`ToolbarItems`),
 * for a test that renders a page alone: the Installed page's sort and
 * search field go there. Nothing else of the toolbar.
 */
export function WithToolbarSlot({ children }: { children: ReactNode }) {
  const [slot, setSlot] = useState<HTMLDivElement | null>(null);
  return (
    <>
      <header>
        <div ref={setSlot} data-toolbar-slot="" />
      </header>
      <ToolbarSlotProvider value={slot}>{children}</ToolbarSlotProvider>
    </>
  );
}
