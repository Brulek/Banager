//! The menu bar: what a Mac app has at the top of the screen, in the
//! language the window uses.
//!
//! What goes in it is `menu_bar`, a plain description a test can read
//! without a running app -- tauri's menu items can only be made on the
//! main thread of a running one -- and `build` turns that into tauri's
//! menu. Where macOS provides an item's action (About and its panel,
//! Services, Hide, Quit, Close Window, the Edit menu's, the Window
//! menu's), the item is macOS's own (`MacItem`), shortcut and all, and
//! Banager gives only its label. The items that act in the page --
//! Settings…; the View menu's four pages, ⌘1 to ⌘4 as in Finder's and
//! Mail's; Check Again; Search -- bring the window back if it was closed
//! or minimized and tell it, one event each (`PageCommand`, through
//! `window::show_and_tell`), and the page runs the code its own controls
//! run (src/lib/menu.ts). Help has five items of Banager's: Welcome to
//! Banager, which shows again the sheet the first launch showed; Common
//! Questions, a sheet answering what people ask most, in plain words;
//! Keyboard Shortcuts, a sheet of the keys Banager answers to; Check Tool
//! Setup…, which opens the sheet Settings' About opens too; and Copy
//! Diagnostic Info…, which the page answers by opening Settings on its
//! button of that name, focused: the copy is the button's click, which a
//! webview always lets write the clipboard.
//!
//! The page says which language: `set_menu_language`, at startup and at
//! every change of language. Until it has, the menu bar is built in the
//! one the page is about to choose (`initial_language`).

use banager_core::settings::Language;
use serde::Deserialize;
use std::sync::Mutex;
use tauri::menu::{
    AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Manager, Runtime};

use crate::window;

/// The languages the menu bar is written in: the window's three. The page
/// names them as its i18n does, "en", "zh-CN" and "zh-Hant"; Tauri refuses any other
/// value for `set_menu_language` before the command runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum MenuLanguage {
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "zh-Hant")]
    ZhHant,
}

/// What an item of the menu bar asks of the page. The page handles each
/// with what its own control for it runs (src/lib/menu.ts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageCommand {
    /// Settings… (⌘,): the Settings page, as the sidebar's Settings opens it.
    Settings,
    /// Overview (⌘1), Updates (⌘2), Installed (⌘3), Other Programs (⌘4):
    /// that page, as its row in the sidebar opens it -- Installed on
    /// everything installed, whatever source it was showing. `Unknown` is
    /// Other Programs, as the page is `UnknownPage` in src/pages.
    Overview,
    Updates,
    Installed,
    Unknown,
    /// Check Again (⌘R): the refresh the page header's Check again runs,
    /// and nothing while one runs.
    CheckAgain,
    /// Search (⌘F): the Installed page, with its search box focused.
    Search,
    /// Help's Welcome to Banager: the sheet the first launch showed
    /// (src/components/WelcomeSheet.tsx), again, over whatever page is
    /// open. Closing it saves nothing new: it was shown already. No
    /// ellipsis, as it asks nothing more, and no shortcut.
    Welcome,
    /// Help's Common Questions: a sheet of the questions people using
    /// these tools ask most, each answered in a few plain sentences, with a
    /// button to the page where it can be acted on where there is one
    /// (src/components/FaqSheet.tsx), over whatever page is open. No
    /// ellipsis, as it only shows, and no shortcut.
    CommonQuestions,
    /// Help's Keyboard Shortcuts: a sheet listing the keys Banager answers
    /// to (src/components/ShortcutsSheet.tsx) -- this menu bar's shortcuts,
    /// and the lists' and dialogs' keys -- over whatever page is open. No
    /// ellipsis, as it only shows, and no shortcut.
    KeyboardShortcuts,
    /// Help's Check Tool Setup…: the sheet that says how this Mac's
    /// command-line tools are set up (src/components/ToolSetupSheet.tsx),
    /// over whatever page is open, as Settings' About opens it. No
    /// shortcut.
    CheckToolSetup,
    /// Help's Copy Diagnostic Info…: Settings, on its button of that name
    /// (src/components/DiagnosticsRows.tsx), focused, which copies the text
    /// at its click -- the ellipsis says that one more step follows. No
    /// shortcut.
    CopyDiagnostics,
}

impl PageCommand {
    /// In the menu bar's order: Settings… in the app's menu, then View's,
    /// then Help's.
    pub const ALL: [PageCommand; 12] = [
        PageCommand::Settings,
        PageCommand::Overview,
        PageCommand::Updates,
        PageCommand::Installed,
        PageCommand::Unknown,
        PageCommand::CheckAgain,
        PageCommand::Search,
        PageCommand::Welcome,
        PageCommand::CommonQuestions,
        PageCommand::KeyboardShortcuts,
        PageCommand::CheckToolSetup,
        PageCommand::CopyDiagnostics,
    ];

    /// The item's id: what `on_menu_event` hears when it is chosen.
    pub fn id(self) -> &'static str {
        match self {
            PageCommand::Settings => "settings",
            PageCommand::Overview => "overview",
            PageCommand::Updates => "updates",
            PageCommand::Installed => "installed",
            PageCommand::Unknown => "unknown",
            PageCommand::CheckAgain => "check-again",
            PageCommand::Search => "search",
            PageCommand::Welcome => "welcome",
            PageCommand::CommonQuestions => "common-questions",
            PageCommand::KeyboardShortcuts => "keyboard-shortcuts",
            PageCommand::CheckToolSetup => "check-tool-setup",
            PageCommand::CopyDiagnostics => "copy-diagnostics",
        }
    }

    /// The event the window hears; src/lib/api.ts's `MENU_EVENTS` spells
    /// the same twelve.
    pub fn event(self) -> &'static str {
        match self {
            PageCommand::Settings => "menu://settings",
            PageCommand::Overview => "menu://overview",
            PageCommand::Updates => "menu://updates",
            PageCommand::Installed => "menu://installed",
            PageCommand::Unknown => "menu://unknown",
            PageCommand::CheckAgain => "menu://check-again",
            PageCommand::Search => "menu://search",
            PageCommand::Welcome => "menu://welcome",
            PageCommand::CommonQuestions => "menu://common-questions",
            PageCommand::KeyboardShortcuts => "menu://keyboard-shortcuts",
            PageCommand::CheckToolSetup => "menu://check-tool-setup",
            PageCommand::CopyDiagnostics => "menu://copy-diagnostics",
        }
    }

    /// Its shortcut, as tauri writes one: ⌘, ⌘1 ⌘2 ⌘3 ⌘4 ⌘R ⌘F on a Mac;
    /// none for Help's five.
    pub fn shortcut(self) -> Option<&'static str> {
        match self {
            PageCommand::Settings => Some("CmdOrCtrl+Comma"),
            PageCommand::Overview => Some("CmdOrCtrl+1"),
            PageCommand::Updates => Some("CmdOrCtrl+2"),
            PageCommand::Installed => Some("CmdOrCtrl+3"),
            PageCommand::Unknown => Some("CmdOrCtrl+4"),
            PageCommand::CheckAgain => Some("CmdOrCtrl+R"),
            PageCommand::Search => Some("CmdOrCtrl+F"),
            PageCommand::Welcome
            | PageCommand::CommonQuestions
            | PageCommand::KeyboardShortcuts
            | PageCommand::CheckToolSetup
            | PageCommand::CopyDiagnostics => None,
        }
    }

    pub fn from_id(id: &str) -> Option<PageCommand> {
        PageCommand::ALL
            .into_iter()
            .find(|command| command.id() == id)
    }
}

/// An item whose action macOS provides (tauri's `PredefinedMenuItem`):
/// AppKit carries it out -- About's panel, Hide's hiding, the Edit
/// menu's Undo to Select All in a text field -- with the shortcut a Mac
/// app has for it (⌘H, ⌥⌘H, ⌘Q, ⌘W, ⌘Z, ⇧⌘Z, ⌘X, ⌘C, ⌘V, ⌘A, ⌘M).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacItem {
    About,
    Services,
    Hide,
    HideOthers,
    ShowAll,
    /// ⌘Q: AppKit's `terminate:`, which asks quit.rs first whether
    /// Banager quits now, as the Dock's Quit and a logout do.
    Quit,
    /// ⌘W: AppKit's `performClose:`, what the window's red button does,
    /// which hides the window rather than closing it (window.rs).
    CloseWindow,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Minimize,
    Zoom,
    BringAllToFront,
}

/// One entry of a menu, with its label in the menu bar's language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Mac(MacItem, String),
    Page(PageCommand, String),
    Separator,
}

/// One menu of the menu bar: Banager's own (the one macOS titles with the
/// app's name), File, Edit, View, Window, Help.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopMenu {
    /// The Window and Help menus carry tauri's ids for them, by which it
    /// makes them the app's Window menu -- macOS adds the window's name and
    /// its own arranging items -- and its Help menu, whose search field
    /// macOS adds.
    pub id: &'static str,
    pub label: String,
    pub items: Vec<Item>,
}

/// The menu bar's words in one language. `{app}` is the app's name.
/// Where a Mac app's menu has the item, the words are the ones macOS's own
/// menus use in that language -- Finder's, Safari's: 拷贝 and 拷貝, not
/// 复制 or 複製, and 显示 or 顯示方式 for View. The four pages are named as
/// the sidebar names them (`nav.*` in src/i18n/en.json and zh-CN.json),
/// which a test checks. In English and Simplified Chinese, Chinese and the
/// app's name are a typed space apart: the page's text-autospace draws that
/// gap in the window, and a menu on a macOS that draws none of its own would
/// run them together. Traditional Chinese types none, as Apple's own zh_TW
/// menus write it (Finder's 結束Finder): macOS 27's menus draw a 1/8 em gap
/// themselves, so a typed space would sit wider than Finder's.
struct Words {
    about: &'static str,
    settings: &'static str,
    services: &'static str,
    hide: &'static str,
    hide_others: &'static str,
    show_all: &'static str,
    quit: &'static str,
    file: &'static str,
    close_window: &'static str,
    edit: &'static str,
    undo: &'static str,
    redo: &'static str,
    cut: &'static str,
    copy: &'static str,
    paste: &'static str,
    select_all: &'static str,
    view: &'static str,
    overview: &'static str,
    updates: &'static str,
    installed: &'static str,
    unknown: &'static str,
    /// The page header's Check again, in its menu's title case.
    check_again: &'static str,
    search: &'static str,
    window: &'static str,
    minimize: &'static str,
    zoom: &'static str,
    bring_all_to_front: &'static str,
    help: &'static str,
    /// Help's first item of Banager's, `{app}` for the app's name, named
    /// as the sheet's title names it (`welcome.title` in src/i18n, whose
    /// `{{name}}` is the app's name) -- with the typed space the menu's
    /// other items have, but in Traditional Chinese exactly -- which a test
    /// checks.
    welcome: &'static str,
    /// Help's second, named as the sheet's title names it (`faq.title` in
    /// src/i18n), which a test checks.
    common_questions: &'static str,
    /// Help's third, named as the sheet's title names it
    /// (`shortcuts.title` in src/i18n), which a test checks.
    keyboard_shortcuts: &'static str,
    /// Help's fourth, named as the page names it (`setupCheck.menu` in
    /// src/i18n), which a test checks.
    check_tool_setup: &'static str,
    /// Help's fifth; Settings' button says the same
    /// (`diagnostics.copy` in src/i18n), which a test checks.
    copy_diagnostics: &'static str,
}

const ENGLISH: Words = Words {
    about: "About {app}",
    settings: "Settings…",
    services: "Services",
    hide: "Hide {app}",
    hide_others: "Hide Others",
    show_all: "Show All",
    quit: "Quit {app}",
    file: "File",
    close_window: "Close Window",
    edit: "Edit",
    undo: "Undo",
    redo: "Redo",
    cut: "Cut",
    copy: "Copy",
    paste: "Paste",
    select_all: "Select All",
    view: "View",
    overview: "Overview",
    updates: "Updates",
    installed: "Installed",
    unknown: "Other Programs",
    check_again: "Check Again",
    search: "Search",
    window: "Window",
    minimize: "Minimize",
    zoom: "Zoom",
    bring_all_to_front: "Bring All to Front",
    help: "Help",
    welcome: "Welcome to {app}",
    common_questions: "Common Questions",
    keyboard_shortcuts: "Keyboard Shortcuts",
    check_tool_setup: "Check Tool Setup…",
    copy_diagnostics: "Copy Diagnostic Info…",
};

const SIMPLIFIED_CHINESE: Words = Words {
    about: "关于 {app}",
    settings: "设置…",
    services: "服务",
    hide: "隐藏 {app}",
    hide_others: "隐藏其他",
    show_all: "全部显示",
    quit: "退出 {app}",
    file: "文件",
    close_window: "关闭窗口",
    edit: "编辑",
    undo: "撤销",
    redo: "重做",
    cut: "剪切",
    copy: "拷贝",
    paste: "粘贴",
    select_all: "全选",
    view: "显示",
    overview: "概览",
    updates: "更新",
    installed: "已安装",
    unknown: "其他程序",
    check_again: "重新检查",
    search: "搜索",
    window: "窗口",
    minimize: "最小化",
    zoom: "缩放",
    bring_all_to_front: "前置全部窗口",
    help: "帮助",
    welcome: "欢迎使用 {app}",
    common_questions: "常见问题",
    keyboard_shortcuts: "键盘快捷键",
    check_tool_setup: "检查工具环境…",
    copy_diagnostics: "拷贝诊断信息…",
};

const TRADITIONAL_CHINESE: Words = Words {
    common_questions: "常見問題",
    about: "關於{app}",
    settings: "設定…",
    services: "服務",
    hide: "隱藏{app}",
    hide_others: "隱藏其他",
    show_all: "顯示全部",
    quit: "結束{app}",
    file: "檔案",
    close_window: "關閉視窗",
    edit: "編輯",
    undo: "還原",
    redo: "重做",
    cut: "剪下",
    copy: "拷貝",
    paste: "貼上",
    select_all: "全選",
    view: "顯示方式",
    overview: "概覽",
    updates: "更新",
    installed: "已安裝",
    unknown: "其他程式",
    check_again: "重新檢查",
    search: "搜尋",
    window: "視窗",
    minimize: "縮到最小",
    zoom: "縮放",
    bring_all_to_front: "將此程式所有視窗移至最前",
    help: "輔助說明",
    welcome: "歡迎使用{app}",
    keyboard_shortcuts: "鍵盤快速鍵",
    check_tool_setup: "檢查工具環境…",
    copy_diagnostics: "拷貝診斷資訊…",
};

/// The menu bar in `language`, laid out as a Mac app's is: About, then
/// Settings…, Services, the three Hide items and Quit, each group apart;
/// File, with Close Window; the Edit menu a text field needs; View with the
/// sidebar's Overview, Updates and Installed and the last row under its
/// 「来源」, Other Programs, ⌘1 to ⌘4 as Finder's and Mail's are, then the
/// page's Check Again and Search; the Window menu; and Help, under the
/// search field macOS puts there: Welcome to Banager, Common Questions and
/// Keyboard Shortcuts, then, apart, Check Tool Setup and Copy Diagnostic
/// Info.
pub fn menu_bar(language: MenuLanguage, app_name: &str) -> Vec<TopMenu> {
    let words = match language {
        MenuLanguage::En => &ENGLISH,
        MenuLanguage::ZhCn => &SIMPLIFIED_CHINESE,
        MenuLanguage::ZhHant => &TRADITIONAL_CHINESE,
    };
    let named = |template: &str| template.replace("{app}", app_name);
    let mac = |item: MacItem, label: &str| Item::Mac(item, label.to_string());
    let page = |command: PageCommand, label: &str| Item::Page(command, label.to_string());
    vec![
        TopMenu {
            id: "app",
            label: app_name.to_string(),
            items: vec![
                mac(MacItem::About, &named(words.about)),
                Item::Separator,
                page(PageCommand::Settings, words.settings),
                Item::Separator,
                mac(MacItem::Services, words.services),
                Item::Separator,
                mac(MacItem::Hide, &named(words.hide)),
                mac(MacItem::HideOthers, words.hide_others),
                mac(MacItem::ShowAll, words.show_all),
                Item::Separator,
                mac(MacItem::Quit, &named(words.quit)),
            ],
        },
        TopMenu {
            id: "file",
            label: words.file.to_string(),
            items: vec![mac(MacItem::CloseWindow, words.close_window)],
        },
        TopMenu {
            id: "edit",
            label: words.edit.to_string(),
            items: vec![
                mac(MacItem::Undo, words.undo),
                mac(MacItem::Redo, words.redo),
                Item::Separator,
                mac(MacItem::Cut, words.cut),
                mac(MacItem::Copy, words.copy),
                mac(MacItem::Paste, words.paste),
                mac(MacItem::SelectAll, words.select_all),
            ],
        },
        TopMenu {
            id: "view",
            label: words.view.to_string(),
            items: vec![
                page(PageCommand::Overview, words.overview),
                page(PageCommand::Updates, words.updates),
                page(PageCommand::Installed, words.installed),
                page(PageCommand::Unknown, words.unknown),
                Item::Separator,
                page(PageCommand::CheckAgain, words.check_again),
                page(PageCommand::Search, words.search),
            ],
        },
        TopMenu {
            id: WINDOW_SUBMENU_ID,
            label: words.window.to_string(),
            items: vec![
                mac(MacItem::Minimize, words.minimize),
                mac(MacItem::Zoom, words.zoom),
                Item::Separator,
                mac(MacItem::BringAllToFront, words.bring_all_to_front),
            ],
        },
        TopMenu {
            id: HELP_SUBMENU_ID,
            label: words.help.to_string(),
            items: vec![
                page(PageCommand::Welcome, &named(words.welcome)),
                page(PageCommand::CommonQuestions, words.common_questions),
                page(PageCommand::KeyboardShortcuts, words.keyboard_shortcuts),
                Item::Separator,
                page(PageCommand::CheckToolSetup, words.check_tool_setup),
                page(PageCommand::CopyDiagnostics, words.copy_diagnostics),
            ],
        },
    ]
}

/// The language to build the menu bar in before the page has said: the
/// one it is about to choose. Settings' language when it names one;
/// following the system, the page's i18n takes the language WebKit
/// reports, the first of the user's preferred languages, and reads any
/// Chinese script or region as Simplified or Traditional Chinese, and
/// anything else as English. Should the
/// two ever differ, the page's word wins as soon as it arrives
/// (`set_menu_language`); this only spares a Chinese Mac an English menu
/// bar while the window loads.
pub fn initial_language(setting: Language, preferred: &[String]) -> MenuLanguage {
    match setting {
        Language::En => MenuLanguage::En,
        Language::ZhCn => MenuLanguage::ZhCn,
        Language::ZhHant => MenuLanguage::ZhHant,
        Language::System => {
            let first = preferred
                .first()
                .map(|s| s.to_ascii_lowercase().replace('_', "-"));
            let parts: Vec<&str> = first.as_deref().unwrap_or("").split('-').collect();
            if parts.first() != Some(&"zh") {
                MenuLanguage::En
            } else if parts.contains(&"hant") {
                MenuLanguage::ZhHant
            } else if parts.contains(&"hans") {
                MenuLanguage::ZhCn
            } else if parts.iter().any(|part| ["tw", "hk", "mo"].contains(part)) {
                MenuLanguage::ZhHant
            } else {
                MenuLanguage::ZhCn
            }
        }
    }
}

/// The user's preferred languages, most preferred first, as macOS keeps
/// them ("zh-Hans-CN", "en-US"): `NSLocale preferredLanguages`, which
/// reads the user's settings and runs nothing.
#[cfg(target_os = "macos")]
pub fn preferred_languages() -> Vec<String> {
    objc2_foundation::NSLocale::preferredLanguages()
        .iter()
        .map(|language| language.to_string())
        .collect()
}

#[cfg(not(target_os = "macos"))]
pub fn preferred_languages() -> Vec<String> {
    Vec::new()
}

/// Tauri's menu for `menus`. Only on the main thread of a running app,
/// as every tauri menu item is made; `menu_bar` is what the tests read.
fn build<R: Runtime>(app: &AppHandle<R>, menus: &[TopMenu]) -> tauri::Result<Menu<R>> {
    let bar = Menu::new(app)?;
    for top in menus {
        let submenu = Submenu::with_id(app, top.id, &top.label, true)?;
        for item in &top.items {
            match item {
                Item::Mac(mac, label) => submenu.append(&mac_item(app, *mac, label)?)?,
                Item::Page(command, label) => submenu.append(&MenuItem::with_id(
                    app,
                    command.id(),
                    label,
                    true,
                    command.shortcut(),
                )?)?,
                Item::Separator => submenu.append(&PredefinedMenuItem::separator(app)?)?,
            }
        }
        bar.append(&submenu)?;
    }
    Ok(bar)
}

fn mac_item<R: Runtime>(
    app: &AppHandle<R>,
    item: MacItem,
    label: &str,
) -> tauri::Result<PredefinedMenuItem<R>> {
    let label = Some(label);
    match item {
        MacItem::About => PredefinedMenuItem::about(app, label, Some(about_panel(app))),
        MacItem::Services => PredefinedMenuItem::services(app, label),
        MacItem::Hide => PredefinedMenuItem::hide(app, label),
        MacItem::HideOthers => PredefinedMenuItem::hide_others(app, label),
        MacItem::ShowAll => PredefinedMenuItem::show_all(app, label),
        MacItem::Quit => PredefinedMenuItem::quit(app, label),
        MacItem::CloseWindow => PredefinedMenuItem::close_window(app, label),
        MacItem::Undo => PredefinedMenuItem::undo(app, label),
        MacItem::Redo => PredefinedMenuItem::redo(app, label),
        MacItem::Cut => PredefinedMenuItem::cut(app, label),
        MacItem::Copy => PredefinedMenuItem::copy(app, label),
        MacItem::Paste => PredefinedMenuItem::paste(app, label),
        MacItem::SelectAll => PredefinedMenuItem::select_all(app, label),
        MacItem::Minimize => PredefinedMenuItem::minimize(app, label),
        // tauri's "maximize" is AppKit's Zoom on a Mac (`performZoom:`).
        MacItem::Zoom => PredefinedMenuItem::maximize(app, label),
        MacItem::BringAllToFront => PredefinedMenuItem::bring_all_to_front(app, label),
    }
}

/// What macOS's About panel shows: the app's name and version, as
/// tauri's default menu gave it -- named here so a window run from
/// `tauri dev`, outside an app bundle, has them too -- and the icon and
/// the rest AppKit finds itself.
fn about_panel<R: Runtime>(app: &AppHandle<R>) -> AboutMetadata<'static> {
    let package = app.package_info();
    AboutMetadata {
        name: Some(package.name.clone()),
        version: Some(package.version.to_string()),
        copyright: app.config().bundle.copyright.clone(),
        ..Default::default()
    }
}

/// The language the menu bar is in, once it is up. The page often names
/// the one it is in already -- at startup, the one `initial_language`
/// guessed -- and that rebuilds nothing.
#[derive(Default)]
pub struct MenuBar {
    language: Mutex<Option<MenuLanguage>>,
}

impl MenuBar {
    /// The language the menu bar is in, once it is up: the window's, once
    /// the page has said which. The update notification is written in it
    /// too (`notify::report_update_set`).
    pub fn language(&self) -> Option<MenuLanguage> {
        *self.language.lock().unwrap()
    }
}

/// Puts up the menu bar in `language`, unless it is in it already.
/// `run()` manages `MenuBar` on the builder, before any of this can run.
pub fn show<R: Runtime>(app: &AppHandle<R>, language: MenuLanguage) -> tauri::Result<()> {
    let state = app.state::<MenuBar>();
    let mut current = state.language.lock().unwrap();
    if *current == Some(language) {
        return Ok(());
    }
    app.set_menu(build(app, &menu_bar(language, &app.package_info().name))?)?;
    *current = Some(language);
    Ok(())
}

/// The language the page uses, which the menu bar follows:
/// src/i18n/useLanguageSync.ts sends the one its i18n resolved, at startup
/// and at every change -- an override in Settings, or following the
/// system again.
///
/// Not `async`, as the commands in ipc.rs are: a command that is not runs
/// on the main thread, the one AppKit's menus are made on, so the menu is
/// built there directly instead of each item being handed over to it and
/// waited for, and building one is quick. A failure leaves the menu bar
/// in the language it had; the page only logs it.
#[tauri::command]
pub fn set_menu_language(app: AppHandle, language: MenuLanguage) -> Result<(), String> {
    show(&app, language).map_err(|e| e.to_string())
}

/// Tells the window that one of the items acting in the page was chosen:
/// its event (`PageCommand::event`), to the window alone, once the window
/// is back on screen (`window::show_and_tell`) -- closed, or in the Dock,
/// it would show nothing of what the item does. Every other item's action
/// is macOS's and needs nothing from here.
pub fn forward_to_page<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let Some(command) = PageCommand::from_id(id) else {
        return;
    };
    if let Err(e) = window::show_and_tell(app, command.event()) {
        eprintln!(
            "[banager] could not tell the window {} was chosen: {e}",
            command.id()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each menu's label, and its items' labels, with "—" for a separator.
    fn labels(menus: &[TopMenu]) -> Vec<(String, Vec<String>)> {
        menus
            .iter()
            .map(|menu| {
                let items = menu
                    .items
                    .iter()
                    .map(|item| match item {
                        Item::Mac(_, label) | Item::Page(_, label) => label.clone(),
                        Item::Separator => "—".to_string(),
                    })
                    .collect();
                (menu.label.clone(), items)
            })
            .collect()
    }

    /// The same bar with every label blanked: what must not differ between
    /// the two languages.
    fn shape(menus: &[TopMenu]) -> Vec<TopMenu> {
        menus
            .iter()
            .map(|menu| TopMenu {
                id: menu.id,
                label: String::new(),
                items: menu
                    .items
                    .iter()
                    .map(|item| match item {
                        Item::Mac(mac, _) => Item::Mac(*mac, String::new()),
                        Item::Page(command, _) => Item::Page(*command, String::new()),
                        Item::Separator => Item::Separator,
                    })
                    .collect(),
            })
            .collect()
    }

    fn owned(menus: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        menus
            .iter()
            .map(|(menu, items)| {
                (
                    menu.to_string(),
                    items.iter().map(|item| item.to_string()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn test_the_english_menu_bar_is_a_mac_apps() {
        let bar = menu_bar(MenuLanguage::En, "Banager");
        assert_eq!(
            labels(&bar),
            owned(&[
                (
                    "Banager",
                    &[
                        "About Banager",
                        "—",
                        "Settings…",
                        "—",
                        "Services",
                        "—",
                        "Hide Banager",
                        "Hide Others",
                        "Show All",
                        "—",
                        "Quit Banager",
                    ],
                ),
                ("File", &["Close Window"]),
                (
                    "Edit",
                    &["Undo", "Redo", "—", "Cut", "Copy", "Paste", "Select All"],
                ),
                (
                    "View",
                    &[
                        "Overview",
                        "Updates",
                        "Installed",
                        "Other Programs",
                        "—",
                        "Check Again",
                        "Search",
                    ],
                ),
                ("Window", &["Minimize", "Zoom", "—", "Bring All to Front"]),
                (
                    "Help",
                    &[
                        "Welcome to Banager",
                        "Common Questions",
                        "Keyboard Shortcuts",
                        "—",
                        "Check Tool Setup…",
                        "Copy Diagnostic Info…",
                    ],
                ),
            ])
        );
    }

    #[test]
    fn test_the_chinese_menu_bar_has_macos_own_chinese_words() {
        let bar = menu_bar(MenuLanguage::ZhCn, "Banager");
        assert_eq!(
            labels(&bar),
            owned(&[
                (
                    "Banager",
                    &[
                        "关于 Banager",
                        "—",
                        "设置…",
                        "—",
                        "服务",
                        "—",
                        "隐藏 Banager",
                        "隐藏其他",
                        "全部显示",
                        "—",
                        "退出 Banager",
                    ],
                ),
                ("文件", &["关闭窗口"]),
                (
                    "编辑",
                    &["撤销", "重做", "—", "剪切", "拷贝", "粘贴", "全选"],
                ),
                (
                    "显示",
                    &[
                        "概览",
                        "更新",
                        "已安装",
                        "其他程序",
                        "—",
                        "重新检查",
                        "搜索"
                    ],
                ),
                ("窗口", &["最小化", "缩放", "—", "前置全部窗口"]),
                (
                    "帮助",
                    &[
                        "欢迎使用 Banager",
                        "常见问题",
                        "键盘快捷键",
                        "—",
                        "检查工具环境…",
                        "拷贝诊断信息…"
                    ]
                ),
            ])
        );
    }

    /// Whether `c` is a Chinese character.
    fn is_han(c: char) -> bool {
        matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}')
    }

    /// `text` with a space typed wherever a Chinese character meets a
    /// Latin letter or a digit: the gap the page's text-autospace draws.
    fn spaced(text: &str) -> String {
        let mut out = String::new();
        let mut previous: Option<char> = None;
        for c in text.chars() {
            if let Some(p) = previous {
                if (is_han(p) && c.is_ascii_alphanumeric())
                    || (p.is_ascii_alphanumeric() && is_han(c))
                {
                    out.push(' ');
                }
            }
            out.push(c);
            previous = Some(c);
        }
        out
    }

    /// `text` with every space between a Chinese character and a Latin
    /// letter or a digit taken out: how Apple's own zh_TW menus write it
    /// (Finder's 結束Finder, 關於Finder).
    fn unspaced(text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::new();
        for (i, &c) in chars.iter().enumerate() {
            if c == ' ' && i > 0 && i + 1 < chars.len() {
                let (p, n) = (chars[i - 1], chars[i + 1]);
                if (is_han(p) && n.is_ascii_alphanumeric())
                    || (p.is_ascii_alphanumeric() && is_han(n))
                {
                    continue;
                }
            }
            out.push(c);
        }
        out
    }

    #[test]
    fn test_no_menu_item_runs_chinese_into_the_apps_name() {
        // The window's text-autospace never reaches a menu, so there the
        // gap between Chinese and Latin is typed: without it, a macOS that
        // draws no gap of its own shows 关于Banager.
        for language in [MenuLanguage::En, MenuLanguage::ZhCn] {
            for (menu, items) in labels(&menu_bar(language, "Banager")) {
                for label in std::iter::once(&menu).chain(&items) {
                    assert_eq!(label, &spaced(label), "{language:?}");
                }
            }
        }
        assert_eq!(
            labels(&menu_bar(MenuLanguage::ZhCn, "Banager"))[5].1[0],
            "欢迎使用 Banager"
        );
    }

    #[test]
    fn test_traditional_chinese_menu_writes_banager_as_apples_own_menus_do() {
        // Apple's own zh_TW menus type no space between Chinese and a
        // Latin name -- Finder's 結束Finder, 關於Finder -- and macOS draws
        // the gap itself (1/8 em on macOS 27). The window's
        // `welcome.title`, 歡迎使用{{name}}, has none either.
        for (menu, items) in labels(&menu_bar(MenuLanguage::ZhHant, "Banager")) {
            for label in std::iter::once(&menu).chain(&items) {
                assert_eq!(label, &unspaced(label));
            }
        }
        let bar = menu_bar(MenuLanguage::ZhHant, "Banager");
        let words = labels(&bar);
        assert_eq!(
            words[0].1,
            [
                "關於Banager",
                "—",
                "設定…",
                "—",
                "服務",
                "—",
                "隱藏Banager",
                "隱藏其他",
                "顯示全部",
                "—",
                "結束Banager",
            ]
            .map(String::from)
            .to_vec()
        );
        assert_eq!(words[5].1[0], "歡迎使用Banager");
    }

    #[test]
    fn test_traditional_chinese_menu_uses_taiwan_words() {
        let bar = menu_bar(MenuLanguage::ZhHant, "Banager");
        let words = labels(&bar);
        assert_eq!(words[1], ("檔案".into(), vec!["關閉視窗".into()]));
        assert_eq!(
            words[2],
            (
                "編輯".into(),
                ["還原", "重做", "—", "剪下", "拷貝", "貼上", "全選"]
                    .map(String::from)
                    .to_vec()
            )
        );
        assert_eq!(words[3].0, "顯示方式");
        // Bring All to Front as AppKit's own zh_TW MenuCommands table, and
        // Finder's and TextEdit's Window menus, say it: not the shorter
        // 將全部移至最前 a walkthrough asked for (walk-5 W5-11).
        assert_eq!(
            words[4],
            (
                "視窗".into(),
                ["縮到最小", "縮放", "—", "將此程式所有視窗移至最前"]
                    .map(String::from)
                    .to_vec()
            )
        );
        assert_eq!(words[5].0, "輔助說明");
        assert!(words[5].1.contains(&"常見問題".to_owned()));
    }

    #[test]
    fn test_only_the_words_differ_between_the_three_languages() {
        let english = menu_bar(MenuLanguage::En, "Banager");
        assert_eq!(
            shape(&english),
            shape(&menu_bar(MenuLanguage::ZhCn, "Banager"))
        );
        assert_eq!(
            shape(&english),
            shape(&menu_bar(MenuLanguage::ZhHant, "Banager"))
        );
        assert_eq!(
            english.iter().map(|menu| menu.id).collect::<Vec<_>>(),
            [
                "app",
                "file",
                "edit",
                "view",
                WINDOW_SUBMENU_ID,
                HELP_SUBMENU_ID
            ]
        );
    }

    #[test]
    fn test_every_item_but_the_pages_twelve_is_macos_own() {
        let bar = menu_bar(MenuLanguage::En, "Banager");
        let items: Vec<&Item> = bar.iter().flat_map(|menu| &menu.items).collect();
        let pages: Vec<PageCommand> = items
            .iter()
            .filter_map(|item| match item {
                Item::Page(command, _) => Some(*command),
                _ => None,
            })
            .collect();
        assert_eq!(pages, PageCommand::ALL);
        let macs: Vec<MacItem> = items
            .iter()
            .filter_map(|item| match item {
                Item::Mac(mac, _) => Some(*mac),
                _ => None,
            })
            .collect();
        assert_eq!(
            macs,
            [
                MacItem::About,
                MacItem::Services,
                MacItem::Hide,
                MacItem::HideOthers,
                MacItem::ShowAll,
                MacItem::Quit,
                MacItem::CloseWindow,
                MacItem::Undo,
                MacItem::Redo,
                MacItem::Cut,
                MacItem::Copy,
                MacItem::Paste,
                MacItem::SelectAll,
                MacItem::Minimize,
                MacItem::Zoom,
                MacItem::BringAllToFront,
            ]
        );
    }

    #[test]
    fn test_the_app_menu_is_named_for_the_app_it_is_given() {
        let bar = menu_bar(MenuLanguage::ZhCn, "Other");
        assert_eq!(bar[0].label, "Other");
        assert_eq!(
            bar[0].items[0],
            Item::Mac(MacItem::About, "关于 Other".to_string())
        );
        assert_eq!(
            bar[0].items.last(),
            Some(&Item::Mac(MacItem::Quit, "退出 Other".to_string()))
        );
    }

    #[test]
    fn test_each_page_item_has_its_id_event_and_shortcut() {
        let described: Vec<(&str, &str, Option<&str>)> = PageCommand::ALL
            .into_iter()
            .map(|command| (command.id(), command.event(), command.shortcut()))
            .collect();
        assert_eq!(
            described,
            [
                ("settings", "menu://settings", Some("CmdOrCtrl+Comma")),
                ("overview", "menu://overview", Some("CmdOrCtrl+1")),
                ("updates", "menu://updates", Some("CmdOrCtrl+2")),
                ("installed", "menu://installed", Some("CmdOrCtrl+3")),
                ("unknown", "menu://unknown", Some("CmdOrCtrl+4")),
                ("check-again", "menu://check-again", Some("CmdOrCtrl+R")),
                ("search", "menu://search", Some("CmdOrCtrl+F")),
                ("welcome", "menu://welcome", None),
                ("common-questions", "menu://common-questions", None),
                ("keyboard-shortcuts", "menu://keyboard-shortcuts", None),
                ("check-tool-setup", "menu://check-tool-setup", None),
                ("copy-diagnostics", "menu://copy-diagnostics", None),
            ]
        );
        for command in PageCommand::ALL {
            assert_eq!(PageCommand::from_id(command.id()), Some(command));
        }
        // What macOS's own items carry: tauri's counted ids, never these.
        assert_eq!(PageCommand::from_id("1"), None);
        assert_eq!(PageCommand::from_id("menu://search"), None);
    }

    #[test]
    fn test_each_page_items_shortcut_is_one_tauri_reads_as_command_and_its_key() {
        // tauri would drop a shortcut it cannot read, and say nothing.
        use muda::accelerator::{Accelerator, Code, Modifiers};
        // One key for each item, `None` for one with no shortcut: a count
        // apart would not compile.
        let keys: [Option<Code>; PageCommand::ALL.len()] = [
            Some(Code::Comma),
            Some(Code::Digit1),
            Some(Code::Digit2),
            Some(Code::Digit3),
            Some(Code::Digit4),
            Some(Code::KeyR),
            Some(Code::KeyF),
            None,
            None,
            None,
            None,
            None,
        ];
        for (command, key) in PageCommand::ALL.into_iter().zip(keys) {
            let (Some(shortcut), Some(key)) = (command.shortcut(), key) else {
                assert_eq!(command.shortcut(), None, "{command:?}");
                assert_eq!(key, None, "{command:?}");
                continue;
            };
            let read: Accelerator = shortcut
                .parse()
                .unwrap_or_else(|e| panic!("{shortcut:?} is no shortcut tauri reads: {e}"));
            // CmdOrCtrl: ⌘ on a Mac, Ctrl elsewhere.
            let modifier = if cfg!(target_os = "macos") {
                Modifiers::SUPER
            } else {
                Modifiers::CONTROL
            };
            assert_eq!(read, Accelerator::new(Some(modifier), key));
        }
    }

    #[test]
    fn test_the_pages_are_named_as_the_sidebar_names_them() {
        // The sidebar's words: `nav.*` in the page's two locales. Settings…
        // is the sidebar's Settings with the ellipsis every Mac app's
        // Settings… has.
        for (words, locale) in [
            (&ENGLISH, include_str!("../../src/i18n/en.json")),
            (
                &TRADITIONAL_CHINESE,
                include_str!("../../src/i18n/zh-Hant.json"),
            ),
            (
                &SIMPLIFIED_CHINESE,
                include_str!("../../src/i18n/zh-CN.json"),
            ),
        ] {
            let locale: serde_json::Value = serde_json::from_str(locale).unwrap();
            let nav = |key: &str| {
                locale["nav"][key]
                    .as_str()
                    .unwrap_or_else(|| panic!("no nav.{key} in the page's words"))
                    .to_string()
            };
            assert_eq!(
                [
                    words.overview,
                    words.updates,
                    words.installed,
                    words.unknown
                ],
                [
                    nav("overview"),
                    nav("updates"),
                    nav("installed"),
                    nav("unknown")
                ]
            );
            assert_eq!(words.settings, format!("{}…", nav("settings")));
            // Help's item is named as Settings' button is, with the
            // ellipsis of an item that leads to one more step: the button.
            assert_eq!(
                words.copy_diagnostics,
                format!("{}…", locale["diagnostics"]["copy"].as_str().unwrap())
            );
            // Welcome to Banager as the sheet's title names it: in
            // Traditional Chinese exactly, as Apple's zh_TW menus type no
            // space before a Latin name; elsewhere with the space the
            // page's text-autospace draws typed in.
            let title = locale["welcome"]["title"]
                .as_str()
                .unwrap()
                .replace("{{name}}", "Banager");
            let expected = if std::ptr::eq(words, &TRADITIONAL_CHINESE) {
                title
            } else {
                spaced(&title)
            };
            assert_eq!(words.welcome.replace("{app}", "Banager"), expected);
            // Common Questions as the sheet's title names it.
            assert_eq!(
                words.common_questions,
                locale["faq"]["title"].as_str().unwrap()
            );
            // Keyboard Shortcuts as the sheet's title names it.
            assert_eq!(
                words.keyboard_shortcuts,
                locale["shortcuts"]["title"].as_str().unwrap()
            );
            // And Check Tool Setup… as the page names it.
            assert_eq!(
                words.check_tool_setup,
                locale["setupCheck"]["menu"].as_str().unwrap()
            );
        }
    }

    #[test]
    fn test_the_page_can_name_only_the_three_languages() {
        let parse = |value: &str| serde_json::from_value::<MenuLanguage>(serde_json::json!(value));
        assert_eq!(parse("en").unwrap(), MenuLanguage::En);
        assert_eq!(parse("zh-CN").unwrap(), MenuLanguage::ZhCn);
        assert_eq!(parse("zh-Hant").unwrap(), MenuLanguage::ZhHant);
        for other in [
            "", "EN", "zh", "zh-cn", "zh-TW", "fr", "En", "ZhCn", "System",
        ] {
            assert!(parse(other).is_err(), "{other:?} was taken for a language");
        }
        assert!(serde_json::from_value::<MenuLanguage>(serde_json::json!(1)).is_err());
    }

    #[test]
    fn test_settings_language_decides_when_it_names_one() {
        let chinese_mac = ["zh-Hans-CN".to_string(), "en-CN".to_string()];
        assert_eq!(
            initial_language(Language::En, &chinese_mac),
            MenuLanguage::En
        );
        assert_eq!(initial_language(Language::ZhCn, &[]), MenuLanguage::ZhCn);
        assert_eq!(
            initial_language(Language::ZhHant, &chinese_mac),
            MenuLanguage::ZhHant
        );
    }

    #[test]
    fn test_following_the_system_takes_its_first_preferred_language() {
        let system = |languages: &[&str]| {
            let languages: Vec<String> = languages.iter().map(|l| l.to_string()).collect();
            initial_language(Language::System, &languages)
        };
        assert_eq!(system(&["zh-Hans-CN", "en-CN"]), MenuLanguage::ZhCn);
        for locale in [
            "zh-TW",
            "zh-HK",
            "zh-MO",
            "zh-Hant",
            "zh-Hant-TW",
            "zh-Hant-CN",
            "ZH_tw",
        ] {
            assert_eq!(system(&[locale]), MenuLanguage::ZhHant, "{locale}");
        }
        for locale in ["zh-CN", "zh-SG", "zh-Hans", "zh-Hans-HK", "zh-Hans-CN"] {
            assert_eq!(system(&[locale]), MenuLanguage::ZhCn, "{locale}");
        }
        assert_eq!(system(&["zh"]), MenuLanguage::ZhCn);
        assert_eq!(system(&["en-US", "zh-Hans-CN"]), MenuLanguage::En);
        assert_eq!(system(&["ja-JP", "zh-Hans-CN"]), MenuLanguage::En);
        // Zhuang, whose three-letter code starts with Chinese's two.
        assert_eq!(system(&["zha"]), MenuLanguage::En);
        assert_eq!(system(&[]), MenuLanguage::En);
    }
}
