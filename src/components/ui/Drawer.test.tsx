import { useState } from "react";
import { describe, expect, it } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../test/setup";
import { InfoDetail } from "../InfoDetail";
import { Drawer } from "./Drawer";

/** A row's details: a button that opens the drawer, with an ⓘ inside it. */
function Page() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        jq
      </button>
      <Drawer open={open} onOpenChange={setOpen} title="jq" closeLabel="Close">
        <p>
          Installed by Homebrew.{" "}
          <InfoDetail label="Details: Installed by Homebrew.">Homebrew keeps it up to date.</InfoDetail>
        </p>
      </Drawer>
    </>
  );
}

describe("Drawer", () => {
  it("closes an open ⓘ first on Escape, and only that", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Page />);
    const opener = screen.getByRole("button", { name: "jq" });
    await user.click(opener);
    const drawer = await screen.findByRole("dialog", { name: "jq" });

    const info = screen.getByRole("button", { name: "Details: Installed by Homebrew." });
    await user.click(info);
    expect(screen.getByText("Homebrew keeps it up to date.")).toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(screen.queryByText("Homebrew keeps it up to date.")).toBeNull();
    expect(screen.getByRole("dialog", { name: "jq" })).toBe(drawer);
    expect(document.activeElement).toBe(info);

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(opener));
  });
});
