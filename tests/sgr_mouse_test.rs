use venti::input::mouse::encode_sgr_mouse;
use venti::proto::{MouseActionKind, MouseButtonKind, MouseEventMsg};

fn msg(button: MouseButtonKind, action: MouseActionKind) -> MouseEventMsg {
    MouseEventMsg {
        col: 9,
        row: 4,
        button,
        action,
        shift: false,
        alt: false,
        ctrl: false,
    }
}

#[test]
fn sgr_press_and_release() {
    let press = encode_sgr_mouse(&msg(MouseButtonKind::Left, MouseActionKind::Down));
    assert_eq!(press, b"\x1b[<0;10;5M".to_vec());

    let release = encode_sgr_mouse(&msg(MouseButtonKind::Left, MouseActionKind::Up));
    assert_eq!(release, b"\x1b[<0;10;5m".to_vec());

    let right = encode_sgr_mouse(&msg(MouseButtonKind::Right, MouseActionKind::Down));
    assert_eq!(right, b"\x1b[<2;10;5M".to_vec());

    let middle = encode_sgr_mouse(&msg(MouseButtonKind::Middle, MouseActionKind::Down));
    assert_eq!(middle, b"\x1b[<1;10;5M".to_vec());
}

#[test]
fn sgr_motion_and_wheel() {
    let drag = encode_sgr_mouse(&msg(MouseButtonKind::Left, MouseActionKind::Drag));
    assert_eq!(drag, b"\x1b[<32;10;5M".to_vec());

    let moved = encode_sgr_mouse(&msg(MouseButtonKind::Left, MouseActionKind::Moved));
    assert_eq!(moved, b"\x1b[<35;10;5M".to_vec());

    let up = encode_sgr_mouse(&msg(MouseButtonKind::ScrollUp, MouseActionKind::Down));
    assert_eq!(up, b"\x1b[<64;10;5M".to_vec());

    let down = encode_sgr_mouse(&msg(MouseButtonKind::ScrollDown, MouseActionKind::Down));
    assert_eq!(down, b"\x1b[<65;10;5M".to_vec());
}

#[test]
fn sgr_modifier_bits() {
    let mut m = msg(MouseButtonKind::Left, MouseActionKind::Down);
    m.shift = true;
    m.alt = true;
    m.ctrl = true;
    let encoded = encode_sgr_mouse(&m);
    assert_eq!(encoded, b"\x1b[<28;10;5M".to_vec());

    let mut m = msg(MouseButtonKind::Left, MouseActionKind::Up);
    m.shift = true;
    let encoded = encode_sgr_mouse(&m);
    assert_eq!(encoded, b"\x1b[<4;10;5m".to_vec());
}
