//! Document ordering, independent of the GUI so lifecycle rules can be tested locally.
pub struct DocumentDeck<T> {
    pub items: Vec<T>,
    pub active: usize,
}

/// Prefer the produced character: on some layouts '<' and '>' share a key.
pub fn shortcut_direction(key_code: i32, unicode_key: i32, shift: bool) -> Option<bool> {
    match unicode_key {
        // Cocoa/wx may report the unshifted '<' for both keys on Italian
        // layouts. US Shift+comma also produces '<', but still means next.
        60 => Some(shift && key_code == 60),
        62 => Some(true),
        _ => match key_code {
            60 => Some(shift),
            62 => Some(true),
            44 if shift => Some(false),
            46 if shift => Some(true),
            _ => None,
        },
    }
}

impl<T: Default> Default for DocumentDeck<T> {
    fn default() -> Self {
        Self {
            items: vec![T::default()],
            active: 0,
        }
    }
}

impl<T: Default> DocumentDeck<T> {
    pub fn replace_current(&mut self, value: T) {
        self.items[self.active] = value;
    }

    pub fn append(&mut self, value: T) {
        self.items.push(value);
        self.active = self.items.len() - 1;
    }

    pub fn step(&mut self, backwards: bool) {
        let count = self.items.len();
        self.active = if backwards {
            (self.active + count - 1) % count
        } else {
            (self.active + 1) % count
        };
    }

    pub fn remove_current(&mut self) {
        self.items.remove(self.active);
        if self.items.is_empty() {
            self.items.push(T::default());
        }
        self.active = self.active.min(self.items.len() - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_respect_layout_and_leave_comma_for_settings() {
        assert_eq!(shortcut_direction(60, 60, false), Some(false));
        assert_eq!(shortcut_direction(60, 62, true), Some(true));
        assert_eq!(shortcut_direction(60, 60, true), Some(true));
        assert_eq!(shortcut_direction(44, 60, true), Some(false));
        assert_eq!(shortcut_direction(44, 44, false), None);
        assert_eq!(shortcut_direction(60, 0, false), Some(false));
        assert_eq!(shortcut_direction(60, 44, false), Some(false));
        assert_eq!(shortcut_direction(60, 44, true), Some(true));
        assert_eq!(shortcut_direction(44, 44, true), Some(false));
        assert_eq!(shortcut_direction(46, 46, true), Some(true));
        assert_eq!(shortcut_direction(46, 46, false), None);
    }

    #[derive(Default, Debug, PartialEq)]
    struct Document {
        text: String,
        path: String,
        cursor: usize,
        modified: bool,
    }

    #[test]
    fn edits_paths_and_cursor_survive_round_trip() {
        let mut deck = DocumentDeck::<Document>::default();
        deck.replace_current(Document {
            text: "α changed".into(),
            path: "first.txt".into(),
            cursor: 5,
            modified: true,
        });
        deck.append(Document {
            text: "second".into(),
            path: "second.txt".into(),
            ..Default::default()
        });
        deck.step(true);
        assert_eq!(deck.items[deck.active].text, "α changed");
        assert_eq!(deck.items[deck.active].path, "first.txt");
        assert_eq!(deck.items[deck.active].cursor, 5);
        assert!(deck.items[deck.active].modified);
        deck.step(false);
        assert_eq!(deck.items[deck.active].path, "second.txt");
        assert!(!deck.items[deck.active].modified);
    }

    #[test]
    fn navigation_wraps_in_both_directions() {
        let mut deck = DocumentDeck::<String>::default();
        deck.step(true);
        assert_eq!(deck.active, 0);
        deck.append("two".into());
        deck.append("three".into());
        deck.step(false);
        assert_eq!(deck.active, 0);
        deck.step(true);
        assert_eq!(deck.active, 2);
    }

    #[test]
    fn closing_middle_preserves_neighbors_and_last_leaves_blank() {
        let mut deck = DocumentDeck::<String>::default();
        deck.replace_current("one".into());
        deck.append("two".into());
        deck.append("three".into());
        deck.active = 1;
        deck.remove_current();
        assert_eq!(deck.items, ["one", "three"]);
        assert_eq!(deck.items[deck.active], "three");
        deck.remove_current();
        assert_eq!(deck.items[deck.active], "one");
        deck.remove_current();
        assert_eq!(deck.items, [""]);
        assert_eq!(deck.active, 0);
    }
}
