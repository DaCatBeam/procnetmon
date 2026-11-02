use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::Action;

pub struct Binding {
    pub keys: &'static [KeyCode],
    pub action: Action,
    pub help: &'static str,
}

pub const BINDINGS: &[Binding] = &[
    Binding {
        keys: &[KeyCode::Char('q'), KeyCode::Esc],
        action: Action::Quit,
        help: "quit",
    },
    Binding {
        keys: &[KeyCode::Down, KeyCode::Char('j')],
        action: Action::SelectNext,
        help: "next",
    },
    Binding {
        keys: &[KeyCode::Up, KeyCode::Char('k')],
        action: Action::SelectPrev,
        help: "previous",
    },
    Binding {
        keys: &[KeyCode::Right, KeyCode::Tab],
        action: Action::FocusNext,
        help: "next pane",
    },
    Binding {
        keys: &[KeyCode::Left, KeyCode::BackTab],
        action: Action::FocusPrev,
        help: "previous pane",
    },
];

#[must_use]
pub fn action_for(key: KeyEvent) -> Option<Action> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(Action::Quit);
    }

    BINDINGS
        .iter()
        .find(|b| b.keys.contains(&key.code))
        .map(|b| b.action)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use rstest::rstest;

    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[rstest]
    #[case::q(KeyCode::Char('q'), Action::Quit)]
    #[case::esc(KeyCode::Esc, Action::Quit)]
    #[case::down(KeyCode::Down, Action::SelectNext)]
    #[case::j(KeyCode::Char('j'), Action::SelectNext)]
    #[case::up(KeyCode::Up, Action::SelectPrev)]
    #[case::k(KeyCode::Char('k'), Action::SelectPrev)]
    #[case::right(KeyCode::Right, Action::FocusNext)]
    #[case::tab(KeyCode::Tab, Action::FocusNext)]
    #[case::left(KeyCode::Left, Action::FocusPrev)]
    #[case::back_tab(KeyCode::BackTab, Action::FocusPrev)]
    fn bound_keys_do_their_action(#[case] code: KeyCode, #[case] expected: Action) {
        assert_eq!(action_for(press(code)), Some(expected));
    }

    #[rstest]
    fn ctrl_c_quits() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(action_for(ctrl_c), Some(Action::Quit));
    }

    #[rstest]
    #[case::plain_c(press(KeyCode::Char('c')))]
    #[case::ctrl_x(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL))]
    #[case::unbound(press(KeyCode::Enter))]
    fn other_keys_do_nothing(#[case] key: KeyEvent) {
        assert_eq!(action_for(key), None);
    }

    #[rstest]
    fn no_key_is_bound_twice() {
        let keys: Vec<_> = BINDINGS.iter().flat_map(|binding| binding.keys).collect();
        let distinct: HashSet<_> = keys.iter().collect();
        assert_eq!(distinct.len(), keys.len(), "{keys:?}");
    }
}
