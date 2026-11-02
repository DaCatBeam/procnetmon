//! The footer's key legend. It's built from the keymap, so it can't drift from the real bindings.

use std::borrow::Cow;

use crossterm::event::KeyCode;
use ratatui::{
    style::Stylize,
    text::{Line, Span},
};

use crate::keymap::BINDINGS;

/// The keymap's bindings as one line, like " q/Esc quit  ↓/j next", in keymap order.
///
/// Bindings that don't fit in `width` columns are left off whole rather than cut mid-word.
#[must_use]
pub fn legend(width: u16) -> Line<'static> {
    let mut spans = Vec::new();
    let mut used = 0;
    for binding in BINDINGS {
        let keys: Vec<_> = binding.keys.iter().map(|&code| label(code)).collect();
        let gap = if spans.is_empty() { " " } else { "  " };
        let entry = [
            Span::raw(gap),
            Span::from(keys.join("/")).bold(),
            Span::from(format!(" {}", binding.help)).dark_gray(),
        ];
        let entry_width: usize = entry.iter().map(Span::width).sum();
        if used + entry_width > usize::from(width) {
            break;
        }
        used += entry_width;
        spans.extend(entry);
    }
    Line::from(spans)
}

/// A key's short name: arrows as glyphs, letters as themselves, and named keys like Esc by name.
fn label(code: KeyCode) -> Cow<'static, str> {
    match code {
        KeyCode::Up => "↑".into(),
        KeyCode::Down => "↓".into(),
        KeyCode::Left => "←".into(),
        KeyCode::Right => "→".into(),
        KeyCode::BackTab => "Shift+Tab".into(),
        KeyCode::Char(c) => c.to_string().into(),
        KeyCode::F(n) => format!("F{n}").into(),
        // The rest, like Esc, Tab, Enter, and Home, read fine as their variant names.
        other => format!("{other:?}").into(),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn text(line: &Line) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[rstest]
    fn shows_every_binding_when_there_is_room() {
        let shown = text(&legend(200));
        for binding in BINDINGS {
            assert!(
                shown.contains(binding.help),
                "{shown:?} lacks {}",
                binding.help
            );
            for &code in binding.keys {
                assert!(shown.contains(&*label(code)), "{shown:?} lacks {code:?}");
            }
        }
    }

    #[rstest]
    fn leaves_off_whole_bindings_that_do_not_fit() {
        let full = text(&legend(200));
        for width in 0..200 {
            let line = legend(width);
            let shown = text(&line);
            assert!(line.width() <= usize::from(width), "{width}: {shown:?}");
            // What's shown is the full legend up to the end of some binding.
            let rest = full
                .strip_prefix(&shown)
                .expect("a prefix of the full legend");
            assert!(
                shown.is_empty() || rest.is_empty() || rest.starts_with("  "),
                "{width}: {shown:?}"
            );
        }
    }

    #[rstest]
    fn a_binding_that_exactly_fits_is_shown() {
        let full = legend(200);
        let width = u16::try_from(full.width()).expect("the legend is narrower than u16::MAX");
        assert_eq!(legend(width), full);
        assert_ne!(legend(width - 1), full);
    }

    #[rstest]
    #[case::up(KeyCode::Up, "↑")]
    #[case::down(KeyCode::Down, "↓")]
    #[case::left(KeyCode::Left, "←")]
    #[case::right(KeyCode::Right, "→")]
    #[case::back_tab(KeyCode::BackTab, "Shift+Tab")]
    #[case::letter(KeyCode::Char('q'), "q")]
    #[case::function_key(KeyCode::F(5), "F5")]
    #[case::esc(KeyCode::Esc, "Esc")]
    #[case::tab(KeyCode::Tab, "Tab")]
    fn keys_have_short_names(#[case] code: KeyCode, #[case] expected: &str) {
        assert_eq!(label(code), expected);
    }
}
