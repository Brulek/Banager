import { createContext, useContext } from "react";
import { resolveSourceIcon, resolveToolIcon, toolIconCredits, toolIconKey, type ToolIcons } from "./toolIcons";

/**
 * The logos the avatars draw (`ToolAvatar`, `SourceAvatar`), and the ones
 * Settings credits (`IconCreditsDrawer`): the built-in pack's, the only
 * ones the app has, unless a provider above them hands them another pack.
 * The tests do (`renderWithProviders`): the built-in pack is regenerated
 * from the reviewed mapping, so a test that drew from it would pass or
 * fail on whatever that mapping happens to list.
 */
export const ToolIconsContext = createContext<ToolIcons>({
  toolIconKey,
  resolveToolIcon,
  resolveSourceIcon,
  credits: toolIconCredits,
});

/** The pack the nearest `ToolIconsContext` provider hands down, or the built-in one. */
export function useToolIcons(): ToolIcons {
  return useContext(ToolIconsContext);
}
