//! Integration tests for the WFC World Map Creator subcommand.

use roco_cli::test_harness::MockCliRunner;

#[test]
fn test_cli_wfc_map_generation() {
    // ROCO_DIR is passed per-process via with_env (NOT std::env::set_var):
    // the process-global env is shared across parallel test threads, so
    // set_var here raced with the other WFC test and the binary wrote its
    // HTML into the other test's directory (flaky "Expected wfc_map.html").
    let runner = MockCliRunner::new();
    let roco_dir = runner.working_dir().to_string_lossy().into_owned();
    let runner = runner.with_env("ROCO_DIR", roco_dir);

    // 1. Run basic map generation
    let res = runner.run_binary(["map", "--width", "10", "--height", "6", "--seed", "12345"]);
    res.assert_success();

    // Verify stdout contains success indicators
    res.assert_stdout_contains("Synthesizing wave function constraints");
    res.assert_stdout_contains("World map generated successfully");

    // Verify HTML webapp file creation
    let html_path = runner.working_dir().join("wfc_map.html");
    assert!(html_path.exists(), "Expected wfc_map.html to be created");
    let html_content = std::fs::read_to_string(&html_path).unwrap();
    assert!(html_content.contains("Wave Function Collapse"));
}

#[test]
fn test_cli_wfc_map_ttrpg_export() {
    let runner = MockCliRunner::new();
    let roco_dir = runner.working_dir().to_string_lossy().into_owned();
    let runner = runner.with_env("ROCO_DIR", roco_dir);

    // 2. Run map generation with TTRPG export
    let res = runner.run_binary([
        "map", "--width", "12", "--height", "8", "--seed", "54321", "--ttrpg",
    ]);
    res.assert_success();

    res.assert_stdout_contains("Successfully exported travelable biome regions");

    // Verify ttrpg_state.json was generated and is valid
    let json_path = runner.working_dir().join("ttrpg_state.json");
    assert!(
        json_path.exists(),
        "Expected ttrpg_state.json to be created"
    );
    let json_content = std::fs::read_to_string(&json_path).unwrap();

    // Assert structured world elements from WFC are registered
    assert!(
        json_content.contains("The Glimmering Ocean")
            || json_content.contains("The Glimmering Bay")
            || json_content.contains("The Glimmering Plains")
    );
    assert!(json_content.contains("\"player\""));
    assert!(json_content.contains("\"world\""));
}
