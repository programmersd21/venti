use venti::config::validate::KNOWN_COMMANDS;
use venti::input::Command;
use venti::render::filtered_commands;
use venti::render::{command_doc, ghost_remainder};

#[test]
fn every_known_command_has_a_description() {
    for cmd in KNOWN_COMMANDS {
        assert!(
            command_doc(cmd).is_some(),
            "missing doc for known command {cmd:?}"
        );
    }
}

#[test]
fn ghost_is_remainder_of_prefix_match() {
    let ghost = ghost_remainder("spl");
    assert_eq!(ghost.as_deref(), Some("it_down"));
}

#[test]
fn prefix_matches_rank_before_fuzzy() {
    let matches = filtered_commands("spl");
    assert!(!matches.is_empty());
    assert!(matches[0].starts_with("split"));
}

#[test]
fn unknown_query_has_no_ghost() {
    assert_eq!(ghost_remainder("zzzzzz"), None);
}

#[test]
fn empty_query_has_no_ghost() {
    assert_eq!(ghost_remainder(""), None);
}

#[test]
fn every_known_command_name_is_accepted_by_the_parser() {
    for cmd in KNOWN_COMMANDS {
        assert!(
            Command::is_known_name(cmd),
            "KNOWN_COMMANDS entry {cmd:?} is rejected by Command::from_str"
        );
    }
}
