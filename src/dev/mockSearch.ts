/**
 * The browser preview's Homebrew search (`search_homebrew` in
 * ./mockBackend.ts): a pretend Homebrew's names and descriptions, searched
 * as `BrewAdapter::search` searches the real one's -- names first (the
 * one the text is, those it starts, the rest), then those only a
 * description matched, at most `SEARCH_HITS_MAX`.
 */
import type { Found } from "../lib/types";

interface MockPackage {
  kind: Found["kind"];
  name: string;
  description: string;
}

/** The pretend Homebrew's formulae and casks. */
export const MOCK_HOMEBREW: MockPackage[] = [
  { kind: "Formula", name: "jq", description: "Lightweight and flexible command-line JSON processor" },
  { kind: "Formula", name: "jq-lsp", description: "Jq language server" },
  { kind: "Formula", name: "gojq", description: "Pure Go implementation of jq" },
  { kind: "Formula", name: "jaq", description: "JQ clone focussed on correctness, speed, and simplicity" },
  { kind: "Formula", name: "ripgrep", description: "Search tool like grep and The Silver Searcher" },
  { kind: "Formula", name: "fd", description: "Simple, fast and user-friendly alternative to find" },
  { kind: "Formula", name: "bat", description: "Clone of cat(1) with syntax highlighting and Git integration" },
  { kind: "Formula", name: "htop", description: "Improved top (interactive process viewer)" },
  { kind: "Formula", name: "yt-dlp", description: "Feature-rich command-line audio/video downloader" },
  { kind: "Formula", name: "ffmpeg", description: "Play, record, convert, and stream select audio and video codecs" },
  { kind: "Formula", name: "imagemagick", description: "Tools and libraries to manipulate images in select formats" },
  { kind: "Formula", name: "gemini-cli", description: "Interact with Google Gemini AI models from the command-line" },
  { kind: "Cask", name: "claude-code", description: "Terminal-based AI coding assistant" },
  { kind: "Cask", name: "codex", description: "OpenAI's coding agent that runs in your terminal" },
  { kind: "Cask", name: "visual-studio-code", description: "Open-source code editor" },
  { kind: "Cask", name: "iina", description: "Free and open-source media player" },
  { kind: "Cask", name: "handbrake-app", description: "Open-source video transcoder" },
  { kind: "Cask", name: "jquake", description: "Real-time earthquake monitoring software for Japan" },
];

/** At most this many results, as `brew::SEARCH_HITS_MAX`. */
const SEARCH_HITS_MAX = 40;

export function mockSearch(query: string): Array<{ kind: Found["kind"]; name: string; description: string }> {
  const wanted = query.toLowerCase();
  const rank = (name: string) => (name === wanted ? 0 : name.startsWith(wanted) ? 1 : 2);
  const named = MOCK_HOMEBREW.filter((p) => p.name.toLowerCase().includes(wanted)).sort(
    (a, b) => rank(a.name.toLowerCase()) - rank(b.name.toLowerCase()),
  );
  const described = MOCK_HOMEBREW.filter(
    (p) => !p.name.toLowerCase().includes(wanted) && p.description.toLowerCase().includes(wanted),
  );
  return [...named, ...described].slice(0, SEARCH_HITS_MAX);
}
