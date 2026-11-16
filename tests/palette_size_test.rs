//! the command palette lays out one row per command at
//! `DOC_COLUMN`; every known command and its docs must fit.
use venti::config::validate::KNOWN_COMMANDS;
use venti::render::command_doc;

const MARKER_WIDTH: usize = 2;
const COMMAND_COL_WIDTH: usize = 24;
const SEPARATOR_WIDTH: usize = 2;
/// characters left for the description column: [border][pad][2 marker][24 name][2 sep][desc][pad][border]
const DESCRIPTION_COL_WIDTH: usize = 90 - 4 - MARKER_WIDTH - COMMAND_COL_WIDTH - SEPARATOR_WIDTH;

#[test]
fn all_command_descriptions_fit_without_truncation() {
    for &cmd in KNOWN_COMMANDS {
        assert!(
            cmd.len() <= COMMAND_COL_WIDTH,
            "command '{cmd}' is {} chars but only {COMMAND_COL_WIDTH} are allocated",
            cmd.len()
        );

        if let Some(doc) = command_doc(cmd) {
            assert!(
                doc.len() <= DESCRIPTION_COL_WIDTH,
                "description for '{cmd}' is {} chars but only {DESCRIPTION_COL_WIDTH} are available",
                doc.len()
            );
        }
    }
}
