use crate::validation::agent::{StoryModeAgent};
use crate::validation::intent::{IntentClassifier, StoryIntent};
use roco_engine::{CompletionRequest, CompletionResponse, EngineError, ModelBackend};
use futures::future::BoxFuture;
use std::fs;
use std::path::PathBuf;

struct MockModel {
    response: String,
}

impl ModelBackend for MockModel {
    fn name(&self) -> &str {
        "mock"
    }

    fn complete(
        &self,
        _request: CompletionRequest,
    ) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        let resp = CompletionResponse {
            text: self.response.clone(),
            usage: Default::default(),
            parsed: None,
            trace: Vec::new(),
        };
        Box::pin(async move { Ok(resp) })
    }
}

#[test]
fn test_slash_command_parsing() {
    let classifier = IntentClassifier::default();

    // /draft
    let res = classifier.classify(&MockModel { response: "".into() }, "/draft 3", &[], None);
    assert!(res.is_ok());
    assert_eq!(res.unwrap().intent, StoryIntent::DraftChapter(3));

    // /revert
    let res = classifier.classify(&MockModel { response: "".into() }, "/revert 1", &[], None);
    assert!(res.is_ok());
    assert_eq!(res.unwrap().intent, StoryIntent::RevertChapter(1));

    // /backups
    let res = classifier.classify(&MockModel { response: "".into() }, "/backups 2", &[], None);
    assert!(res.is_ok());
    assert_eq!(res.unwrap().intent, StoryIntent::ListBackups(2));

    // /apply
    let res = classifier.classify(&MockModel { response: "".into() }, "/apply Alice Bob", &[], None);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().intent,
        StoryIntent::ApplyRename {
            old: "Alice".to_string(),
            new: "Bob".to_string()
        }
    );
}

#[test]
fn test_new_safe_operations_integration() {
    let ws_path = PathBuf::from(".roco/workspaces/my-story-test");
    fs::create_dir_all(&ws_path).unwrap();

    // Create outline & wiki
    fs::write(
        ws_path.join("outline.md"),
        "Title: My Story\nGenre: Sci-Fi\n\n## Chapter 1: The Crash\nThe astronaut crashes.\n\n## Chapter 2: The Vessel\nThey discover a derelict alien vessel.\n",
    )
    .unwrap();
    fs::write(
        ws_path.join("wiki.md"),
        "## Characters\n### Alice\nAn astronaut.\n",
    )
    .unwrap();

    // Flat chapters dir
    let chapters_dir = ws_path.join("chapters");
    fs::create_dir_all(&chapters_dir).unwrap();
    fs::write(
        chapters_dir.join("01-chapter.md"),
        "# The Crash\nAlice crawled out of the wreckage.\n",
    )
    .unwrap();

    // Pre-create chapter 2 so writing to it will trigger a backup!
    let ch2_path = chapters_dir.join("02-chapter.md");
    fs::write(&ch2_path, "# The Vessel\nOld draft content.\n").unwrap();

    let mut agent = StoryModeAgent::new();

    // Lock the workspace
    agent.session_manager_mut().lock("my-story-test").unwrap();

    let mock_backend = MockModel {
        response: r#"{"title": "The Vessel", "content": "The giant metallic hull of the vessel loomed in the thick fog. Alice approached with caution. It was silent."}"#.to_string(),
    };

    // 1. Test /draft 2 (this will backup the "Old draft content" we pre-created!)
    let draft_res = agent.process(&mock_backend, "/draft 2").unwrap();
    let draft_display = draft_res.display();
    assert!(draft_display.contains("Drafted successfully"));
    assert!(draft_display.contains("The Vessel"));

    // Check file on disk is updated with the model response
    let ch2_content = fs::read_to_string(&ch2_path).unwrap();
    assert!(ch2_content.contains("giant metallic hull"));

    // 2. Test /backups 2 (the backup of "Old draft content" must exist!)
    let backups_res = agent.process(&mock_backend, "/backups 2").unwrap();
    let backups_display = backups_res.display();
    assert!(backups_display.contains("Available Backups"));

    // 3. Test /revert 2
    let revert_res = agent.process(&mock_backend, "/revert 2").unwrap();
    let revert_display = revert_res.display();
    assert!(revert_display.contains("Successfully restored"));

    // Verify file content is reverted back to "Old draft content"
    let ch2_reverted = fs::read_to_string(&ch2_path).unwrap();
    assert!(ch2_reverted.contains("Old draft content"));

    // 4. Test /apply Alice Bob
    let apply_res = agent.process(&mock_backend, "/apply Alice Bob").unwrap();
    let apply_display = apply_res.display();
    assert!(apply_display.contains("Rename Completed"));
    assert!(apply_display.contains("Alice"));

    // Verify Bob is now in chapter 1 and wiki
    let ch1_content = fs::read_to_string(chapters_dir.join("01-chapter.md")).unwrap();
    assert!(ch1_content.contains("Bob crawled"));
    assert!(!ch1_content.contains("Alice crawled"));

    let wiki_content = fs::read_to_string(ws_path.join("wiki.md")).unwrap();
    assert!(wiki_content.contains("Bob"));
    assert!(!wiki_content.contains("Alice"));

    // Clean up
    fs::remove_dir_all(&ws_path).ok();
}
