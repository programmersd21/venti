use crossterm::ExecutableCommand;
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use std::io::stdout;

/// a kitty image identified by the id (`i=`) or number (`I=`) it was placed
/// with. tracked per session so the client can delete exactly its own images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KittyImageId {
    Image(u32),
    Number(u32),
}

/// set of kitty image ids forwarded to the host terminal this session.
#[derive(Debug, Default, Clone)]
pub struct KittyImageIds(pub std::collections::BTreeSet<KittyImageId>);

impl KittyImageIds {
    pub fn extend<I: IntoIterator<Item = KittyImageId>>(&mut self, ids: I) {
        self.0.extend(ids);
    }
}

/// raii guard ensuring client terminal state is restored on drop or panic.
pub struct TerminalGuard {
    mouse: bool,
    kitty_keyboard: bool,
    kitty_images: std::sync::Arc<std::sync::Mutex<KittyImageIds>>,
}

impl TerminalGuard {
    /// enters raw mode and alternate screen, optionally enabling mouse capture.
    pub fn enter() -> std::io::Result<Self> {
        Self::enter_with_mouse(true)
    }

    /// enters raw mode and alternate screen with the given mouse capture setting.
    pub fn enter_with_mouse(mouse: bool) -> std::io::Result<Self> {
        enable_raw_mode()?;
        let mut out = stdout();
        out.execute(EnterAlternateScreen)?;
        let kitty_keyboard =
            crate::util::outer_term::detect() == crate::util::outer_term::OuterGraphics::Kitty;
        if kitty_keyboard {
            use std::io::Write;
            if let Err(error) = out.write_all(b"\x1b[>1u").and_then(|()| out.flush()) {
                let _ = out.execute(LeaveAlternateScreen);
                let _ = disable_raw_mode();
                return Err(error);
            }
        }

        if mouse {
            // 1000: button clicks, 1002: button+drag, 1003: any motion, 1006: SGR extended.
            // mode 1003 is required for hover-driven UIs (opencode, lazygit, etc.).
            // kitty respects `pointer_shape_when_grabbed` for the pointer shape, so the
            // user can reclaim the arrow pointer there if they prefer.
            use std::io::Write;
            let _ = out.write_all(b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h");
            let _ = out.flush();
        }
        out.execute(EnableBracketedPaste)?;
        out.execute(EnableFocusChange)?;
        Ok(Self {
            mouse,
            kitty_keyboard,
            kitty_images: std::sync::Arc::new(std::sync::Mutex::new(KittyImageIds::default())),
        })
    }

    /// shared set of kitty image ids the client forwarded to the host. pass
    /// this to the apply state so forwarded placements are recorded.
    pub fn kitty_images(&self) -> std::sync::Arc<std::sync::Mutex<KittyImageIds>> {
        std::sync::Arc::clone(&self.kitty_images)
    }

    /// emits kitty delete commands for every image id forwarded this session.
    /// no-op on terminals without kitty graphics support or when nothing was
    /// forwarded. wrapped in the tmux passthrough envelope when nested.
    fn delete_kitty_images(&self, out: &mut std::io::Stdout) {
        use crate::util::outer_term::OuterGraphics;
        let graphics = crate::util::outer_term::detect();
        let wrap = graphics == OuterGraphics::Tmux;
        if graphics != OuterGraphics::Kitty && graphics != OuterGraphics::Tmux {
            return;
        }
        let ids: Vec<KittyImageId> = match self.kitty_images.lock() {
            Ok(ids) => ids.0.iter().copied().collect(),
            Err(_) => return,
        };
        use std::io::Write;
        for id in ids {
            let inner = match id {
                KittyImageId::Image(id) => {
                    format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\")
                }
                KittyImageId::Number(id) => {
                    format!("\x1b_Ga=d,d=N,I={id},q=2\x1b\\")
                }
            };
            let bytes: Vec<u8> = if wrap {
                crate::client::apply_update::wrap_tmux_passthrough(inner.as_bytes())
            } else {
                inner.into_bytes()
            };
            let _ = out.write_all(&bytes);
        }
        let _ = out.flush();
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        use crossterm::cursor::{SetCursorStyle, Show as CursorShow};
        let mut out = stdout();
        // kitty image placements and image data survive leaving the alternate
        // screen, so delete exactly the images we forwarded before restoring
        // the host terminal. without this they linger as ghosts on the user's
        // shell after venti quits.
        self.delete_kitty_images(&mut out);
        if self.kitty_keyboard {
            use std::io::Write;
            let _ = out.write_all(b"\x1b[<u");
            let _ = out.flush();
        }
        let _ = out.execute(DisableFocusChange);
        let _ = out.execute(DisableBracketedPaste);
        if self.mouse {
            use std::io::Write;
            let _ = out.write_all(b"\x1b[?1000l\x1b[?1002l\x1b[?1006l\x1b[?1003l\x1b[?1015l");
            let _ = out.flush();
        }
        // restore host terminal cursor before leaving the alternate screen so the
        // user doesn't end up staring at a hidden cursor in their shell.
        let _ = out.execute(SetCursorStyle::DefaultUserShape);
        let _ = out.execute(CursorShow);
        let _ = out.execute(LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}
