use venti::input::{KeyCode, KeyParser, ParsedInput};

#[test]
fn kitty_ctrl_1_through_9() {
    for i in 1..=9 {
        let mut parser = KeyParser::new();
        let codepoint = 48 + i; // 49-57 are '1'-'9'
        let seq = format!("\x1b[{};5u", codepoint);
        let events = parser.parse_bytes(seq.as_bytes());

        assert_eq!(events.len(), 1);
        if let ParsedInput::Key(k) = &events[0] {
            let expected_char = char::from_digit(i as u32, 10).unwrap();
            assert_eq!(
                k.code,
                KeyCode::Char(expected_char),
                "Should be digit {}",
                i
            );
            assert!(k.modifiers.ctrl, "Should have ctrl modifier");
            assert!(!k.modifiers.alt, "Should not have alt");
            assert!(!k.modifiers.shift, "Should not have shift");
        } else {
            panic!("Expected Key event");
        }
    }
}
