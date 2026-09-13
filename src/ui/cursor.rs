//! One cursor (17_4_UI.md step 4 / F1). Every list modal, the editors,
//! the startup menu and the list tabs move a `usize` the same way: Up
//! stops at the top, Down stops at the last row, and a list that has
//! shrunk pulls the cursor back inside. The arms name their own keys
//! and call these.

/// One row up, stopping at the top.
pub fn cursor_up(index: &mut usize) {
    *index = index.saturating_sub(1);
}

/// One row down, stopping at the last of `len` rows.
pub fn cursor_down(index: &mut usize, len: usize) {
    if *index + 1 < len {
        *index += 1;
    }
}

/// Pull a cursor back inside a list of `len` rows (to 0 when empty);
/// a cursor already inside does not move.
pub fn clamp_cursor(index: &mut usize, len: usize) {
    if *index >= len {
        *index = len.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_and_down_stop_at_the_ends() {
        let mut i = 0;
        cursor_up(&mut i);
        assert_eq!(i, 0);
        cursor_down(&mut i, 3);
        cursor_down(&mut i, 3);
        cursor_down(&mut i, 3);
        assert_eq!(i, 2);
        cursor_down(&mut i, 0);
        assert_eq!(i, 2, "an empty list moves nothing");
    }

    #[test]
    fn clamp_only_moves_a_cursor_past_the_end() {
        let mut i = 1;
        clamp_cursor(&mut i, 3);
        assert_eq!(i, 1);
        clamp_cursor(&mut i, 1);
        assert_eq!(i, 0);
        clamp_cursor(&mut i, 0);
        assert_eq!(i, 0);
    }
}
