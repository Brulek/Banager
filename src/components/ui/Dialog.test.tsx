import { useEffect, useRef, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../test/setup";
import { InfoDetail } from "../InfoDetail";
import { Dialog } from "./Dialog";
import { Menu } from "./Menu";

/**
 * A page with a button that opens a sheet, as the rows' Uninstall and the
 * Updates page's Update all do: not a Radix Dialog.Trigger. `passOpener`
 * hands the button over as `returnFocusTo`; without it, the sheet goes by
 * what had the focus as it opened.
 */
function Page({
  passOpener = true,
  onClosed,
  takeFocusOnClose = false,
}: {
  passOpener?: boolean;
  onClosed?: () => void;
  takeFocusOnClose?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [logOpen, setLogOpen] = useState(false);
  const opener = useRef<HTMLElement | null>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const close = () => {
    setOpen(false);
    if (takeFocusOnClose) setLogOpen(true);
  };
  return (
    <>
      <button
        type="button"
        onClick={(event) => {
          opener.current = event.currentTarget;
          setOpen(true);
        }}
      >
        Uninstall jq
      </button>
      {logOpen ? <Log /> : null}
      <Dialog
        open={open}
        onOpenChange={setOpen}
        title="Uninstall jq?"
        returnFocusTo={passOpener ? opener : undefined}
        initialFocus={cancel}
        onClosed={onClosed}
        footer={
          <>
            <button ref={cancel} type="button" onClick={close}>
              Cancel
            </button>
            <button type="button" onClick={close}>
              Uninstall
            </button>
          </>
        }
      >
        <p>
          Permanently deletes ~/.rustup.{" "}
          <InfoDetail label="Details: Permanently deletes ~/.rustup.">Nothing goes to the Trash.</InfoDetail>
        </p>
      </Dialog>
    </>
  );
}

/** What the log drawer does as it opens: it takes the focus, from an effect. */
function Log() {
  const ref = useRef<HTMLButtonElement>(null);
  useEffect(() => ref.current?.focus(), []);
  return (
    <button ref={ref} type="button">
      Log
    </button>
  );
}

describe("Dialog", () => {
  it("renders the title, children and footer when open", () => {
    const { getByRole, getByText } = renderWithProviders(
      <Dialog
        open
        onOpenChange={vi.fn()}
        title="Confirm"
        footer={<button type="button">OK</button>}
      >
        <p>Body content</p>
      </Dialog>,
    );

    expect(getByRole("dialog", { name: "Confirm" })).toBeInTheDocument();
    expect(getByText("Body content")).toBeInTheDocument();
    expect(getByRole("button", { name: "OK" })).toBeInTheDocument();
  });

  it("does not render when closed", () => {
    const { queryByRole } = renderWithProviders(
      <Dialog open={false} onOpenChange={vi.fn()} title="Confirm">
        <p>Body content</p>
      </Dialog>,
    );

    expect(queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("renders without a footer when none is passed", () => {
    const { getByRole, queryByRole } = renderWithProviders(
      <Dialog open onOpenChange={vi.fn()} title="Confirm">
        <p>Body content</p>
      </Dialog>,
    );

    expect(getByRole("dialog")).toBeInTheDocument();
    expect(queryByRole("button")).not.toBeInTheDocument();
  });

  it("puts the focus where it is told to as it opens", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Page />);

    await user.click(screen.getByRole("button", { name: "Uninstall jq" }));

    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cancel" })));
  });

  it.each([
    ["Cancel", async (user: ReturnType<typeof userEvent.setup>) => user.click(screen.getByRole("button", { name: "Cancel" }))],
    ["Escape", async (user: ReturnType<typeof userEvent.setup>) => user.keyboard("{Escape}")],
    [
      "the button it asks for",
      async (user: ReturnType<typeof userEvent.setup>) =>
        user.click(screen.getByRole("button", { name: "Uninstall" })),
    ],
  ])("gives the focus back to the button that opened it, closed with %s", async (_how, closeIt) => {
    // Radix gives it back only to a Dialog.Trigger, which this button is not.
    const user = userEvent.setup();
    renderWithProviders(<Page />);
    const opener = screen.getByRole("button", { name: "Uninstall jq" });

    await user.click(opener);
    await screen.findByRole("dialog");
    await closeIt(user);

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(opener));
  });

  it("goes by what had the focus as it opened when it is not told what opened it", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Page passOpener={false} />);
    const opener = screen.getByRole("button", { name: "Uninstall jq" });

    await user.click(opener);
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");

    await waitFor(() => expect(document.activeElement).toBe(opener));
  });

  it("leaves the focus with whatever took it as the sheet closed", async () => {
    // The Installed page's log drawer opens as an uninstall starts; the
    // sheet does not pull the focus out of it.
    const user = userEvent.setup();
    const onClosed = vi.fn();
    renderWithProviders(<Page takeFocusOnClose onClosed={onClosed} />);

    await user.click(screen.getByRole("button", { name: "Uninstall jq" }));
    await screen.findByRole("dialog");
    await user.click(screen.getByRole("button", { name: "Uninstall" }));

    await waitFor(() => expect(onClosed).toHaveBeenCalledTimes(1));
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Log" }));
  });

  it("says it has closed only once the focus is back", async () => {
    const user = userEvent.setup();
    const opener = () => screen.getByRole("button", { name: "Uninstall jq" });
    const focusedWhenClosed: Array<Element | null> = [];
    renderWithProviders(<Page onClosed={() => focusedWhenClosed.push(document.activeElement)} />);

    await user.click(opener());
    await screen.findByRole("dialog");
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(focusedWhenClosed).toEqual([opener()]));
  });

  it("gives the focus back to the ⋯ button when one of its menu's items opened it", async () => {
    function MenuPage() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <Menu label="More actions for jq" items={[{ id: "uninstall", label: "Uninstall…", onSelect: () => setOpen(true) }]} />
          <Dialog open={open} onOpenChange={setOpen} title="Uninstall jq?">
            <button type="button" onClick={() => setOpen(false)}>
              Cancel
            </button>
          </Dialog>
        </>
      );
    }
    const user = userEvent.setup();
    renderWithProviders(<MenuPage />);
    const more = screen.getByRole("button", { name: "More actions for jq" });

    await user.click(more);
    await user.click(screen.getByRole("menuitem", { name: "Uninstall…" }));
    await screen.findByRole("dialog");
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(more));
  });

  it("closes an open ⓘ first on Escape, and only that", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Page />);
    const opener = screen.getByRole("button", { name: "Uninstall jq" });
    await user.click(opener);
    const dialog = await screen.findByRole("dialog");

    const info = screen.getByRole("button", { name: "Details: Permanently deletes ~/.rustup." });
    await user.click(info);
    expect(screen.getByText("Nothing goes to the Trash.")).toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(screen.queryByText("Nothing goes to the Trash.")).toBeNull();
    expect(screen.getByRole("dialog")).toBe(dialog);
    expect(document.activeElement).toBe(info);

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(opener));
  });

  it("does not give the focus to an opener that is gone from the page", async () => {
    function Vanishing() {
      const [open, setOpen] = useState(false);
      const [shown, setShown] = useState(true);
      const opener = useRef<HTMLElement | null>(null);
      return (
        <>
          {shown ? (
            <button
              type="button"
              onClick={(event) => {
                opener.current = event.currentTarget;
                setOpen(true);
              }}
            >
              Update glib
            </button>
          ) : null}
          <Dialog open={open} onOpenChange={setOpen} title="Update glib?" returnFocusTo={opener}>
            <button
              type="button"
              onClick={() => {
                // The row's button gives way to its progress as the update starts.
                setShown(false);
                setOpen(false);
              }}
            >
              Update
            </button>
          </Dialog>
        </>
      );
    }
    const user = userEvent.setup();
    renderWithProviders(<Vanishing />);

    await user.click(screen.getByRole("button", { name: "Update glib" }));
    await screen.findByRole("dialog");
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Update" }));
    });

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.queryByRole("button", { name: "Update glib" })).toBeNull();
    // Nowhere to go back to: the focus is left on the page, not thrown at
    // a detached button.
    await waitFor(() => expect(document.activeElement).toBe(document.body));
  });
});
