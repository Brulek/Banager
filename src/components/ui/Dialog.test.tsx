import { useEffect, useRef, useState } from "react";
import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../test/setup";
import { InfoDetail } from "../InfoDetail";
import { PageHeader } from "../PageHeader";
import { DIALOG_WIDTHS, Dialog } from "./Dialog";
import { BUTTON } from "./controls";
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

/** Every shipping `.tsx` under src/, for the scans below. */
function componentSources(): Array<[string, string]> {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
  const walk = (dir: string): string[] =>
    readdirSync(dir).flatMap((entry) => {
      const full = path.join(dir, entry);
      if (statSync(full).isDirectory()) return entry === "dev" || entry === "test" ? [] : walk(full);
      return /\.tsx$/.test(full) && !/\.test\.tsx$/.test(full) ? [full] : [];
    });
  return walk(root).map((file) => [path.relative(root, file), readFileSync(file, "utf8")]);
}

describe("Dialog", () => {
  it.each([
    ["one", 360],
    ["several", 480],
    ["log", 560],
  ] as const)("is %s wide: %ipx, and never wider than the window less 16 either side", (width, px) => {
    renderWithProviders(
      <Dialog open onOpenChange={vi.fn()} title="Confirm" width={width}>
        <p>Body content</p>
      </Dialog>,
    );
    const dialog = screen.getByRole("dialog");
    expect(DIALOG_WIDTHS[width]).toBe(px);
    expect(dialog).toHaveAttribute("data-dialog-width", String(px));
    // jsdom writes the calc() its own way round.
    expect(dialog.style.width).toMatch(new RegExp(`^min\\(${px}px, (?:calc\\(100vw - 32px\\)|-32px \\+ 100vw)\\)$`));
  });

  it("is 360 wide unless told otherwise", () => {
    renderWithProviders(
      <Dialog open onOpenChange={vi.fn()} title="Confirm">
        <p>Body content</p>
      </Dialog>,
    );
    expect(screen.getByRole("dialog")).toHaveAttribute("data-dialog-width", "360");
  });

  it("darkens nothing under it, as a Mac's alert and sheet do, and still keeps the page out of reach", async () => {
    const onOpenChange = vi.fn();
    renderWithProviders(
      <Dialog open onOpenChange={onOpenChange} title="Confirm" footer={<button type="button">OK</button>}>
        <p>Body content</p>
      </Dialog>,
    );
    const overlay = document.querySelector("[data-dialog-overlay]") as HTMLElement;
    expect(overlay).not.toBeNull();
    // Over the whole window, and clear: no fill, nothing fading in.
    expect(overlay.className.split(" ")).toEqual(["fixed", "inset-0"]);
    expect(overlay.className).not.toMatch(/\bbg-|animate-/);
    // No dimmer's colour left to reach for, in either appearance.
    const css = readFileSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../index.css"), "utf8");
    expect(css).not.toContain("--color-overlay");
    // The dialog's own shadow and edge set it apart instead.
    expect(screen.getByRole("dialog")).toHaveClass("shadow-dialog");
    // Escape still closes it, and the focus stays inside it.
    expect(screen.getByRole("dialog").contains(document.activeElement)).toBe(true);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
  });

  it("looks like a macOS alert: no edge, corners of 10, the dialog's shadow, 52 from the top, 20 in", () => {
    renderWithProviders(
      <Dialog
        open
        onOpenChange={vi.fn()}
        title="Uninstall “jq”?"
        icon={<span data-testid="icon" />}
        subtitle="Homebrew · 1.8.1"
        footer={<button type="button">OK</button>}
      >
        <p>Body content</p>
      </Dialog>,
    );
    const dialog = screen.getByRole("dialog");
    for (const look of ["rounded-group", "shadow-dialog", "top-[52px]", "bg-surface"]) expect(dialog).toHaveClass(look);
    expect(dialog.className).not.toMatch(/\bborder\b|shadow-2xl/);
    // The icon, 12 over the question; the question in 13 bold; the line
    // under it quieter.
    const icon = dialog.querySelector("[data-dialog-icon]") as HTMLElement;
    expect(icon).toContainElement(screen.getByTestId("icon"));
    expect(icon).toHaveClass("mb-3");
    const title = screen.getByRole("heading", { name: "Uninstall “jq”?" });
    expect(title).toHaveClass("text-title");
    expect(icon.compareDocumentPosition(title) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByText("Homebrew · 1.8.1")).toHaveClass("text-small", "text-muted");
    expect(title.parentElement).toHaveClass("px-5", "pt-5");
  });

  it("puts its buttons on the right, 8 apart and 16 under what it says, with no line or band of their own", () => {
    renderWithProviders(
      <Dialog
        open
        onOpenChange={vi.fn()}
        title="Confirm"
        footerStart={<button type="button">Copy</button>}
        footer={
          <>
            <button type="button">Cancel</button>
            <button type="button">OK</button>
          </>
        }
      >
        <p>Body content</p>
      </Dialog>,
    );
    const footer = screen.getByRole("dialog").querySelector("[data-dialog-footer]") as HTMLElement;
    expect(footer).toHaveClass("justify-end", "gap-2", "px-5", "pt-4", "pb-5");
    expect(footer.className).not.toMatch(/\bp-5\b|\bpt-5\b/);
    expect(footer.className).not.toMatch(/border|bg-/);
    // What goes at the other end, on the left.
    expect(within(footer).getByRole("button", { name: "Copy" }).parentElement).toHaveClass("mr-auto");
    expect(within(footer).getAllByRole("button").map((button) => button.textContent)).toEqual(["Copy", "Cancel", "OK"]);
  });

  it("stacks its buttons as wide as itself, in the order given, when asked to", () => {
    renderWithProviders(
      <Dialog
        open
        onOpenChange={vi.fn()}
        title="Confirm"
        stackedFooter
        footer={
          <>
            <button type="button">Stay</button>
            <button type="button">Quit</button>
          </>
        }
      >
        <p>Body content</p>
      </Dialog>,
    );
    const footer = screen.getByRole("dialog").querySelector("[data-dialog-footer]") as HTMLElement;
    expect(footer).toHaveClass("flex-col", "items-stretch", "[&>button]:w-full", "pt-4");
    expect(within(footer).getAllByRole("button").map((button) => button.textContent)).toEqual(["Stay", "Quit"]);
  });

  it("takes the focus itself as it opens when told to, rather than a control's", async () => {
    renderWithProviders(
      <Dialog open onOpenChange={vi.fn()} title="Log" focusSelf footer={<button type="button">Done</button>}>
        <p>Body content</p>
      </Dialog>,
    );
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("dialog")));
  });

  it("is never red: no button anywhere in the app is drawn in the danger colours", () => {
    // HIG (Alerts): an action the user chose, removing something included,
    // is not tinted as a warning -- so no dialog's button, nor any other.
    for (const kinds of Object.values(BUTTON)) {
      for (const className of Object.values(kinds)) expect(className).not.toMatch(/danger|red/);
    }
    const red = componentSources().filter(([, source]) => /\bbg-(?:danger|red)\b|\bbg-\[var\(--color-danger/.test(source));
    expect(red.map(([file]) => file)).toEqual([]);
  });

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

  it("describes itself by its subtitle, then the main text it names, and by nothing where it has neither", () => {
    const { getByRole, rerender } = renderWithProviders(
      <Dialog open onOpenChange={vi.fn()} title="Uninstall “jq”?" subtitle="Homebrew · 1.8.1" describedBy="about-jq">
        <p id="about-jq">Removes jq.</p>
        <p>Not this.</p>
      </Dialog>,
    );
    expect(getByRole("dialog", { name: "Uninstall “jq”?" })).toHaveAccessibleDescription("Homebrew · 1.8.1 Removes jq.");

    rerender(
      <Dialog open onOpenChange={vi.fn()} title="Uninstall “jq”?" describedBy="about-jq">
        <p id="about-jq">Removes jq.</p>
      </Dialog>,
    );
    expect(getByRole("dialog")).toHaveAccessibleDescription("Removes jq.");

    rerender(
      <Dialog open onOpenChange={vi.fn()} title="Operation log">
        <p>Lines.</p>
      </Dialog>,
    );
    expect(getByRole("dialog")).not.toHaveAttribute("aria-describedby");
  });

  it("is an alert dialog where it asks to confirm, as NSAlert reads, and behaves as any other", async () => {
    // Decision I21c: the role only, not Radix's AlertDialog -- its focus is
    // still placed as `initialFocus` says, and Escape (as a click on the
    // page around it) still closes it.
    const onOpenChange = vi.fn();
    const cancel = { current: null as HTMLButtonElement | null };
    const { getByRole, queryByRole } = renderWithProviders(
      <Dialog
        open
        alert
        onOpenChange={onOpenChange}
        title="Uninstall “jq”?"
        subtitle="Homebrew · 1.8.1"
        describedBy="about-jq"
        initialFocus={cancel}
        footer={
          <>
            <button
              ref={(button) => {
                cancel.current = button;
              }}
              type="button"
            >
              Cancel
            </button>
            <button type="button">Uninstall</button>
          </>
        }
      >
        <p id="about-jq">Removes jq.</p>
      </Dialog>,
    );
    const sheet = getByRole("alertdialog", { name: "Uninstall “jq”?" });
    expect(queryByRole("dialog")).toBeNull();
    expect(sheet).toHaveAccessibleDescription("Homebrew · 1.8.1 Removes jq.");
    await waitFor(() => expect(document.activeElement).toBe(getByRole("button", { name: "Cancel" })));
    await userEvent.setup().keyboard("{Escape}");
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("fades its body's bottom edge while more of it is below what is in sight, and not once its end is", () => {
    const { getByRole } = renderWithProviders(
      <Dialog open onOpenChange={vi.fn()} title="Icon credits" footer={<button type="button">Done</button>}>
        <p>A long list</p>
      </Dialog>,
    );
    const body = getByRole("dialog").querySelector("[data-dialog-body]") as HTMLElement;
    // Laid out 300 high, holding 900: more below.
    let scrollTop = 0;
    Object.defineProperty(body, "clientHeight", { configurable: true, get: () => 300 });
    Object.defineProperty(body, "scrollHeight", { configurable: true, get: () => 900 });
    Object.defineProperty(body, "scrollTop", { configurable: true, get: () => scrollTop });
    fireEvent.scroll(body);
    expect(body).toHaveAttribute("data-more-below");
    scrollTop = 400;
    fireEvent.scroll(body);
    expect(body).toHaveAttribute("data-more-below");
    // Scrolled to its end: nothing left to fade.
    scrollTop = 600;
    fireEvent.scroll(body);
    expect(body).not.toHaveAttribute("data-more-below");

    // index.css: the fade, over the bottom 24.
    const css = readFileSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../index.css"), "utf-8")
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/\s+/g, " ");
    expect(css).toContain(
      "[data-dialog-body][data-more-below] { mask-image: linear-gradient(to bottom, #000 calc(100% - 24px), transparent); }",
    );
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

  it.each([
    ["gone from the page", "removed"],
    ["turned off", "disabled"],
  ] as const)("gives the focus to the page's title when the opener is %s", async (_what, how) => {
    function Vanishing() {
      const [open, setOpen] = useState(false);
      const [started, setStarted] = useState(false);
      const opener = useRef<HTMLElement | null>(null);
      return (
        <>
          <PageHeader title="Updates" actions={null} />
          {how === "removed" && started ? null : (
            <button
              type="button"
              disabled={started}
              onClick={(event) => {
                opener.current = event.currentTarget;
                setOpen(true);
              }}
            >
              Update glib
            </button>
          )}
          <Dialog open={open} onOpenChange={setOpen} title="Update glib?" returnFocusTo={opener}>
            <button
              type="button"
              onClick={() => {
                // The row's button gives way to its progress as the update
                // starts; Update selected turns off with nothing ticked.
                setStarted(true);
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
    // Not thrown at a button that cannot take it, nor left on the window's
    // body, from where the next Tab starts over at the top.
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("heading", { name: "Updates" })));
  });

  // r24 W10: the window's body is where the focus is when nothing has it,
  // and no opener -- given back to it, the next Tab starts over at the
  // sidebar and VoiceOver's cursor is nowhere.
  it.each([
    ["the welcome sheet, open as the window first draws", { atOnce: true, passBody: false }],
    ["a sheet the menu bar opens while nothing has the focus", { atOnce: false, passBody: false }],
    ["a sheet told the body opened it, as Update All from the menu bar", { atOnce: false, passBody: true }],
  ] as const)("gives the focus to the page's title, not the body, after %s", async (_what, { atOnce, passBody }) => {
    let openSheet: () => void = () => undefined;
    function Window() {
      const [open, setOpen] = useState<boolean>(atOnce);
      const opener = useRef<HTMLElement | null>(null);
      openSheet = () => {
        opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
        setOpen(true);
      };
      return (
        <>
          <PageHeader title="Overview" actions={null} />
          <Dialog open={open} onOpenChange={setOpen} title="Welcome" returnFocusTo={passBody ? opener : undefined}>
            <button type="button" onClick={() => setOpen(false)}>
              Get Started
            </button>
          </Dialog>
        </>
      );
    }
    const user = userEvent.setup();
    renderWithProviders(<Window />);
    if (!atOnce) {
      expect(document.activeElement).toBe(document.body);
      act(() => openSheet());
    }
    const done = await screen.findByRole("button", { name: "Get Started" });
    await waitFor(() => expect(document.activeElement).toBe(done));

    await user.keyboard("{Escape}");

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("heading", { name: "Overview" })));
    expect(document.activeElement).not.toBe(document.body);
  });

  it("gives the focus to the page's title after its own button closes a sheet nothing opened", async () => {
    function Window() {
      const [open, setOpen] = useState(true);
      return (
        <>
          <PageHeader title="Overview" actions={null} />
          <Dialog open={open} onOpenChange={setOpen} title="Welcome">
            <button type="button" onClick={() => setOpen(false)}>
              Get Started
            </button>
          </Dialog>
        </>
      );
    }
    const user = userEvent.setup();
    renderWithProviders(<Window />);

    await user.click(await screen.findByRole("button", { name: "Get Started" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("heading", { name: "Overview" })));
  });
});
