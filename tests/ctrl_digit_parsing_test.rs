use venti::input::{KeyCode, KeyParser, ParsedInput};

#[test]
fn ctrl_1_through_9_raw_bytes() {
    let mut parser = KeyParser::new();
    // terminals send 0x01..0x08 for ctrl+1..ctrl+8 (indistinguishable from
    // ctrl+a..ctrl+h); 0x09 is the tab key itself. this is why digits are
    // deliberately unbound in the profile.
    for (byte, expect) in (0x01u8..=0x08).zip('a'..='h') {
        let events = parser.parse_bytes(&[byte]);
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedInput::Key(k) => {
                assert_eq!(k.code, KeyCode::Char(expect));
                assert!(k.modifiers.ctrl);
                assert!(!k.modifiers.shift);
            }
            other => panic!("expected key, got {other:?}"),
        }
    }
}

#[test]
fn plain_digit_1_through_9() {
    let mut parser = KeyParser::new();
    for digit in '1'..='9' {
        let events = parser.parse_bytes(digit.to_string().as_bytes());
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedInput::Key(k) => {
                assert_eq!(k.code, KeyCode::Char(digit));
                assert!(!k.modifiers.ctrl);
                assert!(!k.modifiers.alt);
                assert!(!k.modifiers.shift);
            }
            other => panic!("expected key, got {other:?}"),
        }
    }
}

#[test]
fn alt_shift_digit_parsing() {
    let mut parser = KeyParser::new();
    // esc + ! is how a terminal reports alt+shift+1.
    let events = parser.parse_bytes(b"\x1b!");
    assert_eq!(events.len(), 1);
    match &events[0] {
        ParsedInput::Key(k) => {
            assert_eq!(k.code, KeyCode::Char('1'));
            assert!(k.modifiers.alt);
            assert!(k.modifiers.shift);
            assert!(!k.modifiers.ctrl);
        }
        other => panic!("expected key, got {other:?}"),
    }
}

#[test]
fn ctrl_shift_digits_via_kitty_protocol() {
    let mut parser = KeyParser::new();
    // kitty keyboard protocol: csi 49;6u → ctrl+shift+1 (6 = 1+ctrl+shift).
    let events = parser.parse_bytes(b"\x1b[49;6u");
    assert_eq!(events.len(), 1);
    match &events[0] {
        ParsedInput::Key(k) => {
            assert_eq!(k.code, KeyCode::Char('1'));
            assert!(k.modifiers.ctrl);
            assert!(k.modifiers.shift);
        }
        other => panic!("expected key, got {other:?}"),
    }
}
