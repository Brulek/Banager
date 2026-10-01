import { describe, expect, it } from "vitest";
import { act, render } from "@testing-library/react";
import { useState } from "react";
import { useFocusOnPageChange } from "./pageFocus";
import type { Page } from "../store/ui";

/** A window in small: the title every page has, and a page whose button opens another page. */
function Window({ initial = "overview" as Page, sidebar = false }) {
  const [page, setPage] = useState<Page>(initial);
  useFocusOnPageChange(page);
  return (
    <div>
      {sidebar ? (
        <button type="button" onClick={() => setPage("settings")}>
          Settings row
        </button>
      ) : null}
      <h1 tabIndex={-1} data-focus-fallback="">
        {page}
      </h1>
      {page === "overview" ? (
        <button key="overview" type="button" onClick={() => setPage("updates")}>
          Review Updates
        </button>
      ) : (
        <button key={page} type="button">
          On {page}
        </button>
      )}
    </div>
  );
}

describe("useFocusOnPageChange", () => {
  it("puts the focus on the new page's title when the button that opened it went with the old page", () => {
    const { getByRole } = render(<Window />);
    const review = getByRole("button", { name: "Review Updates" });
    review.focus();
    act(() => review.click());
    expect(review.isConnected).toBe(false);
    expect(document.activeElement).toBe(getByRole("heading", { name: "updates" }));
  });

  it("leaves the focus where it is when it is still there: a sidebar row", () => {
    const { getByRole } = render(<Window sidebar />);
    const row = getByRole("button", { name: "Settings row" });
    row.focus();
    act(() => row.click());
    expect(document.activeElement).toBe(row);
  });

  it("does nothing as the window first opens", () => {
    const { getByRole } = render(<Window />);
    expect(document.activeElement).not.toBe(getByRole("heading"));
  });
});
