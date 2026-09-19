import { describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "./test/setup";
import App from "./App";

const mockInvoke = vi.mocked(invoke);

const emptySnapshot = {
  generation: 0,
  detect: "Found",
  instances: [],
  artifacts: [],
  updates: [],
  refreshed_at: null,
  stale: false,
  errors: [],
};

const defaultSettings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
};

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(emptySnapshot);
    if (cmd === "get_settings") return Promise.resolve(defaultSettings);
    return Promise.resolve(undefined);
  });
});

describe("App", () => {
  it("shows the Installed page's filter box by default", async () => {
    const { findByLabelText } = renderWithProviders(<App />);
    await findByLabelText("Filter installed items");
  });

  it("switches the content area when a sidebar link is clicked", async () => {
    const { getByRole, findByLabelText, findByText } = renderWithProviders(<App />);
    await findByLabelText("Filter installed items");

    fireEvent.click(getByRole("button", { name: "Updates" }));

    expect(await findByText("Everything is up to date")).toBeInTheDocument();
  });
});
