use super::commands::Command;
use super::keys::{Key, KeyCode, Modifiers, parse_key};
use super::modes::InputMode;
use crate::config::schema::ProfileMode;
use std::collections::HashMap;
use std::str::FromStr;

/// the shift+tab key, used by every profile to fold the sidebar away.
fn shift_tab_key() -> Key {
    Key::new(
        KeyCode::Tab,
        Modifiers {
            ctrl: false,
            alt: false,
            shift: true,
            super_key: false,
        },
    )
}

/// collection of active keybindings across all modes.
#[derive(Debug, Clone)]
pub struct KeyBindings {
    bindings: HashMap<(InputMode, Key), Command>,
    passthrough: Vec<Key>,
    disabled: Vec<Key>,
}

impl KeyBindings {
    /// constructs default keybindings for the given profile and passthrough list.
    pub fn new(profile: ProfileMode, passthrough_strs: &[String]) -> Self {
        let mut bindings = HashMap::new();
        let mut passthrough = Vec::new();

        for pt in passthrough_strs {
            if let Ok(k) = parse_key(pt) {
                passthrough.push(k);
            }
        }

        match profile {
            ProfileMode::Beginner => {
                Self::populate_beginner(&mut bindings);
            }
            ProfileMode::Hypr => {
                Self::populate_hypr(&mut bindings);
            }
            ProfileMode::Vim => {
                Self::populate_vim(&mut bindings);
            }
            ProfileMode::Dev => {
                Self::populate_dev(&mut bindings);
            }
        }

        Self {
            bindings,
            passthrough,
            disabled: Vec::new(),
        }
    }

    /// sets disabled keys (these will be passed through instead of handled)
    pub fn set_disabled(&mut self, disabled_strs: &[String]) {
        self.disabled.clear();
        for ds in disabled_strs {
            if let Ok(k) = parse_key(ds) {
                self.disabled.push(k);
            }
        }
    }

    /// looks up a command for the given mode and key press.
    ///
    /// f keys always drive venti. alt keys drive venti too; applications
    /// receive alt only for combos listed in `input.passthrough`.
    ///
    /// fallback: if an alt+shift combination does not match, retry with just alt.
    /// this compensates for host terminals that strip shift from alt-modified keys.
    pub fn find(&self, mode: InputMode, key: &Key) -> Option<&Command> {
        if self.disabled.contains(key) {
            return None;
        }
        if mode == InputMode::Normal && self.passthrough.contains(key) {
            return None;
        }
        
        // try exact match first
        if let Some(cmd) = self.bindings.get(&(mode, key.clone())) {
            return Some(cmd);
        }
        
        // fallback: if alt+shift, try alt without shift
        if key.modifiers.alt && key.modifiers.shift && !key.modifiers.ctrl && !key.modifiers.super_key {
            let fallback_key = Key::new(
                key.code.clone(),
                Modifiers {
                    ctrl: false,
                    alt: true,
                    shift: false,
                    super_key: false,
                },
            );
            if let Some(cmd) = self.bindings.get(&(mode, fallback_key)) {
                return Some(cmd);
            }
        }
        
        None
    }

    /// iterates over all bindings in the given mode.
    /// used by the help renderer so the cheat-sheet always matches the map.
    pub fn entries(&self, mode: InputMode) -> Vec<(&Key, &Command)> {
        self.bindings
            .iter()
            .filter(|((m, _), _)| *m == mode)
            .map(|((_, k), c)| (k, c))
            .collect()
    }

    /// adds or overrides a binding.
    pub fn bind(&mut self, mode: InputMode, key: Key, cmd: Command) {
        self.bindings.insert((mode, key), cmd);
    }

    /// applies custom user bindings from configuration.
    pub fn apply_custom_binds(&mut self, binds: &HashMap<String, String>) -> Result<(), String> {
        for (key_str, cmd_str) in binds {
            let (mode, raw_key) = (InputMode::Normal, key_str.as_str());

            let key = parse_key(raw_key)
                .map_err(|e| format!("invalid key in binds \"{key_str}\": {e}"))?;
            let cmd = Command::from_str(cmd_str)
                .map_err(|e| format!("invalid command in binds \"{cmd_str}\": {e}"))?;
            self.bind(mode, key, cmd);
        }
        Ok(())
    }

    fn populate_shared(m: &mut HashMap<(InputMode, Key), Command>) {
        Self::populate_fkeys(m);
        // shift+tab folds the workspace sidebar away; every profile gets it so
        // the sidebar is reachable without a chord table.
        m.insert((InputMode::Normal, shift_tab_key()), Command::ToggleSidebar);

        let alt_shift = Modifiers {
            ctrl: false,
            alt: true,
            shift: true,
            super_key: false,
        };

        for (key, cmd) in [
            (Key::alt(KeyCode::Char('h')), Command::FocusLeft),
            (Key::alt(KeyCode::Char('j')), Command::FocusDown),
            (Key::alt(KeyCode::Char('k')), Command::FocusUp),
            (Key::alt(KeyCode::Char('l')), Command::FocusRight),
            // note: alt+shift+arrows moves the focused pane between workspaces.
            // resize stays in f10 resize mode and via mouse border-drag.
            (Key::new(KeyCode::Char('h'), alt_shift), Command::SwapLeft),
            // j/k follow vim: j swaps down, k swaps up
            (Key::new(KeyCode::Char('j'), alt_shift), Command::SwapDown),
            (Key::new(KeyCode::Char('k'), alt_shift), Command::SwapUp),
            (Key::new(KeyCode::Char('l'), alt_shift), Command::SwapRight),
            (Key::alt(KeyCode::Char('q')), Command::ClosePane),
            (
                Key::new(KeyCode::Char('q'), alt_shift),
                Command::KillSession,
            ),
            (Key::alt(KeyCode::Char('n')), Command::NewWorkspace),
            (Key::alt(KeyCode::Right), Command::WorkspaceNext),
            (Key::alt(KeyCode::Left), Command::WorkspacePrev),
            (Key::alt(KeyCode::Char(',')), Command::WorkspacePrev),
            (Key::alt(KeyCode::Char('.')), Command::WorkspaceNext),
            (
                Key::new(KeyCode::Char(','), alt_shift),
                Command::MoveToWorkspacePrev,
            ),
            (
                Key::new(KeyCode::Char('.'), alt_shift),
                Command::MoveToWorkspaceNext,
            ),
            (
                Key::alt(KeyCode::Char('w')),
                Command::CloseWorkspace(Vec::new()),
            ),
            (Key::alt(KeyCode::Char('t')), Command::ThemeMenu),
            (Key::alt(KeyCode::Char('s')), Command::ToggleSidebar),
            (Key::alt(KeyCode::Space), Command::CommandPalette),
            (Key::alt(KeyCode::Char('p')), Command::RotateSplit),
        ] {
            m.insert((InputMode::Normal, key), cmd);
        }

        Self::populate_digits(m);

        // alt+shift+left/right move the focused pane to the previous/next workspace
        m.insert(
            (InputMode::Normal, Key::new(KeyCode::Left, alt_shift)),
            Command::MoveToWorkspacePrev,
        );
        m.insert(
            (InputMode::Normal, Key::new(KeyCode::Right, alt_shift)),
            Command::MoveToWorkspaceNext,
        );
    }

    /// alt+1..9 reliably encode across every terminal (esc+digit sequence), so
    /// the digit row is the goto namespace in every profile. ctrl+digit cannot
    /// be relied on.
    fn populate_digits(m: &mut HashMap<(InputMode, Key), Command>) {
        let alt_shift = Modifiers {
            ctrl: false,
            alt: true,
            shift: true,
            super_key: false,
        };
        for n in 1..=9usize {
            let ch = char::from_digit(n as u32, 10).expect("single digit");
            m.insert(
                (InputMode::Normal, Key::alt(KeyCode::Char(ch))),
                Command::WorkspaceGoto(vec![n]),
            );
            m.insert(
                (InputMode::Normal, Key::new(KeyCode::Char(ch), alt_shift)),
                Command::MoveToWorkspace(vec![n]),
            );
        }
    }

    fn populate_workspace_basics(m: &mut HashMap<(InputMode, Key), Command>) {
        for (key, cmd) in [
            (Key::alt(KeyCode::Char('n')), Command::NewWorkspace),
            (
                Key::alt(KeyCode::Char('w')),
                Command::CloseWorkspace(Vec::new()),
            ),
        ] {
            m.insert((InputMode::Normal, key), cmd);
        }
        m.insert((InputMode::Normal, shift_tab_key()), Command::ToggleSidebar);
        Self::populate_digits(m);
    }

    /// beginner: mouse + f-keys + arrows. no chord table to memorize.
    fn populate_beginner(m: &mut HashMap<(InputMode, Key), Command>) {
        Self::populate_fkeys(m);
        Self::populate_workspace_basics(m);
        // arrows focus panes, the most discoverable mapping there is
        for (key, cmd) in [
            (Key::alt(KeyCode::Left), Command::FocusLeft),
            (Key::alt(KeyCode::Down), Command::FocusDown),
            (Key::alt(KeyCode::Up), Command::FocusUp),
            (Key::alt(KeyCode::Right), Command::FocusRight),
        ] {
            m.insert((InputMode::Normal, key), cmd);
        }
        m.insert(
            (InputMode::Normal, Key::alt(KeyCode::Char('q'))),
            Command::ClosePane,
        );

        // alt+shift bindings for all profiles
        let alt_shift = Modifiers {
            ctrl: false,
            alt: true,
            shift: true,
            super_key: false,
        };
        m.insert(
            (InputMode::Normal, Key::new(KeyCode::Char('h'), alt_shift)),
            Command::SwapLeft,
        );
        m.insert(
            (InputMode::Normal, Key::new(KeyCode::Char('j'), alt_shift)),
            Command::SwapDown,
        );
        m.insert(
            (InputMode::Normal, Key::new(KeyCode::Char('k'), alt_shift)),
            Command::SwapUp,
        );
        m.insert(
            (InputMode::Normal, Key::new(KeyCode::Char('l'), alt_shift)),
            Command::SwapRight,
        );
        m.insert(
            (InputMode::Normal, Key::new(KeyCode::Left, alt_shift)),
            Command::MoveToWorkspacePrev,
        );
        m.insert(
            (InputMode::Normal, Key::new(KeyCode::Right, alt_shift)),
            Command::MoveToWorkspaceNext,
        );
    }

    /// hypr (default): hyprland-style hjkl focus + shift to swap, alt+arrows cycle workspaces.
    fn populate_hypr(m: &mut HashMap<(InputMode, Key), Command>) {
        Self::populate_shared(m);
    }

    /// vim: hjkl only, vi grammar. alt+[ ] navigate workspaces, -/= resize,
    /// alt+x close, alt+e theme, alt+p palette (like fzf).
    fn populate_vim(m: &mut HashMap<(InputMode, Key), Command>) {
        Self::populate_fkeys(m);
        Self::populate_workspace_basics(m);

        let alt_shift = Modifiers {
            ctrl: false,
            alt: true,
            shift: true,
            super_key: false,
        };
        for (key, cmd) in [
            (Key::alt(KeyCode::Char('h')), Command::FocusLeft),
            (Key::alt(KeyCode::Char('j')), Command::FocusDown),
            (Key::alt(KeyCode::Char('k')), Command::FocusUp),
            (Key::alt(KeyCode::Char('l')), Command::FocusRight),
            (Key::new(KeyCode::Char('h'), alt_shift), Command::SwapLeft),
            (Key::new(KeyCode::Char('j'), alt_shift), Command::SwapDown),
            (Key::new(KeyCode::Char('k'), alt_shift), Command::SwapUp),
            (Key::new(KeyCode::Char('l'), alt_shift), Command::SwapRight),
            // vi-style bracket motion across workspaces
            (Key::alt(KeyCode::Char('[')), Command::WorkspacePrev),
            (Key::alt(KeyCode::Char(']')), Command::WorkspaceNext),
            // - and = shrink/grow the split width (vim < and >)
            (Key::alt(KeyCode::Char('-')), Command::ResizeLeft),
            (Key::alt(KeyCode::Char('=')), Command::ResizeRight),
            // :x quits the window, :e reopens, :palette fzf
            (Key::alt(KeyCode::Char('x')), Command::ClosePane),
            (
                Key::new(KeyCode::Char('x'), alt_shift),
                Command::KillSession,
            ),
            (Key::alt(KeyCode::Char('z')), Command::ToggleZoom),
            (Key::alt(KeyCode::Char('e')), Command::ThemeMenu),
            (Key::alt(KeyCode::Char('p')), Command::CommandPalette),
            (Key::alt(KeyCode::Char('s')), Command::ToggleSidebar),
            // alt+shift+arrows moves the pane between workspaces
            (
                Key::new(KeyCode::Left, alt_shift),
                Command::MoveToWorkspacePrev,
            ),
            (
                Key::new(KeyCode::Right, alt_shift),
                Command::MoveToWorkspaceNext,
            ),
        ] {
            m.insert((InputMode::Normal, key), cmd);
        }
    }

    /// dev: editor/IDE ergonomics. arrows focus, alt+enter new pane,
    /// alt+k palette (ctrl+k in every IDE), alt+[ ] workspace cycle.
    fn populate_dev(m: &mut HashMap<(InputMode, Key), Command>) {
        Self::populate_fkeys(m);
        Self::populate_workspace_basics(m);

        let alt_shift = Modifiers {
            ctrl: false,
            alt: true,
            shift: true,
            super_key: false,
        };
        for (key, cmd) in [
            (Key::alt(KeyCode::Left), Command::FocusLeft),
            (Key::alt(KeyCode::Down), Command::FocusDown),
            (Key::alt(KeyCode::Up), Command::FocusUp),
            (Key::alt(KeyCode::Right), Command::FocusRight),
            (Key::alt(KeyCode::Char('[')), Command::WorkspacePrev),
            (Key::alt(KeyCode::Char(']')), Command::WorkspaceNext),
            // alt+enter = new pane, mirrors every IDE's "new terminal"
            (Key::alt(KeyCode::Enter), Command::NewPane),
            // alt+k = command palette, mirrors ctrl+k in IDEs
            (Key::alt(KeyCode::Char('k')), Command::CommandPalette),
            (Key::alt(KeyCode::Char('q')), Command::ClosePane),
            (Key::alt(KeyCode::Char('t')), Command::ThemeMenu),
            (
                Key::new(KeyCode::Left, alt_shift),
                Command::MoveToWorkspacePrev,
            ),
            (
                Key::new(KeyCode::Right, alt_shift),
                Command::MoveToWorkspaceNext,
            ),
        ] {
            m.insert((InputMode::Normal, key), cmd);
        }
    }

    fn populate_fkeys(m: &mut HashMap<(InputMode, Key), Command>) {
        for (key, cmd) in [
            (Key::plain(KeyCode::F(1)), Command::Help),
            (Key::plain(KeyCode::F(2)), Command::SettingsMenu),
            (Key::plain(KeyCode::F(3)), Command::NewPane),
            (Key::plain(KeyCode::F(4)), Command::ClosePane),
            (Key::plain(KeyCode::F(5)), Command::ToggleZoom),
            (Key::plain(KeyCode::F(6)), Command::ToggleFloat),
            (Key::plain(KeyCode::F(7)), Command::ToggleScratch),
            (Key::plain(KeyCode::F(8)), Command::Search),
            (Key::plain(KeyCode::F(9)), Command::CopyMode),
            (Key::plain(KeyCode::F(10)), Command::ResizeMode),
            (Key::plain(KeyCode::F(11)), Command::CommandPalette),
            (Key::plain(KeyCode::F(12)), Command::ThemeMenu),
        ] {
            m.insert((InputMode::Normal, key), cmd);
        }
    }
}
