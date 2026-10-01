import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { ReactNode, Ref } from "react";
import { useTranslation } from "react-i18next";
import { requestNotificationPermission } from "../lib/api";
import { useSettings, useSaveSettings, useSnapshot } from "../lib/queries";
import { ADAPTER_LABEL_KEYS, adapterIdOf, adapterLabel, instanceLabels, settingsSaveSentence } from "../lib/sources";
import { activeSnoozes, shownSkippedVersion, skippedVersionId } from "../lib/updateState";
import { snoozeDate } from "../lib/snooze";
import { AUTO_CHECK_CHOICES, AUTO_CHECK_CHOICE_KEYS, autoCheckChoice, withAutoCheckChoice } from "../lib/checkFrequency";
import type { ArtifactKey, Settings, Language, SkippedVersion, SnoozedUpdate } from "../lib/types";
import { artifactKeyId, useUiStore } from "../store/ui";
import { Switch } from "../components/ui/Switch";
import { IconCreditsDrawer } from "../components/IconCreditsDrawer";
import { NextAutoCheck } from "../components/NextAutoCheck";
import { DiagnosticsRows } from "../components/DiagnosticsRows";
import { TextWithInfo } from "../components/InfoDetail";
import { BUTTON } from "../components/ui/controls";
import { PopupButton } from "../components/ui/PopupButton";
import {
  FORM_COLUMN,
  GROUP,
  GROUP_FOOTNOTE,
  GROUP_ROW,
  GROUP_ROW_TWO_LINES,
  GROUP_TITLE,
  SMALL_WRAPPING,
} from "../components/ui/group";

const LANGUAGES: Language[] = ["System", "En", "ZhCn"];

function languageLabelKey(lang: Language): string {
  if (lang === "System") return "settings.language.system";
  if (lang === "En") return "settings.language.english";
  return "settings.language.chinese";
}

/** A row's own button: Stop skipping, Remind me again, View… -- a regular grey one (`BUTTON`). */
const ROW_BUTTON = BUTTON.regular.grey;

/**
 * One group of settings, as System Settings draws one (`GROUP`): its title
 * over a container that holds its rows, a hairline between each two, and
 * under it, where the group has one, a footnote -- what the group's rows
 * do, said once for all of them. A region, named by its title. With
 * `headingRef`, a group the page can be opened at: its title can then take
 * the focus from a script (`tabIndex` -1), and draws no ring for it, as
 * the page's title does. `list`: its rows are a list's items, the
 * container the list.
 */
function SettingsGroup({
  title,
  headingRef,
  footnote,
  list = false,
  children,
}: {
  title: string;
  headingRef?: Ref<HTMLHeadingElement>;
  footnote?: ReactNode;
  list?: boolean;
  children: ReactNode;
}) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId}>
      <h2
        id={headingId}
        ref={headingRef}
        tabIndex={headingRef === undefined ? undefined : -1}
        className={`${GROUP_TITLE} outline-none`}
      >
        {title}
      </h2>
      {list ? <ul className={GROUP}>{children}</ul> : <div className={GROUP}>{children}</div>}
      {footnote}
    </section>
  );
}

/**
 * One setting in a group: what it is, with its control on the right, on
 * one line -- or, where the group needs it, a second line under the label
 * (`subtitle`), 11 muted: the one explanation a group keeps by its row,
 * or a state that passes, such as why a switch cannot be turned on now.
 * `label` is the element that names the control -- a `<label>` for a
 * switch or a popup.
 */
function SettingRow({
  label,
  subtitle,
  control,
}: {
  label: ReactNode;
  subtitle?: ReactNode;
  control: ReactNode;
}) {
  return (
    <div className={subtitle ? GROUP_ROW_TWO_LINES : GROUP_ROW}>
      <div className="min-w-0">
        {label}
        {subtitle}
      </div>
      {control}
    </div>
  );
}

const ROW_LABEL = "block text-body text-foreground";
/** The label of a row whose switch is disabled: in the colour of disabled text, as the switch is faded. */
const ROW_LABEL_DISABLED = "block text-body text-tertiary";
/** A row's second line: 11 muted, its lines 16 apart if it wraps (`SMALL_WRAPPING`). */
const ROW_SUBTITLE = `${SMALL_WRAPPING} text-muted`;

/**
 * A hidden update's row: its software, the way the Updates and Installed
 * rows name it, then in small muted text beside it its source and what
 * was skipped -- 「Homebrew · 2.102.0」 -- as Mail marks an account beside
 * a name (spec R3); the source is left out for a tool with its own
 * installer, which is its own source. Its button on the right.
 */
function HiddenEntry({ name, meta, button }: { name: string; meta: string | null; button: ReactNode }) {
  return (
    <li className={GROUP_ROW}>
      <span className="flex min-w-0 items-baseline gap-2">
        <span className="truncate text-body text-foreground">{name}</span>
        {meta !== null ? <span className="shrink-0 text-small tabular-nums text-muted">{meta}</span> : null}
      </span>
      {button}
    </li>
  );
}

/** A hidden-updates group with nothing in it: one row that says so, in the muted colour (spec R4: never the tertiary). */
function NoEntries({ text }: { text: string }) {
  return <p className={`${GROUP_ROW} text-body text-muted`}>{text}</p>;
}

/**
 * Settings, a grouped form as System Settings' own (spec §3.7), in five
 * groups: 「通用」 -- the language, and whether to show technical details
 * -- 「更新」 -- how often to check (「检查更新」: 不自动检查, 每天 or
 * 每周), 「有更新时通知我」 under it, and
 * whether Homebrew's self-updating apps are listed -- then the three kinds
 * of hidden update, 「已跳过的版本」, 「30天内不提醒的工具」 (with the day each
 * comes back) and 「不再提醒的工具」, each entry with the button that
 * takes it back, where the Overview's count of
 * hidden updates opens the page -- and 「关于」: the app's 「版本」, then
 * the 「图标来源」 row that opens the credits for the logos built into
 * the app (`IconCreditsDrawer`), and 「拷贝诊断信息」 with the checkbox
 * that adds the list of tools (`DiagnosticsRows`). Every change is saved at once; one that cannot
 * be saved is undone on screen and said at the top.
 */
export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const settingsQuery = useSettings();
  const saveMutation = useSaveSettings();
  const { data: snapshot } = useSnapshot();
  const [draft, setDraft] = useState<Settings | null>(null);
  const [creditsOpen, setCreditsOpen] = useState(false);
  // 「有更新时通知我」 while the permission it needs is being asked for
  // (`turnNotifyOn`), and whether it was refused the last time it was.
  const [askingToNotify, setAskingToNotify] = useState(false);
  const [notifyRefused, setNotifyRefused] = useState(false);
  // The same two for 「操作完成时通知」.
  const [askingToNotifyOps, setAskingToNotifyOps] = useState(false);
  const [notifyOpsRefused, setNotifyOpsRefused] = useState(false);
  // The settings the page holds, as of its last render: what the answer to
  // that permission is saved over, since it arrives after the click.
  const held = useRef<Settings | undefined>(undefined);
  useEffect(() => {
    held.current = draft ?? settingsQuery.data;
  });

  useEffect(() => {
    if (settingsQuery.data && draft === null) {
      setDraft(settingsQuery.data);
    }
  }, [settingsQuery.data, draft]);

  // What the Updates and Installed rows call each package they list: its
  // `display_name` ("Claude Code", where the package is "claude").
  const displayNames = useMemo(() => {
    const byId = new Map<string, string>();
    for (const artifact of snapshot?.artifacts ?? []) {
      byId.set(artifactKeyId(artifact.key), artifact.display_name);
    }
    return byId;
  }, [snapshot]);
  // Each source by the name the sidebar gives it -- 「Homebrew（Intel）」
  // where this Mac has two (`instanceLabels`).
  const labels = useMemo(() => instanceLabels(t, snapshot?.instances ?? []), [t, snapshot]);

  const current = draft ?? settingsQuery.data;
  const loaded = !settingsQuery.isLoading && current !== undefined;

  // Opened from the Overview's 「2个已隐藏」 (`showHiddenUpdates`): the
  // two groups of hidden updates -- their titles and their rows -- in
  // view, and the focus on the first one's title, as soon as they are on
  // screen.
  const hiddenGroups = useRef<HTMLDivElement>(null);
  const hiddenHeading = useRef<HTMLHeadingElement>(null);
  const hiddenUpdatesRequested = useUiStore((s) => s.hiddenUpdatesRequested);
  const hiddenUpdatesShown = useUiStore((s) => s.hiddenUpdatesShown);
  useEffect(() => {
    const heading = hiddenHeading.current;
    if (!hiddenUpdatesRequested || heading === null) return;
    hiddenGroups.current?.scrollIntoView?.({ block: "nearest" });
    heading.focus({ preventScroll: true });
    hiddenUpdatesShown();
  }, [hiddenUpdatesRequested, loaded, hiddenUpdatesShown]);

  if (settingsQuery.isLoading || !current) {
    return (
      <div className={FORM_COLUMN}>
        <p className="px-2.5 text-body text-muted">{t("common.loading")}</p>
      </div>
    );
  }

  // Arrow functions declared *after* the early return, not hoisted function
  // declarations: TypeScript only carries the `!current` narrowing into
  // function expressions, so a `function persist() {}` here would see
  // `current` as `Settings | undefined` and fail `pnpm build` (TS18048 /
  // TS2345) two tasks later, at Task 17's type-check.
  const persist = (next: Settings, previous: Settings = current) => {
    setDraft(next);
    saveMutation.mutate(next, {
      onError: () => setDraft(previous),
    });
  };

  // 「有更新时通知我」 turned on: permission to post is asked for first,
  // the switch on and still meanwhile, and the setting saved on only once
  // it is granted -- over what the page holds by then, and only while the
  // daily check is still on. Refused, or the asking itself failed, the
  // switch is back off with the line that says where to allow it.
  const turnNotifyOn = () => {
    setNotifyRefused(false);
    setAskingToNotify(true);
    void requestNotificationPermission()
      .catch(() => false)
      .then((granted) => {
        setAskingToNotify(false);
        if (!granted) {
          setNotifyRefused(true);
          return;
        }
        const now = held.current;
        if (now?.auto_check) persist({ ...now, notify_updates: true }, now);
      });
  };

  // 「操作完成时通知」 turned on: as 「有更新时通知我」 is, permission
  // first -- the same permission, asked the same way -- and saved on only
  // once it is granted, over what the page holds by then. It needs no
  // automatic check.
  const turnNotifyOpsOn = () => {
    setNotifyOpsRefused(false);
    setAskingToNotifyOps(true);
    void requestNotificationPermission()
      .catch(() => false)
      .then((granted) => {
        setAskingToNotifyOps(false);
        if (!granted) {
          setNotifyOpsRefused(true);
          return;
        }
        const now = held.current;
        if (now) persist({ ...now, notify_operations: true }, now);
      });
  };

  // An entry's name and source. Where the snapshot no longer lists the
  // package, a tool with its own installer is still named by its product
  // name, which is its source's too (its `display_name` is that name, from
  // its recipe in crates/banager-core/src/adapters/standalone/), and
  // anything else by the package's own name. The source is left out where
  // it would only say the name again, as on the Updates page's rows. It is
  // named as the sidebar names it, or -- for a source the snapshot does
  // not list -- by the key alone: an instance id starts with its adapter's.
  const entryOf = (key: ArtifactKey): { name: string; source: string | undefined } => {
    const adapterId = adapterIdOf(key.instance_id);
    const source = labels.get(key.instance_id) ?? adapterLabel(t, adapterId);
    const standalone =
      adapterId.startsWith("standalone-") &&
      Object.prototype.hasOwnProperty.call(ADAPTER_LABEL_KEYS, adapterId);
    const name = displayNames.get(artifactKeyId(key)) || (standalone ? source : key.name);
    return { name, source: source === name ? undefined : source };
  };

  // What an entry's row says beside its name: its source and what was
  // skipped, 「Homebrew · 2.102.0」, or whichever of the two it has.
  const metaOf = (source: string | undefined, skipped?: string): string | null => {
    if (source !== undefined && skipped !== undefined) {
      return t("settings.hiddenEntryMeta", { source, version: skipped });
    }
    return source ?? skipped ?? null;
  };

  // The snoozes still running, as the Updates page hides by them.
  const snoozes = activeSnoozes(current);

  const unignore = (key: Settings["ignored_updates"][number]) => {
    persist({
      ...current,
      ignored_updates: current.ignored_updates.filter(
        (k) => artifactKeyId(k) !== artifactKeyId(key),
      ),
    });
  };

  // A snooze taken back: its package listed again on the Updates page.
  const unsnooze = (snoozed: SnoozedUpdate) => {
    persist({
      ...current,
      snoozed_updates: (current.snoozed_updates ?? []).filter(
        (s) => artifactKeyId(s.key) !== artifactKeyId(snoozed.key),
      ),
    });
  };

  const unskip = (skipped: SkippedVersion) => {
    persist({
      ...current,
      skipped_versions: current.skipped_versions.filter(
        (s) => skippedVersionId(s) !== skippedVersionId(skipped),
      ),
    });
  };

  return (
    // No title of its own: the page header over it says "Settings". A
    // column as System Settings' (`FORM_COLUMN`): centred, and no wider
    // than 560, so that on a wide window a switch stays within reach of
    // its words.
    <div className={FORM_COLUMN}>
      {/* Its reason only with "Show technical details" on as saved, not
          as shown: a save that was to turn it on is what failed. */}
      {saveMutation.isError && (
        <p role="alert" className="px-2.5 text-body text-danger-text">
          {settingsSaveSentence(
            t,
            "settings.saveError",
            saveMutation.error.message,
            settingsQuery.data?.show_technical_details ?? false,
          )}
        </p>
      )}

      <SettingsGroup title={t("settings.groups.general")}>
        <SettingRow
          label={
            <label htmlFor="settings-language" className={ROW_LABEL}>
              {t("settings.language.label")}
            </label>
          }
          control={
            // A popup button, as a grouped form's choice of one from a few
            // (spec §3.7): the Mac's own menu, and a select's keyboard.
            <PopupButton
              id="settings-language"
              value={current.language}
              options={LANGUAGES.map((lang) => ({ value: lang, label: t(languageLabelKey(lang)) }))}
              onChange={(language) => persist({ ...current, language })}
            />
          }
        />
        {/* The group's one explanation, under the row it explains. */}
        <SettingRow
          label={
            <label htmlFor="settings-show-technical" className={ROW_LABEL}>
              {t("settings.showTechnicalDetails.label")}
            </label>
          }
          subtitle={
            <p id="settings-show-technical-desc" className={ROW_SUBTITLE}>
              {t("settings.showTechnicalDetails.description")}
            </p>
          }
          control={
            <Switch
              id="settings-show-technical"
              aria-describedby="settings-show-technical-desc"
              checked={current.show_technical_details}
              onCheckedChange={(checked) => persist({ ...current, show_technical_details: checked })}
            />
          }
        />
      </SettingsGroup>

      {/* The automatic check (src-tauri/src/auto_check.rs), off by
          default, every day or every week when on, and under it the
          notification that belongs to it
          (src-tauri/src/notify.rs): offered only while the daily check
          is on, shown off -- saying so, and what turns it on -- while it
          is not, saved off when the daily check is turned off, and
          turned on only with permission to post (`turnNotifyOn`). What
          the daily check does is said under it, the group's one standing
          line; which apps the last switch adds is the group's footnote,
          right under that switch. */}
      <SettingsGroup
        title={t("settings.groups.updates")}
        footnote={
          <p id="settings-include-self-updating-desc" className={GROUP_FOOTNOTE}>
            {t("settings.includeSelfUpdating.description")}
          </p>
        }
      >
        <SettingRow
          label={
            <label htmlFor="settings-auto-check" className={ROW_LABEL}>
              {t("settings.autoCheck.label")}
            </label>
          }
          subtitle={
            <>
              <p id="settings-auto-check-desc" className={ROW_SUBTITLE}>
                {t("settings.autoCheck.description")}
              </p>
              {/* When the next one is due, while it is on. */}
              {current.auto_check ? (
                <NextAutoCheck at={snapshot?.next_auto_check_at} className={`mt-0.5 ${ROW_SUBTITLE}`} />
              ) : null}
            </>
          }
          control={
            // 「不自动检查」, 「每天」 or 「每周」 (src/lib/checkFrequency.ts):
            // a popup, as the language is chosen.
            <PopupButton
              id="settings-auto-check"
              describedBy="settings-auto-check-desc"
              value={autoCheckChoice(current)}
              options={AUTO_CHECK_CHOICES.map((choice) => ({ value: choice, label: t(AUTO_CHECK_CHOICE_KEYS[choice]) }))}
              onChange={(choice) => {
                setNotifyRefused(false);
                persist(withAutoCheckChoice(current, choice));
              }}
            />
          }
        />
        <SettingRow
          label={
            <label
              htmlFor="settings-notify-updates"
              className={current.auto_check ? ROW_LABEL : ROW_LABEL_DISABLED}
            >
              {t("settings.notifyUpdates.label")}
            </label>
          }
          subtitle={
            !current.auto_check ? (
              // Why it does not move: a faded switch alone said nothing.
              <p id="settings-notify-updates-desc" className={ROW_SUBTITLE}>
                {t("settings.notifyUpdates.needsAutoCheck", {
                  setting: t("settings.autoCheck.label"),
                  day: t("settings.checkEvery.day"),
                  week: t("settings.checkEvery.week"),
                })}
              </p>
            ) : notifyRefused ? (
              <p id="settings-notify-updates-desc" role="status" className={ROW_SUBTITLE}>
                {t("settings.notifyUpdates.refused")}
              </p>
            ) : undefined
          }
          control={
            <Switch
              id="settings-notify-updates"
              aria-describedby={
                !current.auto_check || notifyRefused ? "settings-notify-updates-desc" : undefined
              }
              checked={current.auto_check && (current.notify_updates || askingToNotify)}
              disabled={!current.auto_check || askingToNotify}
              onCheckedChange={(checked) =>
                checked ? turnNotifyOn() : persist({ ...current, notify_updates: false })
              }
            />
          }
        />
        {/* A run of operations that finished while the window was not in
            front: closing the window does not stop one
            (src-tauri/src/notify_ops.rs). Off by default; on only with
            permission to post (`turnNotifyOpsOn`). */}
        <SettingRow
          label={
            <label htmlFor="settings-notify-operations" className={ROW_LABEL}>
              {t("settings.notifyOperations.label")}
            </label>
          }
          subtitle={
            // Only a state that passes, as under 「有更新时通知我」: the
            // group keeps one standing second line, the check's.
            notifyOpsRefused ? (
              <p id="settings-notify-operations-desc" role="status" className={ROW_SUBTITLE}>
                {t("settings.notifyUpdates.refused")}
              </p>
            ) : undefined
          }
          control={
            <Switch
              id="settings-notify-operations"
              aria-describedby={notifyOpsRefused ? "settings-notify-operations-desc" : undefined}
              checked={(current.notify_operations ?? false) || askingToNotifyOps}
              disabled={askingToNotifyOps}
              onCheckedChange={(checked) =>
                checked ? turnNotifyOpsOn() : persist({ ...current, notify_operations: false })
              }
            />
          }
        />
        <SettingRow
          label={
            <label htmlFor="settings-include-self-updating" className={ROW_LABEL}>
              {t("settings.includeSelfUpdating.label")}
            </label>
          }
          control={
            <Switch
              id="settings-include-self-updating"
              aria-describedby="settings-include-self-updating-desc"
              checked={current.include_self_updating}
              onCheckedChange={(checked) => persist({ ...current, include_self_updating: checked })}
            />
          }
        />
      </SettingsGroup>

      {/* The two ways the Updates page hides an update, each a group of
          its own, because they end differently: a skip stops hiding
          anything by itself once its source offers another version, while
          a package never to be reminded about stays hidden until it is
          removed here.

          Every stored skip is listed, including one whose version its
          source has since moved past: such an entry hides nothing any more
          (`hidingRule` matches only the version a row offers), and it is
          shown rather than dropped. This page reads the snapshot for names
          only (`entryOf`), and every other change saved here writes the
          list back as it is, so an entry leaves it only when the user
          presses Stop skipping on it, or skips that package's next version
          on the Updates page, which replaces it (`withSkippedVersion`). */}
      <div ref={hiddenGroups} className="flex flex-col gap-6">
        <SettingsGroup
          title={t("settings.skippedVersions.title")}
          headingRef={hiddenHeading}
          list={current.skipped_versions.length > 0}
        >
          {current.skipped_versions.length === 0 ? (
            <NoEntries text={t("settings.hiddenNone")} />
          ) : (
            current.skipped_versions.map((skipped) => {
              // An Ollama model's skipped version is a digest, never shown.
              const version = shownSkippedVersion(skipped);
              const { name, source } = entryOf(skipped.key);
              return (
                <HiddenEntry
                  key={skippedVersionId(skipped)}
                  name={name}
                  meta={metaOf(source, version ?? t("settings.skippedVersions.newBuild"))}
                  button={
                    <button
                      type="button"
                      aria-label={
                        version === null
                          ? t("settings.skippedVersions.unskipNewBuildAriaLabel", { name })
                          : t("settings.skippedVersions.unskipAriaLabel", { name, version })
                      }
                      onClick={() => unskip(skipped)}
                      className={ROW_BUTTON}
                    >
                      {t("settings.skippedVersions.unskip")}
                    </button>
                  }
                />
              );
            })
          )}
        </SettingsGroup>

        {/* The updates put off for 30 days, each with the day it comes
            back; only those still running (`activeSnoozes`): one that has
            run out hides nothing, and Rust drops it at the next launch. */}
        <SettingsGroup title={t("settings.snoozedUpdates.title")} list={snoozes.length > 0}>
          {snoozes.length === 0 ? (
            <NoEntries text={t("settings.hiddenNone")} />
          ) : (
            snoozes.map((snoozed) => {
              const { name, source } = entryOf(snoozed.key);
              return (
                <HiddenEntry
                  key={artifactKeyId(snoozed.key)}
                  name={name}
                  meta={metaOf(source, t("settings.snoozedUpdates.until", { date: snoozeDate(snoozed.until, i18n.language) }))}
                  button={
                    <button
                      type="button"
                      aria-label={t("settings.ignoredUpdates.unignoreAriaLabel", { name })}
                      onClick={() => unsnooze(snoozed)}
                      className={ROW_BUTTON}
                    >
                      {t("settings.ignoredUpdates.unignore")}
                    </button>
                  }
                />
              );
            })
          )}
        </SettingsGroup>

        <SettingsGroup title={t("settings.ignoredUpdates.title")} list={current.ignored_updates.length > 0}>
          {current.ignored_updates.length === 0 ? (
            <NoEntries text={t("settings.hiddenNone")} />
          ) : (
            current.ignored_updates.map((key) => {
              const { name, source } = entryOf(key);
              return (
                <HiddenEntry
                  key={artifactKeyId(key)}
                  name={name}
                  meta={metaOf(source)}
                  button={
                    <button
                      type="button"
                      aria-label={t("settings.ignoredUpdates.unignoreAriaLabel", { name })}
                      onClick={() => unignore(key)}
                      className={ROW_BUTTON}
                    >
                      {t("settings.ignoredUpdates.unignore")}
                    </button>
                  }
                />
              );
            })
          )}
        </SettingsGroup>
      </div>

      <SettingsGroup
        title={t("settings.groups.about")}
        footnote={
          <p className={GROUP_FOOTNOTE}>
            {/* What the text holds, item by item, behind the ⓘ. */}
            <TextWithInfo text={t("diagnostics.footnote")} label={t("common.detailsLabel", { title: t("diagnostics.label") })}>
              {t("clarity.diagnosticsDetail")}
            </TextWithInfo>
          </p>
        }
      >
        {/* The version as System Settings' About shows one: a plain row,
            the value on the right in the muted colour, and text a user
            can select to copy into a report (`select-text`). */}
        <SettingRow
          label={<span className={ROW_LABEL}>{t("settings.version")}</span>}
          control={<span className="select-text text-body tabular-nums text-muted">{__APP_VERSION__}</span>}
        />
        <SettingRow
          label={<span className={ROW_LABEL}>{t("settings.iconCredits.label")}</span>}
          control={
            <button
              type="button"
              aria-label={t("settings.iconCredits.openAriaLabel")}
              onClick={() => setCreditsOpen(true)}
              className={ROW_BUTTON}
            >
              {t("settings.iconCredits.open")}
            </button>
          }
        />
        <DiagnosticsRows />
      </SettingsGroup>
      <IconCreditsDrawer open={creditsOpen} onOpenChange={setCreditsOpen} />
    </div>
  );
}
