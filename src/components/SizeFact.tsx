import type { ReactNode } from "react";
import { formatBytes } from "../lib/format";
import { sizeNoteOf, sizeText, sizeViewOf, type Translate } from "../lib/sizes";
import { TextWithInfo } from "./InfoDetail";
import type { InstalledArtifact, Sizes } from "../lib/types";

/** One row of the Installed page's details: its label, its value, and whether the value selects. */
export interface SizeFactRow {
  term: string;
  value: ReactNode;
  selectable: boolean;
}

/**
 * The details' row for how much `artifact` takes on disk, or null for
 * none:
 *
 * - a size its source reports (an Ollama model's own, from Ollama) keeps
 *   the row it always had, 「大小」 and the number;
 * - a measured one is 「占用空间」 and 「约312 MB」 -- 「至少约…」 when the
 *   round's budget ran out, 「…，部分无法读取」 when part of it could not be
 *   read; for a Cargo crate, a tool with its own installer and a uv tool,
 *   an ⓘ after the number says what it leaves out or shares
 *   (`sizeNoteOf`). What a Homebrew formula's other versions take is
 *   said with those versions, in the row under this one
 *   (`otherVersionsFact`);
 * - while it is measured, 「正在计算…」 in the secondary colour;
 * - with nothing measured for it (`sizeViewOf`), no row at all.
 *
 * Nothing here says how much could be freed, and nothing offers to: the
 * row only says what is there.
 */
export function sizeFact(t: Translate, artifact: InstalledArtifact, sizes: Sizes | undefined): SizeFactRow | null {
  if (artifact.size_bytes !== null) {
    return { term: t("installed.size"), value: formatBytes(artifact.size_bytes), selectable: true };
  }
  const view = sizeViewOf(sizes, artifact);
  if (view === null) return null;
  if (view.kind === "measuring") {
    return {
      term: t("sizes.term"),
      value: (
        <span data-size="measuring" className="text-muted">
          {t("sizes.measuring")}
        </span>
      ),
      selectable: false,
    };
  }
  const text = sizeText(t, view.measured);
  const note = sizeNoteOf(artifact);
  return {
    term: t("sizes.term"),
    value: (
      <span data-size="measured">
        {note === null ? (
          text
        ) : (
          <TextWithInfo text={text} label={t("common.detailsLabel", { title: t("sizes.term") })}>
            {t(note)}
          </TextWithInfo>
        )}
      </span>
    ),
    selectable: true,
  };
}
