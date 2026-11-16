/// regression: alt+shift chords must survive the escape-prefix parser and
/// match bindings in every encoding terminals use (legacy ESC + glyph, and
/// kitty/xterm csi 1;<mod>A..).
use venti::config::schema::ProfileMode;
use venti::input::{Command, InputMode, Key, KeyBindings, KeyParser, ParsedInput};

fn key_from(events: Vec<ParsedInput>) -> Key {
    events
        .into_iter()
        .find_map(|e| match e {
            ParsedInput::Key(k) => Some(k),
            _ => None,
        })
        .expect("no key parsed")
}

#[test]
fn esc_letter_alt_shift_bindings_match() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);
    let mut p = KeyParser::new();

    let cases: &[(&[u8], Command)] = &[
        (b"\x1bH", Command::SwapLeft),
        (b"\x1bJ", Command::SwapDown),
        (b"\x1bK", Command::SwapUp),
        (b"\x1bL", Command::SwapRight),
        (b"\x1b!", Command::MoveToWorkspace(vec![1])),
        (b"\x1b@", Command::MoveToWorkspace(vec![2])),
        (b"\x1b#", Command::MoveToWorkspace(vec![3])),
        (b"\x1b$", Command::MoveToWorkspace(vec![4])),
        (b"\x1b%", Command::MoveToWorkspace(vec![5])),
        (b"\x1b^", Command::MoveToWorkspace(vec![6])),
        (b"\x1b&", Command::MoveToWorkspace(vec![7])),
        (b"\x1b*", Command::MoveToWorkspace(vec![8])),
        (b"\x1b(", Command::MoveToWorkspace(vec![9])),
        (b"\x1b[1;4D", Command::MoveToWorkspacePrev),
        (b"\x1b[1;4C", Command::MoveToWorkspaceNext),
        (b"\x1bQ", Command::KillSession),
    ];

    for (bytes, expected) in cases.iter() {
        let key = key_from(p.parse_bytes(bytes));
        assert_eq!(
            bindings.find(InputMode::Normal, &key),
            Some(expected),
            "bytes {bytes:?} parsed to {key:?} should map to {expected:?}"
        );
    }
}

#[test]
fn csi_arrows_alt_shift_move_workspace() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);
    let mut p = KeyParser::new();

    for (bytes, expected) in [
        (&b"\x1b[1;4C"[..], Command::MoveToWorkspaceNext),
        (&b"\x1b[1;4D"[..], Command::MoveToWorkspacePrev),
    ] {
        let key = key_from(p.parse_bytes(bytes));
        assert_eq!(bindings.find(InputMode::Normal, &key), Some(&expected));
    }
}

#[test]
fn kitty_protocol_alt_shift_digits() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);
    let mut p = KeyParser::new();

    // kitty sends csi 49;4u for alt+shift+1 (codepoint '1', mod 4 = alt+shift)
    for (bytes, expected) in [
        (&b"\x1b[49;4u"[..], Command::MoveToWorkspace(vec![1])),
        (&b"\x1b[50;4u"[..], Command::MoveToWorkspace(vec![2])),
        (&b"\x1b[57;4u"[..], Command::MoveToWorkspace(vec![9])),
        (&b"\x1b[72;4u"[..], Command::SwapLeft),
        (&b"\x1b[76;4u"[..], Command::SwapRight),
    ] {
        let key = key_from(p.parse_bytes(bytes));
        assert_eq!(bindings.find(InputMode::Normal, &key), Some(&expected));
    }
}
