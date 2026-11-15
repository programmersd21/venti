use std::str::FromStr;
use venti::input::commands::Command;

#[test]
fn parses() {
    assert_eq!(
        Command::from_str("workspace_goto 1").unwrap(),
        Command::WorkspaceGoto(vec![1])
    );
    assert_eq!(
        Command::from_str("workspace_goto 2 4 5").unwrap(),
        Command::WorkspaceGoto(vec![2, 4, 5])
    );
    assert_eq!(
        Command::from_str("close_workspace").unwrap(),
        Command::CloseWorkspace(vec![])
    );
    assert_eq!(
        Command::from_str("close_workspace 2").unwrap(),
        Command::CloseWorkspace(vec![2])
    );
    assert_eq!(
        Command::from_str("close_workspace 2 3 7").unwrap(),
        Command::CloseWorkspace(vec![2, 3, 7])
    );
    assert_eq!(
        Command::from_str("move_to_workspace 1 3").unwrap(),
        Command::MoveToWorkspace(vec![1, 3])
    );
    assert!(Command::from_str("workspace_goto").is_err());
    assert!(Command::from_str("move_to_workspace").is_err());
    assert!(Command::from_str("close_workspace 0").is_err());
    assert!(Command::from_str("workspace_goto 1 x").is_err());
}
