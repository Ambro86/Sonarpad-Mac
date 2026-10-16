//! Pure shortcut mapping, shared by the native controls and command area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutterKey {
    Space,
    Left,
    Right,
    Up,
    Down,
    Enter,
    DeleteKey,
    Help,
    Letter(char),
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutterAction {
    Play,
    Seek(i32, Option<u32>),
    Select(i32),
    Start,
    End,
    Apply,
    Listen,
    Split,
    Delete,
    Restore,
    Save,
    Help,
}

pub fn cutter_shortcut(
    key: CutterKey,
    area: bool,
    option: bool,
    command: bool,
    control: bool,
    shift: bool,
    advanced: bool,
) -> Option<CutterAction> {
    use CutterAction::*;
    use CutterKey as K;
    // In particular, never consume VoiceOver's Control+Option commands.
    if control {
        return None;
    }
    if command && !option && !shift && key == K::Letter('S') {
        return Some(Save);
    }
    if area && !option && matches!(key, K::Left | K::Right) {
        let step = match (command, shift) {
            (true, false) => Some(10),
            (false, true) => Some(5),
            (false, false) => None,
            _ => return None,
        };
        return Some(Seek(if key == K::Left { -1 } else { 1 }, step));
    }
    if command || shift || (!area && !option) {
        return None;
    }
    match key {
        K::Space => Some(Play),
        K::Left => Some(Seek(-1, None)),
        K::Right => Some(Seek(1, None)),
        K::Letter('3') => Some(Listen),
        K::Enter if area => Some(Listen),
        K::Help if area => Some(Help),
        K::Letter('1') if !advanced => Some(Start),
        K::Letter('2') if !advanced => Some(End),
        K::Letter('4') if !advanced => Some(Apply),
        K::Letter('S') if advanced => Some(Split),
        K::Letter('D') if advanced => Some(Delete),
        K::DeleteKey if area && advanced && !option => Some(Delete),
        K::Letter('R') if advanced => Some(Restore),
        K::Up if advanced => Some(Select(-1)),
        K::Down if advanced => Some(Select(1)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delete_key_is_only_active_in_the_advanced_command_area() {
        assert_eq!(
            cutter_shortcut(CutterKey::DeleteKey, true, false, false, false, false, true),
            Some(CutterAction::Delete)
        );
        assert_eq!(
            cutter_shortcut(
                CutterKey::DeleteKey,
                false,
                false,
                false,
                false,
                false,
                true
            ),
            None
        );
        assert_eq!(
            cutter_shortcut(
                CutterKey::DeleteKey,
                true,
                false,
                false,
                false,
                false,
                false
            ),
            None
        );
        assert_eq!(
            cutter_shortcut(CutterKey::DeleteKey, true, true, false, false, false, true),
            None
        );
    }
    #[test]
    fn voiceover_and_system_commands_are_preserved() {
        for key in [
            CutterKey::Space,
            CutterKey::Left,
            CutterKey::Right,
            CutterKey::Up,
            CutterKey::Down,
            CutterKey::Letter('S'),
            CutterKey::Enter,
        ] {
            assert_eq!(
                cutter_shortcut(key, true, true, false, true, false, true),
                None
            );
        }
        assert_eq!(
            cutter_shortcut(CutterKey::Space, true, false, true, false, false, true),
            None
        ); // Spotlight
        assert_eq!(
            cutter_shortcut(CutterKey::Enter, false, false, false, false, false, true),
            None
        );
        assert_eq!(
            cutter_shortcut(
                CutterKey::Letter('S'),
                false,
                false,
                false,
                false,
                false,
                true
            ),
            None
        ); // Choice search
    }
    #[test]
    fn commands_match_buttons_and_modes() {
        for (key, action) in [
            (CutterKey::Space, CutterAction::Play),
            (CutterKey::Letter('3'), CutterAction::Listen),
            (CutterKey::Letter('D'), CutterAction::Delete),
            (CutterKey::Down, CutterAction::Select(1)),
        ] {
            assert_eq!(
                cutter_shortcut(key, true, false, false, false, false, true),
                Some(action)
            );
            assert_eq!(
                cutter_shortcut(key, false, true, false, false, false, true),
                Some(action)
            );
        }
        assert_eq!(
            cutter_shortcut(
                CutterKey::Letter('D'),
                true,
                false,
                false,
                false,
                false,
                false
            ),
            None
        );
        assert_eq!(
            cutter_shortcut(
                CutterKey::Letter('1'),
                true,
                false,
                false,
                false,
                false,
                false
            ),
            Some(CutterAction::Start)
        );
        assert_eq!(
            cutter_shortcut(CutterKey::Enter, true, false, false, false, false, true),
            Some(CutterAction::Listen)
        );
        assert_eq!(
            cutter_shortcut(CutterKey::Right, true, false, false, false, true, true),
            Some(CutterAction::Seek(1, Some(5)))
        );
        assert_eq!(
            cutter_shortcut(CutterKey::Left, true, false, true, false, false, true),
            Some(CutterAction::Seek(-1, Some(10)))
        );
    }
}
