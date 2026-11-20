//! Typing in a one-line text field, shared by every text field in the TUI.
//!
//! A field's cursor counts characters, not bytes: it is also the column the terminal cursor is
//! drawn at. Edits convert it to a byte offset on a character boundary, so non-ASCII input
//! (`ü`, `日本`) edits like any other text.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The characters Ctrl/Alt+Backspace stops at.
fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '/' | '_' | '-')
}

/// The number of characters in `text`: the cursor position at its end.
pub(crate) fn char_len(text: &str) -> usize {
    text.chars().count()
}

/// The byte offset of character `cursor` in `text` (its length past the end).
fn byte_offset(text: &str, cursor: usize) -> usize {
    text.char_indices().nth(cursor).map_or(text.len(), |(i, _)| i)
}

/// The character position of the previous word boundary (for Ctrl/Alt+Backspace).
fn word_boundary_left(text: &str, cursor: usize) -> usize {
    let chars: Vec<char> = text.chars().take(cursor).collect();
    let Some(mut pos) = chars.len().checked_sub(1) else { return 0 };
    // Skip separators, then the word before them.
    while pos > 0 && is_separator(chars[pos]) {
        pos -= 1;
    }
    while pos > 0 && !is_separator(chars[pos - 1]) {
        pos -= 1;
    }
    pos
}

/// Applies an editing key to `text` with its cursor: typing, Backspace (with Ctrl or Alt, a
/// word), Delete, Left, Right, Home and End. Other keys change nothing.
pub(crate) fn edit_line(text: &mut String, cursor: &mut usize, key: KeyEvent) {
    let len = char_len(text);
    *cursor = (*cursor).min(len);
    match key.code {
        KeyCode::Char(c) => {
            text.insert(byte_offset(text, *cursor), c);
            *cursor += 1;
        }
        KeyCode::Backspace if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) => {
            let start = word_boundary_left(text, *cursor);
            text.drain(byte_offset(text, start)..byte_offset(text, *cursor));
            *cursor = start;
        }
        KeyCode::Backspace if *cursor > 0 => {
            *cursor -= 1;
            text.remove(byte_offset(text, *cursor));
        }
        KeyCode::Delete if *cursor < len => {
            text.remove(byte_offset(text, *cursor));
        }
        KeyCode::Left => *cursor = cursor.saturating_sub(1),
        KeyCode::Right => *cursor = (*cursor + 1).min(len),
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = len,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_str(text: &mut String, cursor: &mut usize, s: &str) {
        for c in s.chars() {
            edit_line(text, cursor, key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn typing_after_a_multibyte_character_inserts_after_it() {
        let (mut text, mut cursor) = (String::new(), 0);
        type_str(&mut text, &mut cursor, "/data/ü日本x");
        assert_eq!((text.as_str(), cursor), ("/data/ü日本x", 10));
        edit_line(&mut text, &mut cursor, key(KeyCode::Left));
        edit_line(&mut text, &mut cursor, key(KeyCode::Left));
        type_str(&mut text, &mut cursor, "é");
        assert_eq!((text.as_str(), cursor), ("/data/ü日é本x", 9));
        edit_line(&mut text, &mut cursor, key(KeyCode::Home));
        type_str(&mut text, &mut cursor, "ö");
        assert_eq!((text.as_str(), cursor), ("ö/data/ü日é本x", 1));
    }

    #[test]
    fn deleting_multibyte_characters_removes_whole_characters() {
        let (mut text, mut cursor) = ("aü日本b".to_string(), 0);
        edit_line(&mut text, &mut cursor, key(KeyCode::End));
        assert_eq!(cursor, 5);
        edit_line(&mut text, &mut cursor, key(KeyCode::Left));
        edit_line(&mut text, &mut cursor, key(KeyCode::Backspace));
        assert_eq!((text.as_str(), cursor), ("aü日b", 3));
        edit_line(&mut text, &mut cursor, key(KeyCode::Left));
        edit_line(&mut text, &mut cursor, key(KeyCode::Left));
        edit_line(&mut text, &mut cursor, key(KeyCode::Delete));
        assert_eq!((text.as_str(), cursor), ("a日b", 1));
        // Right stops at the end, Delete there does nothing.
        for _ in 0..5 {
            edit_line(&mut text, &mut cursor, key(KeyCode::Right));
        }
        edit_line(&mut text, &mut cursor, key(KeyCode::Delete));
        assert_eq!((text.as_str(), cursor), ("a日b", 3));
    }

    #[test]
    fn word_backspace_with_multibyte_words() {
        let (mut text, mut cursor) = ("/home/jürgen/日本_data".to_string(), 0);
        edit_line(&mut text, &mut cursor, key(KeyCode::End));
        let word = KeyEvent::new(KeyCode::Backspace, KeyModifiers::CONTROL);
        edit_line(&mut text, &mut cursor, word);
        assert_eq!((text.as_str(), cursor), ("/home/jürgen/日本_", 16));
        edit_line(&mut text, &mut cursor, word);
        assert_eq!((text.as_str(), cursor), ("/home/jürgen/", 13));
        edit_line(&mut text, &mut cursor, word);
        assert_eq!((text.as_str(), cursor), ("/home/", 6));
    }

    #[test]
    fn word_boundary_counts_characters() {
        assert_eq!(word_boundary_left("hello world", 11), 6);
        assert_eq!(word_boundary_left("hello world", 5), 0);
        assert_eq!(word_boundary_left("a/b/c", 5), 4);
        assert_eq!(word_boundary_left("", 0), 0);
        assert_eq!(word_boundary_left("ü日 本語", 5), 3);
        assert_eq!(word_boundary_left("ü日 本語", 2), 0);
    }

    #[test]
    fn a_cursor_past_the_end_is_clamped() {
        let (mut text, mut cursor) = ("日本".to_string(), 6);
        type_str(&mut text, &mut cursor, "x");
        assert_eq!((text.as_str(), cursor), ("日本x", 3));
    }
}
