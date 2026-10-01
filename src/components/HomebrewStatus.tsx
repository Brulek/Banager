import { useId, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import type { HomebrewLifecycle, InstalledArtifact, Sizes } from "../lib/types";
import { otherVersionsSizeText, sizeViewOf } from "../lib/sizes";
import { detailLines } from "./updateDetails";
import { DisclosureIcon } from "./icons";
import { CopyButton } from "./CopyButton";
import { COMMAND_SLOT } from "./withCommand";

/**
 * Homebrew's reasons it knows by name (`DeprecateDisable`'s
 * `FORMULA_DEPRECATE_DISABLE_REASONS` and `CASK_DEPRECATE_DISABLE_REASONS`
 * in Homebrew 7.0.7, `deprecate_disable.rb`), each one fixed sentence. A
 * reason not listed here is the maintainers' own words, said verbatim in
 * quotes (`brewStatus.reasonQuoted`), never translated or dropped.
 */
export const REASON_KEYS: Record<string, string> = {
  does_not_build: "brewStatus.reason.does_not_build",
  no_license: "brewStatus.reason.no_license",
  repo_archived: "brewStatus.reason.repo_archived",
  repo_removed: "brewStatus.reason.repo_removed",
  unmaintained: "brewStatus.reason.unmaintained",
  unreachable: "brewStatus.reason.unreachable",
  unsupported: "brewStatus.reason.unsupported",
  deprecated_upstream: "brewStatus.reason.deprecated_upstream",
  versioned_formula: "brewStatus.reason.versioned_formula",
  checksum_mismatch: "brewStatus.reason.checksum_mismatch",
  discontinued: "brewStatus.reason.discontinued",
  moved_to_mas: "brewStatus.reason.moved_to_mas",
  no_longer_available: "brewStatus.reason.no_longer_available",
  no_longer_meets_criteria: "brewStatus.reason.no_longer_meets_criteria",
  fails_gatekeeper_check: "brewStatus.reason.fails_gatekeeper_check",
};

/** Which of Homebrew's two marks a package carries: disabled wins, as it is the later stage. */
export type HomebrewMark = { kind: "disabled" | "deprecated"; lifecycle: HomebrewLifecycle };

function markOf(artifact: InstalledArtifact): HomebrewMark | null {
  const homebrew = artifact.facts.homebrew;
  if (homebrew === null) return null;
  if (homebrew.disabled !== null) return { kind: "disabled", lifecycle: homebrew.disabled };
  if (homebrew.deprecated !== null) return { kind: "deprecated", lifecycle: homebrew.deprecated };
  return null;
}

/**
 * What a mark means, in one or two sentences: why, when Homebrew gave a
 * reason -- a fixed sentence for a reason it knows by name, its own words
 * in quotes otherwise -- then what the mark does. A disabled package gets
 * no more updates, and the copy already installed is left where it is
 * (Homebrew removes nothing that is installed; Banager neither). A
 * deprecated one may be disabled later; nothing is said about when, which
 * Banager does not know. The date is Homebrew's, as it writes it.
 */
export function lifecycleSentence(t: TFunction, mark: HomebrewMark): string {
  const { date, reason } = mark.lifecycle;
  // An own key only: a free-text reason such as "constructor" must be
  // quoted, not looked up on Object's prototype.
  const why =
    reason === null
      ? ""
      : Object.prototype.hasOwnProperty.call(REASON_KEYS, reason)
        ? t(REASON_KEYS[reason])
        : t("brewStatus.reasonQuoted", { reason });
  const what =
    mark.kind === "disabled"
      ? date !== null
        ? t("brewStatus.disabledOn", { date })
        : t("brewStatus.disabled")
      : date !== null
        ? t("brewStatus.deprecatedOn", { date })
        : t("brewStatus.deprecated");
  return why === "" ? what : t("brewStatus.reasonThenMark", { why, what });
}

/**
 * A row's quiet word for Homebrew's mark -- 「已停用」 or 「已弃用」 -- with
 * the sentence behind its ⓘ, shaped as the Installed page's `RowChip`.
 * `null` for anything Homebrew has not marked.
 */
export function homebrewStatusChip(
  t: TFunction,
  artifact: InstalledArtifact,
): { id: string; label: string; ariaLabel: string; detail: ReactNode; tone: "neutral" } | null {
  const mark = markOf(artifact);
  if (mark === null) return null;
  const name = artifact.display_name;
  return mark.kind === "disabled"
    ? {
        id: "homebrew-disabled",
        label: t("brewStatus.disabledWord"),
        ariaLabel: t("brewStatus.disabledAria", { name }),
        detail: detailLines([lifecycleSentence(t, mark)]),
        tone: "neutral",
      }
    : {
        id: "homebrew-deprecated",
        label: t("brewStatus.deprecatedWord"),
        ariaLabel: t("brewStatus.deprecatedAria", { name }),
        detail: detailLines([lifecycleSentence(t, mark)]),
        tone: "neutral",
      };
}

/**
 * The ids `homebrewStatusChip` gives. The inspector's 状态 row shows these
 * words without their ⓘ: `HomebrewNotes` says the same sentence under the
 * facts, and once is enough.
 */
export const HOMEBREW_STATUS_CHIP_IDS: ReadonlySet<string> = new Set(["homebrew-disabled", "homebrew-deprecated"]);

/**
 * `address` with a line allowed to break only where an address reads
 * well broken -- after `//`, and before each `/`, `?`, `#`, `&` and `=`
 * after it -- not in the middle of a name: never "https://youtube-dl" /
 * ".org/" in a narrow pane, so not before a `.` either. A host too long
 * for a line still breaks (`break-words`).
 */
export function addressWithBreaks(address: string): ReactNode {
  const scheme = address.indexOf("//");
  const head = scheme === -1 ? "" : address.slice(0, scheme + 2);
  const rest = scheme === -1 ? address : address.slice(scheme + 2);
  const parts = rest.split(/(?=[/?#&=])/);
  // Text and <wbr> side by side, so the address is still one text to
  // find and to read aloud.
  return [
    head,
    ...parts.flatMap((part, index) => (index > 0 || head !== "" ? [<wbr key={index} />, part] : [part])),
  ];
}

/**
 * `sentence` with each date in it (`2026-09-01`, as Homebrew writes it)
 * kept on one line: "2025-" / "11-01" reads as two numbers.
 */
function datesUnbroken(sentence: string): ReactNode {
  return sentence
    .split(/(\d{4}-\d{2}-\d{2})/)
    .map((part, index) =>
      index % 2 === 1 ? (
        <span key={index} className="whitespace-nowrap">
          {part}
        </span>
      ) : (
        part
      ),
    );
}

/**
 * A translated sentence with `name` set into it at `COMMAND_SLOT`, never
 * broken across lines: "yt-" / "dlp" reads as two names. A translation
 * without the slot exactly once gets the name back as plain text.
 */
function nameUnbroken(sentence: string, name: string): ReactNode {
  const parts = sentence.split(COMMAND_SLOT);
  if (parts.length !== 2) return parts.join(name);
  return (
    <>
      {parts[0]}
      <span className="whitespace-nowrap">{name}</span>
      {parts[1]}
    </>
  );
}

/**
 * The inspector's 「主页」 fact, for any source that reported one: the
 * address as text, and 「拷贝链接」 under it, its 「已拷贝」 beside it
 * (`CopyButton`). Nothing opens it -- opening a page from Banager is a
 * decision not yet taken -- so the address is only read and copied, by
 * its button rather than by selecting it, as the inspector selects only
 * versions and a location.
 */
export function homepageFact(
  t: TFunction,
  homepage: string | null,
): { term: string; value: ReactNode; selectable: boolean } | null {
  const address = homepage?.trim() ?? "";
  if (address === "") return null;
  return {
    term: t("brewStatus.homepage"),
    value: (
      <span className="flex flex-col items-end gap-1">
        <span data-homepage="" className="break-words">
          {addressWithBreaks(address)}
        </span>
        <CopyButton text={address} label={t("brewStatus.copyLink")} />
      </span>
    ),
    selectable: false,
  };
}

/**
 * The inspector's 「其他版本」 fact: a Homebrew formula's other kegs, by
 * version -- selectable, as the inspector's versions are -- and under
 * them, once measured, what they take together, 「约120 MB」 (「共约…」 for
 * several), in the secondary colour. Null for a package with none.
 *
 * Said as they are, with no cause and not as "old": Homebrew lists a
 * formula's kegs by version, the one linked is not always the newest, and
 * its own cleanup leaves some (a dependency's old keg among them) for
 * reasons not in what it reports. The size is the same kegs' (size.rs
 * `old_versions_job`, every `Cellar/<name>/*` besides the one listed); a
 * size with no versions to say it under is not shown.
 */
export function otherVersionsFact(
  t: TFunction,
  artifact: InstalledArtifact,
  sizes: Sizes | undefined,
): { term: string; value: ReactNode; selectable: boolean } | null {
  const others = artifact.facts.homebrew?.other_versions ?? [];
  if (others.length === 0) return null;
  const view = sizeViewOf(sizes, artifact);
  const measured = view?.kind === "measured" ? view.otherVersions : null;
  return {
    term: t("brewStatus.otherVersionsTerm"),
    value: (
      <span data-other-versions="" className="flex flex-col items-end">
        <span>{others.join(t("common.listSeparator"))}</span>
        {measured !== null ? (
          <span data-other-versions-size="" className="text-muted">
            {otherVersionsSizeText(t, measured, others.length)}
          </span>
        ) : null}
      </span>
    ),
    selectable: true,
  };
}

/**
 * Homebrew's caveats behind a closed disclosure, as Homebrew wrote them,
 * in English. Plain text with its line breaks and indents, and no Copy:
 * caveats often hold shell lines to paste into Terminal, which is not a
 * step to hand someone who did not ask for it.
 */
function Caveats({ text }: { text: string }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const panelId = useId();
  return (
    <div>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        onClick={() => setOpen(!open)}
        className="-ml-1 flex h-7 items-center gap-1.5 rounded-control px-1 text-body text-muted"
      >
        <DisclosureIcon size={10} className={`shrink-0 ${open ? "rotate-90" : ""}`} />
        {t("brewStatus.caveatsTitle")}
      </button>
      {open ? (
        <p
          id={panelId}
          lang="en"
          data-caveats=""
          className="mt-1 whitespace-pre-wrap break-words rounded-control bg-group px-2.5 py-2 text-small text-foreground"
        >
          {text}
        </p>
      ) : null}
    </div>
  );
}

/**
 * What Homebrew says about a package beyond its facts: what its mark
 * means and the name Homebrew suggests instead (a name only: installing
 * it is not offered here) -- `part="mark"`, right under the inspector's
 * group, whose last row, 「状态」, says the mark's word -- and its caveats
 * -- `part="caveats"`, technical, after everything else the inspector
 * says of it. Both, one after the other, without `part`. Nothing for a
 * package Homebrew has nothing to say about. Its other versions are a
 * fact of the group (`otherVersionsFact`).
 */
export function HomebrewNotes({ artifact, part }: { artifact: InstalledArtifact; part?: "mark" | "caveats" }) {
  const { t } = useTranslation();
  const homebrew = artifact.facts.homebrew;
  if (homebrew === null) return null;
  const mark = markOf(artifact);
  // A package both deprecated and disabled may name its replacement on the
  // deprecation only; Homebrew still said it.
  const replacement = mark?.lifecycle.replacement ?? homebrew.deprecated?.replacement ?? null;
  const lines: ReactNode[] = [];
  if (part !== "caveats" && mark !== null) {
    lines.push(
      <p key="mark" data-homebrew-mark="" className="text-body-long text-foreground">
        {datesUnbroken(lifecycleSentence(t, mark))}
      </p>,
    );
  }
  if (part !== "caveats" && replacement !== null) {
    lines.push(
      <p key="replacement" data-homebrew-replacement="" className="text-body-long text-foreground">
        {nameUnbroken(t("brewStatus.replacement", { name: COMMAND_SLOT }), replacement)}
      </p>,
    );
  }
  if (part !== "mark" && homebrew.caveats !== null) lines.push(<Caveats key="caveats" text={homebrew.caveats} />);
  if (lines.length === 0) return null;
  return (
    <div data-homebrew-notes={part ?? ""} className="mt-4 flex flex-col gap-2">
      {lines}
    </div>
  );
}
