//! Copying text to the clipboard from the TUI.
//!
//! The TUI usually runs on a remote machine over SSH, where there is no clipboard to talk to. The
//! terminal, though, is on the user's own machine: the OSC 52 escape sequence asks it to put text
//! on its clipboard, and it passes through SSH unchanged. Most current terminals honour it
//! (iTerm2, kitty, WezTerm, Alacritty, foot, Windows Terminal, xterm with `allowWindowOps`);
//! inside tmux it needs `set -g set-clipboard on`, or `allow-passthrough on` for the wrapped copy.

use std::io::{self, Write};

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(B64[(n >> shift) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The OSC 52 sequence that sets the clipboard to `text`. Inside tmux the same sequence follows,
/// wrapped for tmux's passthrough, so it gets through whichever of the two tmux allows.
pub fn osc52(text: &str, tmux: bool) -> String {
    let seq = format!("\x1b]52;c;{}\x07", base64(text.as_bytes()));
    if tmux {
        format!("{seq}\x1bPtmux;{}\x1b\\", seq.replace('\x1b', "\x1b\x1b"))
    } else {
        seq
    }
}

/// Ask the terminal behind `out` to put `text` on the clipboard.
pub fn copy(out: &mut impl Write, text: &str) -> io::Result<()> {
    out.write_all(osc52(text, std::env::var_os("TMUX").is_some()).as_bytes())?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_rfc_4648_vectors() {
        for (plain, enc) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="),
                             ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64(plain.as_bytes()), enc, "{plain:?}");
        }
        // UTF-8 is encoded byte for byte.
        assert_eq!(base64("χ".as_bytes()), "z4c=");
    }

    #[test]
    fn the_sequence_carries_the_text_for_the_clipboard() {
        assert_eq!(osc52("foobar", false), "\x1b]52;c;Zm9vYmFy\x07");
    }

    #[test]
    fn inside_tmux_the_sequence_is_also_sent_through_passthrough() {
        assert_eq!(osc52("foobar", true), "\x1b]52;c;Zm9vYmFy\x07\x1bPtmux;\x1b\x1b]52;c;Zm9vYmFy\x07\x1b\\");
    }

    #[test]
    fn copy_writes_the_sequence() {
        let mut out = Vec::new();
        copy(&mut out, "foobar").unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.starts_with("\x1b]52;c;Zm9vYmFy\x07"), "{out:?}");
    }
}
