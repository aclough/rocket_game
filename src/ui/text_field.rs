//! One text field (17_4_UI.md step 3 / F3). Every "type a value"
//! sub-modal — engine and reactor names and scales, the rocket name,
//! contract and campaign bids, the designer's payload, the startup
//! company name — answers the same four keys the same way. The arms call
//! `edit_text_field` and act on its verdict instead of each spelling out
//! Esc / Enter / Backspace / Char.

use ratatui::crossterm::event::KeyCode;

/// What a field accepts: any character, or digits and one decimal
/// point's worth of characters (the parse decides whether "1.2.3" is a
/// number; the filter only keeps the buffer free of letters).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Number,
}

impl FieldKind {
    fn accepts(self, c: char) -> bool {
        match self {
            FieldKind::Text => true,
            FieldKind::Number => c.is_ascii_digit() || c == '.',
        }
    }
}

/// What the caller does next: keep showing the field, act on its
/// contents, or drop them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldEdit {
    Continue,
    Commit,
    Cancel,
}

/// Apply one key to a field's buffer. Esc cancels, Enter commits,
/// Backspace pops, a character the kind accepts is appended, and every
/// other key leaves the buffer as it was.
pub fn edit_text_field(key: KeyCode, buffer: &mut String, kind: FieldKind) -> FieldEdit {
    match key {
        KeyCode::Esc => FieldEdit::Cancel,
        KeyCode::Enter => FieldEdit::Commit,
        KeyCode::Backspace => {
            buffer.pop();
            FieldEdit::Continue
        }
        KeyCode::Char(c) if kind.accepts(c) => {
            buffer.push(c);
            FieldEdit::Continue
        }
        _ => FieldEdit::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_into(kind: FieldKind, keys: &[KeyCode]) -> (String, FieldEdit) {
        let mut buffer = String::new();
        let mut last = FieldEdit::Continue;
        for &k in keys {
            last = edit_text_field(k, &mut buffer, kind);
        }
        (buffer, last)
    }

    #[test]
    fn a_number_field_keeps_digits_and_points_and_drops_letters() {
        let keys: Vec<KeyCode> = "1a.5x".chars().map(KeyCode::Char).collect();
        let (buffer, verdict) = type_into(FieldKind::Number, &keys);
        assert_eq!(buffer, "1.5");
        assert_eq!(verdict, FieldEdit::Continue);
    }

    #[test]
    fn a_text_field_keeps_everything_and_backspace_pops() {
        let mut keys: Vec<KeyCode> = "Ab 1".chars().map(KeyCode::Char).collect();
        keys.push(KeyCode::Backspace);
        let (buffer, _) = type_into(FieldKind::Text, &keys);
        assert_eq!(buffer, "Ab ");
    }

    #[test]
    fn enter_commits_esc_cancels_and_neither_touches_the_buffer() {
        let mut buffer = String::from("42");
        assert_eq!(edit_text_field(KeyCode::Enter, &mut buffer, FieldKind::Number), FieldEdit::Commit);
        assert_eq!(edit_text_field(KeyCode::Esc, &mut buffer, FieldKind::Number), FieldEdit::Cancel);
        assert_eq!(edit_text_field(KeyCode::Up, &mut buffer, FieldKind::Number), FieldEdit::Continue);
        assert_eq!(buffer, "42");
    }
}
