//! DRY CLI helper and argument parser integration tests.

use roco_cli::{has_help_flag, parse_opt};

// ── parse_opt ────────────────────────────────────────────────────────────

#[test]
fn test_parse_opt_separate_args() {
    // --key value as two separate args
    let args = vec!["--model", "rwkv-7", "--port", "18080"];
    assert_eq!(parse_opt("--model", &args), Some("rwkv-7"));
    assert_eq!(parse_opt("--port", &args), Some("18080"));
    assert_eq!(parse_opt("--missing", &args), None);
    assert_eq!(parse_opt("--model", &[]), None);
}

#[test]
fn test_parse_opt_eq_format() {
    // --key=value as single arg
    let args = vec!["--model=rwkv-7", "--port=18080"];
    assert_eq!(parse_opt("--model", &args), Some("rwkv-7"));
    assert_eq!(parse_opt("--port", &args), Some("18080"));
}

#[test]
fn test_parse_opt_mixed_format() {
    // Mixed: some with =, some separate
    let args = vec!["--model=rwkv-7", "--port", "18080"];
    assert_eq!(parse_opt("--model", &args), Some("rwkv-7"));
    assert_eq!(parse_opt("--port", &args), Some("18080"));
}

#[test]
fn test_parse_opt_eq_with_empty_value() {
    // --key=  (empty value after =)
    let args = vec!["--flag="];
    assert_eq!(parse_opt("--flag", &args), Some(""));
}

#[test]
fn test_parse_opt_eq_prefix_not_match() {
    // Make sure --flag doesn't match --flagstaff or similar prefixes
    let args = vec!["--flagstaff=1", "--flag", "2"];
    assert_eq!(parse_opt("--flag", &args), Some("2"));
    assert_eq!(parse_opt("--flagstaff", &args), Some("1"));
}

// ── has_help_flag ───────────────────────────────────────────────────────

#[test]
fn test_has_help_flag() {
    assert!(has_help_flag(&["--help"]));
    assert!(has_help_flag(&["-h"]));
    assert!(has_help_flag(&["start", "--help"]));
    assert!(has_help_flag(&["--help", "start"]));
    assert!(has_help_flag(&["-h", "--port", "8000"]));
    assert!(!has_help_flag(&[]));
    assert!(!has_help_flag(&["start"]));
    assert!(!has_help_flag(&["--port", "8000"]));
    assert!(!has_help_flag(&["help"])); // bare 'help' is not a flag
}

#[tokio::test]
async fn test_cli_workspace_interaction_dry() {
    let tmp = tempfile::tempdir().unwrap();
    let ws =
        roco_workspace::Workspace::new(tmp.path(), roco_workspace::WorkspaceKind::User).unwrap();

    // Verify workspace initialization via CLI context
    let resolve_res = ws.resolve("chapter1.md");
    assert!(resolve_res.is_ok());
    assert!(resolve_res.unwrap().starts_with(tmp.path()));
}
