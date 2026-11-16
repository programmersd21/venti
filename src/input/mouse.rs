use crate::proto::{MouseActionKind, MouseButtonKind, MouseEventMsg};

/// encodes a mouse event as an sgr (xterm 1006) escape sequence for forwarding
/// to applications that enabled mouse reporting.
pub fn encode_sgr_mouse(msg: &MouseEventMsg) -> Vec<u8> {
    let mut btn: u16 = match msg.button {
        MouseButtonKind::Left => 0,
        MouseButtonKind::Middle => 1,
        MouseButtonKind::Right => 2,
        MouseButtonKind::ScrollUp => 64,
        MouseButtonKind::ScrollDown => 65,
    };

    let is_release = msg.action == MouseActionKind::Up;
    match msg.action {
        MouseActionKind::Drag => btn += 32,
        MouseActionKind::Moved => btn = 32 + 3,
        _ => {}
    }
    if msg.shift {
        btn += 4;
    }
    if msg.alt {
        btn += 8;
    }
    if msg.ctrl {
        btn += 16;
    }

    let final_byte = if is_release { 'm' } else { 'M' };
    format!(
        "\x1b[<{};{};{}{}",
        btn,
        msg.col + 1,
        msg.row + 1,
        final_byte
    )
    .into_bytes()
}
