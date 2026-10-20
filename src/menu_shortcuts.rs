//! One accelerator per fixed menu command, shared by every UI language.
//! wxWidgets spells Command as Ctrl and Option as Alt on macOS.
//! Dynamic episodes, favorites and recent files deliberately have no accelerator.

pub const BINDINGS: &[(i32, &str)] = &[
    (crate::documents::ID_NEW_DOCUMENT, "Ctrl+N"),
    (crate::documents::ID_CLOSE_DOCUMENT, "Ctrl+W"),
    (crate::documents::ID_NEXT_DOCUMENT, "Ctrl+<"),
    (crate::documents::ID_PREVIOUS_DOCUMENT, "Ctrl+>"),
    (crate::ID_OPEN, "Ctrl+O"),
    (crate::ID_SAVE_TEXT, "Ctrl+S"),
    (crate::ID_SAVE_TEXT_AS, "Ctrl+Shift+S"),
    (crate::ID_EXIT, "Ctrl+Q"),
    (crate::ID_START_PLAYBACK, "Ctrl+L"),
    (crate::ID_PLAY_PAUSE, "Ctrl+P"),
    (crate::ID_STOP, "Ctrl+."),
    (crate::ID_SAVE, "Ctrl+Alt+A"),
    (crate::ID_SETTINGS, "Ctrl+,"),
    (crate::ID_RECENT_ARTICLES, "Ctrl+-"),
    (crate::ID_ARTICLES_ADD_SOURCE, "Ctrl+Shift+A"),
    (crate::ID_ARTICLES_EDIT_SOURCE, "Ctrl+Shift+E"),
    (crate::ID_ARTICLES_DELETE_SOURCE, "Ctrl+Shift+D"),
    (crate::ID_ARTICLES_REORDER_SOURCES, "Ctrl+Shift+R"),
    (
        crate::ID_ARTICLES_SORT_SOURCES_ALPHABETICALLY,
        "Ctrl+Shift+T",
    ),
    (crate::ID_ARTICLES_IMPORT_SOURCES, "Ctrl+Shift+I"),
    (crate::ID_ARTICLES_EXPORT_SOURCES, "Ctrl+Shift+O"),
    (crate::ID_ARTICLES_RESTORE_DEFAULT_SOURCES, "Ctrl+Shift+U"),
    (crate::ID_ARTICLES_ADD_COMMUNITY_SOURCE, "Ctrl+Shift+C"),
    (crate::ID_ARTICLES_COMMUNITY_SOURCES, "Ctrl+Shift+G"),
    (crate::ID_PODCASTS_ADD, "Ctrl+Shift+P"),
    (crate::ID_PODCASTS_DELETE, "Ctrl+Shift+X"),
    (crate::ID_PODCASTS_REORDER_SOURCES, "Ctrl+Shift+K"),
    (
        crate::ID_PODCASTS_SORT_SOURCES_ALPHABETICALLY,
        "Ctrl+Shift+J",
    ),
    (crate::ID_RADIO_SEARCH, "Ctrl+Shift+F"),
    (crate::ID_RADIO_ADD, "Ctrl+Shift+M"),
    (crate::ID_RADIO_EDIT_FAVORITE, "Ctrl+Shift+H"),
    (crate::ID_RADIO_REORDER_FAVORITES, "Ctrl+Shift+B"),
    (crate::ID_RADIO_DELETE_FAVORITE, "Ctrl+Shift+Y"),
    (crate::ID_RADIO_RECORDINGS, "Ctrl+Shift+V"),
    (crate::ID_READ_ONLY_MODE, "Alt+Shift+O"),
    (crate::ID_BOOK_TOC, "Alt+Shift+J"),
    (crate::ID_TOOLS_AUDIO_DESCRIPTION, "Alt+Shift+A"),
    (crate::ID_TOOLS_MEDIA_TRANSCRIPTION, "Alt+Shift+T"),
    (crate::ID_TOOLS_YOUTUBE, "Alt+Shift+U"),
    (crate::ID_TOOLS_CINEMA, "Alt+Shift+C"),
    (crate::ID_TOOLS_CONVERT_MEDIA, "Alt+Shift+M"),
    (crate::ID_TOOLS_CONVERT_FOLDER, "Alt+Shift+Q"),
    (crate::ID_TOOLS_MEDIA_JOIN, "Alt+Shift+Y"),
    (crate::ID_TOOLS_RECORD_PODCAST, "Alt+Shift+P"),
    (crate::ID_TOOLS_MEDIA_CUTTER, "Alt+Shift+X"),
    (crate::ID_SONARPAD_AUDIO_DESCRIPTIONS, "Alt+Shift+S"),
    (crate::ID_RAI_AUDIO_DESCRIPTIONS, "Alt+Shift+D"),
    (crate::ID_RAIPLAY_BROWSE, "Alt+Shift+V"),
    (crate::ID_LA7_PLAY, "Alt+Shift+L"),
    (crate::ID_RAIPLAY_SOUND, "Alt+Shift+H"),
    (crate::ID_TV, "Alt+Shift+Z"),
    (crate::ID_TOOLS_BATCH_AUDIOBOOKS, "Alt+Shift+B"),
    (crate::ID_TOOLS_WIKIPEDIA, "Alt+Shift+W"),
    (crate::ID_TOOLS_VOICE_DICTIONARY, "Alt+Shift+E"),
    (crate::ID_TOOLS_TRECCANI, "Alt+Shift+N"),
    (crate::ID_TOOLS_BDCIECHI, "Alt+Shift+I"),
    (crate::ID_TOOLS_CALENDAR, "Alt+Shift+K"),
    (crate::ID_TOOLS_WEATHER, "Alt+Shift+F"),
    (crate::ID_TOOLS_ROUTES, "Alt+Shift+R"),
    (crate::ID_TOOLS_ITALIAN_DIRECTORIES, "Alt+Shift+G"),
    (crate::ID_ABOUT, "Ctrl+Alt+Shift+I"),
    (crate::ID_DONATIONS, "Ctrl+Alt+Shift+D"),
    (crate::ID_CHANGELOG, "Ctrl+Alt+Shift+H"),
    (crate::ID_CHECK_UPDATES, "Ctrl+Alt+Shift+U"),
    (crate::ID_SEND_DIAGNOSTICS, "Ctrl+Alt+Shift+R"),
];

pub fn label(id: i32, title: &str) -> String {
    let Some((_, shortcut)) = BINDINGS.iter().find(|(command, _)| *command == id) else {
        return title.to_string();
    };
    let mut caption = title.split('\t').next().unwrap_or(title);
    if id == crate::ID_SETTINGS {
        caption = caption.strip_suffix(" (Cmd+,)").unwrap_or(caption);
    }
    format!("{caption}\t{shortcut}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn fixed_commands_and_accelerators_are_globally_unique() {
        let mut commands = HashSet::new();
        let mut shortcuts = HashSet::new();
        for &(id, shortcut) in BINDINGS {
            assert!(commands.insert(id), "Duplicate menu command: {id}");
            assert!(
                shortcuts.insert(shortcut.to_ascii_lowercase()),
                "Duplicate shortcut: {shortcut}"
            );
        }
    }

    #[test]
    fn reserved_editing_navigation_and_system_shortcuts_stay_free() {
        let reserved = [
            "Ctrl+C",
            "Ctrl+V",
            "Ctrl+X",
            "Ctrl+A",
            "Ctrl+Z",
            "Ctrl+Shift+Z",
            "Ctrl+Left",
            "Ctrl+Right",
            "Ctrl+Shift+3",
            "Ctrl+Shift+4",
            "Ctrl+Shift+5",
            "Ctrl+Space",
            "Ctrl+Alt+Space",
            "Ctrl+Shift+Q",
            "Ctrl+Alt+H",
            "Ctrl+Alt+D",
        ];
        for &(_, shortcut) in BINDINGS {
            assert!(
                !reserved.contains(&shortcut),
                "Reserved shortcut: {shortcut}"
            );
            assert!(
                !shortcut.contains("RawCtrl"),
                "Do not use VoiceOver Control+Option"
            );
        }
    }

    #[test]
    fn labels_keep_translation_and_exactly_one_accelerator() {
        for title in ["Apri…", "Ouvrir…", "Abrir…", "Otwórz…", "Otevřít…", "Open…"] {
            assert_eq!(label(crate::ID_OPEN, title), format!("{title}\tCtrl+O"));
            assert_eq!(
                label(crate::ID_OPEN, &format!("{title}\tOld")),
                format!("{title}\tCtrl+O")
            );
        }
        assert_eq!(
            label(crate::ID_SETTINGS, "Impostazioni (Cmd+,)"),
            "Impostazioni\tCtrl+,"
        );
        assert_eq!(label(-99999, "Titolo dinamico"), "Titolo dinamico");
    }
}
