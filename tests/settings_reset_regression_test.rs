use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use venti::config::schema::{Config, WorkspaceBarPosition};
use venti::config::theme::Theme;
use venti::server::core::{CoreEvent, ServerCore};

fn input(bytes: impl Into<Vec<u8>>) -> CoreEvent {
    CoreEvent::ClientInput {
        client_id: 0,
        bytes: bytes.into(),
    }
}

#[test]
fn alt_r_does_not_reset_config_in_settings_menu() {
    let mut config = Config::default();
    config.ui.show_welcome = false;
    config.ui.workspace_bar = WorkspaceBarPosition::Left;
    let running = Arc::new(AtomicBool::new(true));
    let (mut core, _tx) = ServerCore::new(
        "alt-r-config-test",
        config,
        Theme::default_theme(),
        running,
        None,
    );

    core.handle_event(input(b"\x1bOQ".to_vec()));
    core.handle_event(input(b"\x1br".to_vec()));

    assert_eq!(core.config.ui.workspace_bar, WorkspaceBarPosition::Left);
    assert_eq!(core.settings_menu.filter, "");
}
