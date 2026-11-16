//! regression: shift+tab toggles the workspace sidebar in every profile.
use venti::config::schema::ProfileMode;
use venti::input::bindings::KeyBindings;
use venti::input::commands::Command;
use venti::input::keys::{Key, KeyCode, Modifiers};
use venti::input::modes::InputMode;

#[test]
fn shift_tab_toggles_the_sidebar_in_every_profile() {
    let shift_tab = Key::new(
        KeyCode::Tab,
        Modifiers {
            ctrl: false,
            alt: false,
            shift: true,
            super_key: false,
        },
    );
    for profile in [
        ProfileMode::Beginner,
        ProfileMode::Hypr,
        ProfileMode::Vim,
        ProfileMode::Dev,
    ] {
        let bindings = KeyBindings::new(profile, &[]);
        assert_eq!(
            bindings.find(InputMode::Normal, &shift_tab).cloned(),
            Some(Command::ToggleSidebar),
            "shift+tab must toggle the sidebar in {profile:?}"
        );
    }
}
