//! DRY CLI helper and argument parser integration tests.

use roco_cli::parse_opt;

#[test]
fn test_parse_opt_dry_suite() {
    let cases = vec![
        (
            vec!["--model", "rwkv-7", "--port", "8080"],
            "--model",
            Some("rwkv-7"),
        ),
        (
            vec!["--model", "rwkv-7", "--port", "8080"],
            "--port",
            Some("8080"),
        ),
        (vec!["--model", "rwkv-7"], "--missing", None),
        (vec![], "--model", None),
    ];

    for (args, flag, expected) in cases {
        assert_eq!(parse_opt(flag, &args), expected);
    }
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
