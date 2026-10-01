import { useId, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
  SHORTCUT_GROUPS,
  useShortcutsSheet,
  type Shortcut,
  type ShortcutGroup,
  type ShortcutGroupId,
  type ShortcutId,
} from "../lib/shortcuts";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";
import { GROUP, GROUP_ROW, GROUP_TITLE } from "./ui/group";

/** Each group's title, spelled out. */
const GROUP_TITLE_KEYS: Record<ShortcutGroupId, string> = {
  window: "shortcuts.groups.window",
  list: "shortcuts.groups.list",
  dialog: "shortcuts.groups.dialog",
};

/** What each row's keys do, spelled out. */
const SHORTCUT_TEXT_KEYS: Record<ShortcutId, string> = {
  settings: "shortcuts.rows.settings",
  overview: "shortcuts.rows.overview",
  updates: "shortcuts.rows.updates",
  installed: "shortcuts.rows.installed",
  unknown: "shortcuts.rows.unknown",
  checkAgain: "shortcuts.rows.checkAgain",
  search: "shortcuts.rows.search",
  closeWindow: "shortcuts.rows.closeWindow",
  quit: "shortcuts.rows.quit",
  move: "shortcuts.rows.move",
  page: "shortcuts.rows.page",
  ends: "shortcuts.rows.ends",
  tick: "shortcuts.rows.tick",
  details: "shortcuts.rows.details",
  closeDetails: "shortcuts.rows.closeDetails",
  tab: "shortcuts.rows.tab",
  press: "shortcuts.rows.press",
  escape: "shortcuts.rows.escape",
};

/**
 * One row: what the keys do on the left, in the label colour, and the keys
 * on the right, muted, as a Mac menu shows an item's shortcut.
 */
function Row({ shortcut }: { shortcut: Shortcut }) {
  const { t } = useTranslation();
  return (
    <li data-shortcut={shortcut.id} className={GROUP_ROW}>
      <span className="min-w-0 flex-1 break-words text-body text-foreground">{t(SHORTCUT_TEXT_KEYS[shortcut.id])}</span>
      <kbd className="shrink-0 whitespace-nowrap font-sans text-body text-muted">{shortcut.keys}</kbd>
    </li>
  );
}

/** One group: its title over its rows, as Settings' groups are drawn. */
function Group({ group }: { group: ShortcutGroup }) {
  const { t } = useTranslation();
  const titleId = useId();
  return (
    <section aria-labelledby={titleId} data-shortcut-group={group.id} className="mt-4">
      <h3 id={titleId} className={GROUP_TITLE}>
        {t(GROUP_TITLE_KEYS[group.id])}
      </h3>
      <ul className={GROUP}>
        {group.shortcuts.map((shortcut) => (
          <Row key={shortcut.id} shortcut={shortcut} />
        ))}
      </ul>
    </section>
  );
}

/**
 * 「键盘快捷键」, the sheet Help's item of that name opens
 * (`useShortcutsSheet`): the keys Banager answers to, in three groups --
 * the window's, the lists', the dialogs' (`SHORTCUT_GROUPS`,
 * src/lib/shortcuts.ts). It only shows; Done, the default button, closes
 * it, and so do Escape and a click beside it.
 */
export function ShortcutsSheet() {
  const { t } = useTranslation();
  const open = useShortcutsSheet((s) => s.open);
  const close = () => useShortcutsSheet.setState({ open: false });
  const doneRef = useRef<HTMLButtonElement>(null);
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => useShortcutsSheet.setState({ open: next })}
      title={t("shortcuts.title")}
      width="several"
      initialFocus={doneRef}
      footer={
        <button ref={doneRef} type="button" onClick={close} className={BUTTON.large.default}>
          {t("common.done")}
        </button>
      }
    >
      {open ? SHORTCUT_GROUPS.map((group) => <Group key={group.id} group={group} />) : null}
    </Dialog>
  );
}
