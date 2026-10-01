import { useId, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import type { HomebrewLifecycle, InstalledArtifact } from "../lib/types";
import { detailLines } from "./updateDetails";
import { DisclosureIcon } from "./icons";
import { BUTTON } from "./ui/controls";
import { SMALL_WRAPPING } from "./ui/group";

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
 * The inspector's 「主页」 fact, for any source that reported one: the
 * address as text, and 「拷贝链接」 under it. Nothing opens it -- opening
 * a page from Banager is a decision not yet taken -- so the address is
 * only read and copied, by its button rather than by selecting it, as
 * the inspector selects only versions and a location. `onCopy` is the
 * page's copy, whose 「已拷贝」 the toolbar says.
 */
export function homepageFact(
  t: TFunction,
  homepage: string | null,
  onCopy: (text: string) => void,
): { term: string; value: ReactNode; selectable: boolean } | null {
  const address = homepage?.trim() ?? "";
  if (address === "") return null;
  return {
    term: t("brewStatus.homepage"),
    value: (
      <span className="flex flex-col items-end gap-1">
        <span data-homepage="" className="break-all">
          {address}
        </span>
        <button type="button" onClick={() => onCopy(address)} className={BUTTON.small.grey}>
          {t("brewStatus.copyLink")}
        </button>
      </span>
    ),
    selectable: false,
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
 * What Homebrew says about a package beyond its facts, under the
 * inspector's group: what its mark means, the name Homebrew suggests
 * instead (a name only: installing it is not offered here), the other
 * versions still installed, and its caveats. Nothing for a package
 * Homebrew has nothing to say about.
 *
 * The other versions are said as they are, with no cause: Homebrew's own
 * cleanup leaves some (a dependency's old keg among them), and which one
 * left them is not in what Homebrew reports.
 */
export function HomebrewNotes({ artifact }: { artifact: InstalledArtifact }) {
  const { t } = useTranslation();
  const homebrew = artifact.facts.homebrew;
  if (homebrew === null) return null;
  const mark = markOf(artifact);
  // A package both deprecated and disabled may name its replacement on the
  // deprecation only; Homebrew still said it.
  const replacement = mark?.lifecycle.replacement ?? homebrew.deprecated?.replacement ?? null;
  const others = homebrew.other_versions;
  const lines: ReactNode[] = [];
  if (mark !== null) {
    lines.push(
      <p key="mark" data-homebrew-mark="" className="text-body-long text-foreground">
        {lifecycleSentence(t, mark)}
      </p>,
    );
  }
  if (replacement !== null) {
    lines.push(
      <p key="replacement" data-homebrew-replacement="" className="text-body-long text-foreground">
        {t("brewStatus.replacement", { name: replacement })}
      </p>,
    );
  }
  if (others.length > 0) {
    lines.push(
      <p key="others" data-other-versions="" className={`text-muted ${SMALL_WRAPPING}`}>
        {t("brewStatus.otherVersions", {
          count: others.length,
          versions: others.join(t("common.listSeparator")),
        })}
      </p>,
    );
  }
  if (homebrew.caveats !== null) lines.push(<Caveats key="caveats" text={homebrew.caveats} />);
  if (lines.length === 0) return null;
  return (
    <div data-homebrew-notes="" className="mt-4 flex flex-col gap-2">
      {lines}
    </div>
  );
}
