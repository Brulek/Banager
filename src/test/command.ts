import type { MatcherFunction } from "@testing-library/react";

/**
 * The code a command is set in, found by its whole text as a person reads
 * it: `getByText(command("/opt/homebrew/bin/brew upgrade --formula glib"))`,
 * or every one a pattern matches: `getAllByText(command(/brew upgrade/))`.
 * Each token is an element of its own (`unbrokenTokens` in
 * src/components/CommandPreview.tsx), so the words are the code's, not any
 * one text node's, which is all a plain string matches.
 */
export function command(text: string | RegExp): MatcherFunction {
  return (_content, element) => {
    if (element?.tagName !== "CODE") return false;
    const words = element.textContent ?? "";
    return typeof text === "string" ? words === text : text.test(words);
  };
}
