pub mod anim;
pub mod backend;
pub mod cat;
pub mod chrome;
pub mod completion;
pub mod context_menu;
pub mod glyphs;
pub mod help;
pub mod palette;
pub mod pane_view;
pub mod settings_menu;
pub mod style;
pub mod theme_menu;
pub mod toast;
pub mod vim_cmd;
pub mod welcome;

pub use backend::ClientBackend;
pub use chrome::{
    clock_text, render_hint_bar, render_pane_border, render_workspace_bar, render_workspace_sidebar,
};
pub use completion::{command_doc, ghost_remainder};
pub use context_menu::{ContextMenuState, render_context_menu};
pub use help::render_help;
pub use palette::{PaletteState, filtered_commands, render_palette};
pub use pane_view::PaneView;
pub use settings_menu::{Setting, SettingTab, SettingsMenuState, render_settings_menu};
pub use theme_menu::{BUILTIN_THEMES, ThemeMenuState, render_theme_menu};
pub use toast::render_toast;
pub use vim_cmd::{render_vim_cmd_bar, render_vim_cmd_popup};
pub use welcome::render_welcome;
