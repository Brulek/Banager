import { describe, expect, it } from "vitest";
import tauriConfig from "../../src-tauri/tauri.conf.json";

function directive(csp: string, name: string): string[] {
  const found = csp
    .split(";")
    .map((part) => part.trim())
    .find((part) => part === name || part.startsWith(`${name} `));
  if (found === undefined) {
    throw new Error(`no ${name} directive in CSP: ${csp}`);
  }
  return found.split(/\s+/).slice(1);
}

describe("Tauri CSP", () => {
  // The CSP had never been exercised, and `style-src 'self'` with no nonce
  // blocked two <style> elements our own dependencies inject at runtime:
  //
  //   - @radix-ui/react-scroll-area (the log drawer) renders
  //     ScrollAreaViewportStyle, a <style dangerouslySetInnerHTML> that hides
  //     the native scrollbars;
  //   - react-style-singleton, reached through @radix-ui/react-dialog ->
  //     react-remove-scroll, creates a <style> element for the body scroll
  //     lock every dialog applies.
  //
  // Both would need a nonce plumbed through third-party code to survive a
  // strict style-src. Inline styles are allowed instead: the app renders no
  // untrusted HTML, so there is no author-controlled markup for an injected
  // style to come from.
  it("allows the inline styles our dependencies inject", () => {
    expect(directive(tauriConfig.app.security.csp, "style-src")).toContain("'unsafe-inline'");
  });

  // The loosening is confined to styles. Script injection is the attack this
  // CSP is actually defending against, so script-src (and everything else)
  // stays exactly as strict as it was.
  it("keeps every other directive locked down", () => {
    const csp = tauriConfig.app.security.csp;
    expect(directive(csp, "script-src")).toEqual(["'self'"]);
    expect(directive(csp, "default-src")).toEqual(["'self'"]);
    expect(directive(csp, "connect-src")).toEqual(["'self'"]);
    expect(directive(csp, "img-src")).toEqual([
      "'self'",
      "data:",
      "asset:",
      "https://asset.localhost",
    ]);
  });
});
