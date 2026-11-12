//! Every text the user reads, in both languages. A new text is a new field of
//! [`Text`], so the compiler asks for it in each language.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Ru,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyError {
    /// global-hotkey has no code for this key.
    Unsupported,
    /// Another program holds it (Windows does not say which).
    Taken,
}

pub struct Text {
    pub empty_hint: &'static str,
    pub drop_hint: &'static str,
    pub clear: &'static str,
    /// After the counter in the shelf header: how to copy instead of move.
    pub ctrl_copy: &'static str,
    /// Hover hint of the header counter, which drags everything out.
    pub drag_all: &'static str,
    pub settings: &'static str,
    pub missing: &'static str,
    pub remove: &'static str,

    pub settings_title: &'static str,
    pub place: &'static str,
    pub place_hint: &'static str,
    pub left: &'static str,
    pub right: &'static str,
    pub top: &'static str,
    pub center: &'static str,
    pub bottom: &'static str,
    pub hotkey: &'static str,
    pub hotkey_what: &'static str,
    pub hotkey_press: &'static str,
    pub change: &'static str,
    pub change_hint: &'static str,
    pub assign_anyway: &'static str,
    pub cancel: &'static str,
    pub behavior: &'static str,
    pub auto_show: &'static str,
    /// Under the switch: how the shelf still opens with it off.
    pub auto_show_hint: &'static str,
    pub autostart: &'static str,
    pub language: &'static str,
    pub version: &'static str,
    pub project_page: &'static str,
    pub check_updates: &'static str,
    pub check_updates_hint: &'static str,
    pub download: &'static str,

    pub tray_toggle: &'static str,
    pub quit: &'static str,
}

const EN: Text = Text {
    empty_hint: "Drag files here",
    drop_hint: "Drop to add",
    clear: "Clear",
    ctrl_copy: "Ctrl: copy",
    drag_all: "Drag to take everything off the shelf",
    settings: "Settings",
    missing: "File not found. Click to remove",
    remove: "Remove from shelf",

    settings_title: "Shelve settings",
    place: "Position",
    place_hint: "Click the spot on the screen picture where the shelf should appear.",
    left: "Left",
    right: "Right",
    top: "top",
    center: "center",
    bottom: "bottom",
    hotkey: "Hotkey",
    hotkey_what: "Show or hide the shelf",
    hotkey_press: "Press a shortcut… Esc cancels",
    change: "Change",
    change_hint: "Click, then press the shortcut",
    assign_anyway: "Assign anyway",
    cancel: "Cancel",
    behavior: "Behavior",
    auto_show: "Show the shelf when a file is dragged to the edge",
    auto_show_hint: "When off, open the shelf with the hotkey, the tray icon or the tab at the screen edge (it shows while the shelf is not empty)",
    autostart: "Start with Windows",
    language: "Language",
    version: "Shelve, version",
    project_page: "Project page",
    check_updates: "Check for updates",
    check_updates_hint: "Once a day, asks GitHub if a new version is out",
    download: "Download",

    tray_toggle: "Show/hide",
    quit: "Exit",
};

const RU: Text = Text {
    empty_hint: "Перетащите файлы сюда",
    drop_hint: "Отпустите, чтобы положить",
    clear: "Очистить",
    ctrl_copy: "Ctrl — копия",
    drag_all: "Перетащите, чтобы забрать с полки всё",
    settings: "Настройки",
    missing: "Файл не найден. Щёлкните, чтобы убрать",
    remove: "Убрать с полки",

    settings_title: "Shelve: настройки",
    place: "Расположение",
    place_hint: "Щёлкните по месту на картинке экрана, где должна появляться полка.",
    left: "Слева",
    right: "Справа",
    top: "сверху",
    center: "по центру",
    bottom: "снизу",
    hotkey: "Горячая клавиша",
    hotkey_what: "Показать или спрятать полку",
    hotkey_press: "Нажмите сочетание… Esc: отмена",
    change: "Изменить",
    change_hint: "Нажмите и затем нужное сочетание",
    assign_anyway: "Всё равно назначить",
    cancel: "Отмена",
    behavior: "Поведение",
    auto_show: "Показывать полку, когда файл тащат к краю",
    auto_show_hint: "Если выключено, полку открывают горячая клавиша, значок в трее и язычок у края экрана (он виден, когда полка не пуста)",
    autostart: "Запускать вместе с Windows",
    language: "Язык",
    version: "Shelve, версия",
    project_page: "Страница проекта",
    check_updates: "Проверять обновления",
    check_updates_hint: "Раз в день спрашивает GitHub, нет ли новой версии",
    download: "Скачать",

    tray_toggle: "Показать/скрыть",
    quit: "Выход",
};

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Ru];

    /// The language's own name, the same whatever language is chosen.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Ru => "Русский",
        }
    }

    pub fn text(self) -> &'static Text {
        match self {
            Lang::En => &EN,
            Lang::Ru => &RU,
        }
    }

    /// "3 items": the shelf header counter. Items, not files: folders count too.
    pub fn count(self, n: usize) -> String {
        let word = match self {
            Lang::En if n == 1 => "item",
            Lang::En => "items",
            Lang::Ru => match (n % 10, n % 100) {
                (_, 11..=14) => "объектов",
                (1, _) => "объект",
                (2..=4, _) => "объекта",
                _ => "объектов",
            },
        };
        format!("{n} {word}")
    }

    /// Why the hotkey from the settings did not register at startup.
    pub fn hotkey_status_error(self, combo: &str, why: HotkeyError) -> String {
        match (self, why) {
            (Lang::En, HotkeyError::Taken) => format!("{combo} is taken by another program"),
            (Lang::En, HotkeyError::Unsupported) => format!("{combo} cannot be used"),
            (Lang::Ru, HotkeyError::Taken) => format!("{combo} занято другой программой"),
            (Lang::Ru, HotkeyError::Unsupported) => format!("{combo} нельзя использовать"),
        }
    }

    pub fn update_out(self, version: &str) -> String {
        match self {
            Lang::En => format!("Shelve {version} is out"),
            Lang::Ru => format!("Вышла Shelve {version}"),
        }
    }

    pub fn update_have(self, version: &str) -> String {
        match self {
            Lang::En => format!("You have {version}"),
            Lang::Ru => format!("У вас {version}"),
        }
    }

    pub fn hotkey_failed(self, why: &str) -> String {
        match self {
            Lang::En => format!("Not working: {why}"),
            Lang::Ru => format!("Не работает: {why}"),
        }
    }

    pub fn hotkey_breaks_typing(self, combo: &str) -> String {
        match self {
            Lang::En => format!("{combo} without Ctrl or Alt will stop typing in every program."),
            Lang::Ru => {
                format!("{combo} без Ctrl или Alt перестанет печататься во всех программах.")
            }
        }
    }

    pub fn hotkey_unsupported(self, combo: &str) -> String {
        match self {
            Lang::En => format!("{combo} cannot be assigned, choose another"),
            Lang::Ru => format!("{combo} нельзя назначить, выберите другое"),
        }
    }

    pub fn hotkey_taken(self, combo: &str, kept: &str) -> String {
        match self {
            Lang::En => format!("{combo} is taken by another program, {kept} stays"),
            Lang::Ru => format!("{combo} занято другой программой, осталось {kept}"),
        }
    }

    pub fn autostart_failed(self, why: &str) -> String {
        match self {
            Lang::En => format!("Could not change autostart: {why}"),
            Lang::Ru => format!("Не удалось изменить автозапуск: {why}"),
        }
    }

    pub fn tray_hotkey(self, hotkey: &str) -> String {
        match self {
            Lang::En => format!("Hotkey: {hotkey}"),
            Lang::Ru => format!("Горячая клавиша: {hotkey}"),
        }
    }

    pub fn tray_hotkey_failed(self, why: &str) -> String {
        match self {
            Lang::En => format!("Hotkey is not working: {why}"),
            Lang::Ru => format!("Горячая клавиша не работает: {why}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_counts_one_item_and_many_items() {
        for (n, s) in [
            (1, "1 item"),
            (2, "2 items"),
            (11, "11 items"),
            (21, "21 items"),
        ] {
            assert_eq!(Lang::En.count(n), s);
        }
    }

    #[test]
    fn russian_count_uses_plural_forms() {
        for (n, s) in [
            (1, "1 объект"),
            (2, "2 объекта"),
            (4, "4 объекта"),
            (5, "5 объектов"),
            (11, "11 объектов"),
            (14, "14 объектов"),
            (21, "21 объект"),
            (22, "22 объекта"),
            (100, "100 объектов"),
            (111, "111 объектов"),
        ] {
            assert_eq!(Lang::Ru.count(n), s);
        }
    }

    #[test]
    fn auto_show_hint_names_the_hotkey_from_the_settings() {
        for lang in Lang::ALL {
            let t = lang.text();
            let hint = t.auto_show_hint.to_lowercase();
            assert!(hint.contains(&t.hotkey.to_lowercase()), "{lang:?}: {hint}");
        }
    }

    #[test]
    fn hotkey_status_error_names_the_combo_in_the_chosen_language() {
        use HotkeyError::*;
        for (lang, why, s) in [
            (Lang::En, Taken, "Ctrl+Alt+D is taken by another program"),
            (Lang::En, Unsupported, "Ctrl+Alt+D cannot be used"),
            (Lang::Ru, Taken, "Ctrl+Alt+D занято другой программой"),
            (Lang::Ru, Unsupported, "Ctrl+Alt+D нельзя использовать"),
        ] {
            assert_eq!(lang.hotkey_status_error("Ctrl+Alt+D", why), s);
        }
    }
}
