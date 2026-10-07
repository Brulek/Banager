import { fireEvent, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import { SourceDiagnostic } from "./SourceDiagnostic";
import { SourceNoticeLine } from "./SourceNotice";

it("opens and copies the startup diagnostic through the shared notice without technical details", async () => {
  await i18n.changeLanguage("zh-CN");
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  const diagnostic = "npm error EJSONPARSE https://****@proxy.test";
  const notice = { id: "npm", variant: "warning" as const, titleKey: "", descriptionKey: "", diagnostic };
  const view = renderWithProviders(<SourceNoticeLine variant="warning" title="npm" description="Failed"
    detailsAriaLabel="Details: npm" details={<SourceDiagnostic notice={notice} />} />);
  fireEvent.click(view.getByRole("button", { name: "Details: npm" }));
  fireEvent.click(view.getByText("启动诊断"));
  expect(view.getByText(diagnostic)).toBeVisible();
  fireEvent.click(view.getByRole("button", { name: "拷贝诊断" }));
  await waitFor(() => expect(writeText).toHaveBeenCalledWith(diagnostic));
});

it("opens with the house disclosure button, closed until asked (f13b review)", async () => {
  await i18n.changeLanguage("en");
  const diagnostic = "npm error config Invalid npmrc";
  const notice = { id: "npm", variant: "warning" as const, titleKey: "", descriptionKey: "", diagnostic, diagnosticCause: "notFound" as const };
  const view = renderWithProviders(<SourceDiagnostic notice={notice} />);
  const toggle = view.getByRole("button", { name: "Startup Diagnostic" });
  expect(toggle).toHaveAttribute("aria-expanded", "false");
  expect(view.queryByText(diagnostic)).toBeNull();
  fireEvent.click(toggle);
  expect(toggle).toHaveAttribute("aria-expanded", "true");
  expect(view.getByText(diagnostic)).toBeVisible();
  expect(view.getByText("Something it needs is missing. The error says what.")).toBeVisible();
  expect(view.getByRole("button", { name: "Copy Diagnostic" })).toBeInTheDocument();
});
