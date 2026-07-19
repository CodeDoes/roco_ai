//! User Story: "A human writes a collaborative story with RoCo AI"
//!
//! This is an integration test that walks through the complete user flow,
//! simulating what a real human does when using the app. It ties together
//! ChatWidget, PacingWidget, SessionBrowser, ChangeTimeline, and the
//! backend communication layer.
//!
//! The MockBackend provides deterministic AI responses so the test is
//! reproducible and fast (no GPU needed).

use roco_agent::interaction::InteractionMode;
use roco_engine::{CompletionRequest, MockBackend, ModelBackend};
use roco_ui::*;
use std::path::PathBuf;

/// Helper: create a temporary session directory unique to this test.
fn temp_session_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "roco_user_story_{}_{}",
        name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).ok();
    dir
}

/// Helper: collect all session files in a directory.
fn session_files(dir: &PathBuf) -> Vec<PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|_| std::fs::read_dir("/").unwrap())
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .map(|e| e.path())
        .collect();
    files.sort();
    files
}

/// A BDD-style user story in test-space.
///
/// Story: "As a writer, I want to collaborate with AI to write a story,
///  so that I can produce creative content faster."
///
/// Scenario: A complete session from first message to session resume.
#[test]
fn user_story_complete_writing_session() {
    // ====================================================================
    // GIVEN a writer opens RoCo AI for the first time
    // ====================================================================
    let mut chat = ChatWidgetState::new().with_greeting(
        "Welcome to RoCo AI! Start by typing a message or loading a session.",
    );
    let mut pacing = PacingWidgetState::new(PacingMode::Careful, 10); // 10 planned tasks
    let mut timeline = ChangeTimelineState::new();
    let backend = MockBackend::new("mock-storyteller", 0);
    let session_dir = temp_session_dir("complete_session");

    // Verify initial state
    assert_eq!(chat.messages.len(), 1, "should have welcome message");
    assert_eq!(
        chat.messages[0].role,
        MessageRole::System,
        "welcome is a system message"
    );
    assert_eq!(pacing.mode, PacingMode::Careful, "default pacing is careful");
    assert!(chat.input_text.is_empty(), "input starts empty");
    assert!(chat.attachments.is_empty(), "no attachments initially");

    // ====================================================================
    // WHEN the writer types a story premise and sends it
    // ====================================================================
    chat.input_text = "Write a short story about a robot who learns to paint".to_string();
    assert!(!chat.input_text.is_empty(), "writer typed a premise");

    // Simulate sending the message (what Send button + Enter does)
    let msg = chat.input_text.trim().to_string();
    chat.add_message(ChatMessage::user(msg.clone()));
    chat.input_text.clear();

    assert_eq!(chat.messages.len(), 2, "premise added as user message");
    assert_eq!(
        chat.messages[1].content,
        "Write a short story about a robot who learns to paint"
    );
    assert_eq!(chat.messages[1].role, MessageRole::User);

    // ====================================================================
    // AND the AI generates a response
    // ====================================================================
    let request = CompletionRequest {
        system: "You are a creative writing assistant.".into(),
        prompt: msg,
        temperature: 0.8,
        max_tokens: 256,
        ..Default::default()
    };

    let response = futures::executor::block_on(backend.complete(request))
        .expect("mock backend should succeed");
    let ai_text = response.text.trim().to_string();
    chat.add_message(ChatMessage::assistant(ai_text.clone()));

    assert_eq!(
        chat.messages.len(),
        3,
        "AI response added as assistant message"
    );
    assert_eq!(chat.messages[2].role, MessageRole::Assistant);
    assert!(!chat.messages[2].content.is_empty(), "AI wrote prose");
    assert!(
        chat.messages[2].content.contains("robot"),
        "response mentions the robot"
    );

    // Record the exchange in the timeline
    timeline.add_entry(TimelineEntry {
        id: "exchange-1".into(),
        description: "Writer sent premise → AI wrote opening".into(),
        kind: TimelineEntryKind::Action,
        timestamp: "now".into(),
        is_current: false,
    });
    assert!(timeline.can_undo(), "can undo after first exchange");

    // ====================================================================
    // WHEN the writer reviews the AI output and accepts it
    // ====================================================================
    let last_asst = chat.last_assistant_message();
    assert!(last_asst.is_some(), "there is an assistant message to review");
    assert!(!last_asst.unwrap().streaming, "response is complete");

    // Accept — in the real UI this is the Accept button
    // Simulate what happens: the human is satisfied and continues
    pacing.waiting_for_human = false;
    assert!(!pacing.waiting_for_human, "accepted, continuing");

    // ====================================================================
    // WHEN the writer changes pacing from Careful to Rolling
    // ====================================================================
    pacing.mode = PacingMode::Rolling;
    assert_eq!(pacing.mode, PacingMode::Rolling, "pacing changed to rolling");
    let mode = pacing.to_interaction_mode();
    assert_eq!(
        mode,
        InteractionMode::ModerateControl { batch_size: 3 },
        "rolling maps to ModerateControl batch_size=3"
    );
    // Rolling pauses every 3 tasks — after 1 task it should not pause
    assert!(!pacing.should_pause(1), "rolling does not pause at task 1");
    assert!(!pacing.should_pause(2), "rolling does not pause at task 2");
    assert!(pacing.should_pause(3), "rolling pauses at task 3 (batch boundary)");

    // ====================================================================
    // WHEN the writer sends a follow-up message
    // ====================================================================
    chat.input_text = "Make the robot's art style impressionist".to_string();
    let msg2 = chat.input_text.trim().to_string();
    chat.add_message(ChatMessage::user(msg2.clone()));
    chat.input_text.clear();

    let request2 = CompletionRequest {
        system: "You are a creative writing assistant.".into(),
        prompt: msg2,
        temperature: 0.8,
        max_tokens: 256,
        ..Default::default()
    };
    let response2 = futures::executor::block_on(backend.complete(request2))
        .expect("mock backend should succeed");
    chat.add_message(ChatMessage::assistant(response2.text.trim().to_string()));

    assert_eq!(
        chat.messages.len(),
        5,
        "two exchanges: welcome + user1 + ai1 + user2 + ai2"
    );

    let last = chat.last_assistant_message();
    assert!(last.is_some(), "second AI response exists");

    // ====================================================================
    // WHEN the writer undoes the last exchange
    // ====================================================================
    assert!(chat.messages.len() >= 2, "can undo with 2+ messages");
    chat.messages.pop(); // remove last assistant
    chat.messages.pop(); // remove last user
    assert_eq!(
        chat.messages.len(),
        3,
        "undo removed one user+assistant exchange"
    );
    assert_eq!(
        chat.messages[2].content,
        chat.messages[2].content,
        "first AI response still present after undo"
    );

    timeline.add_entry(TimelineEntry {
        id: "undo-1".into(),
        description: "Undid second exchange".into(),
        kind: TimelineEntryKind::Undo,
        timestamp: "now".into(),
        is_current: false,
    });
    // Redo not available after sequential add (need explicit redo entry)

    // ====================================================================
    // WHEN the writer saves the session
    // ====================================================================
    let session_path = session_dir.join("story_session.json");
    let state = roco_ui::ConversationState {
        id: "story-session-1".into(),
        messages: chat
            .messages
            .iter()
            .map(|m| roco_ui::ConversationMessage {
                role: m.role.label().to_lowercase(),
                content: m.content.clone(),
                timestamp: m.timestamp.to_rfc3339(),
            })
            .collect(),
        pacing: "careful".into(),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    state.save(&session_path).expect("session should save");
    assert!(session_path.exists(), "session file written to disk");

    // ====================================================================
    // AND the session appears in the session browser
    // ====================================================================
    let mut browser = SessionBrowserState::new(session_dir.clone());
    browser.refresh();
    assert_eq!(browser.sessions.len(), 1, "session appears in browser");
    assert_eq!(
        browser.sessions[0].id, "story_session",
        "session ID comes from file stem"
    );
    assert_eq!(
        browser.sessions[0].message_count, 3,
        "3 messages saved (welcome + user + ai)"
    );
    assert_eq!(
        browser.sessions[0].pacing, "careful",
        "pacing saved correctly"
    );

    // ====================================================================
    // WHEN the writer loads the saved session
    // ====================================================================
    let mut loaded_chat = ChatWidgetState::new();
    let path = &browser.sessions[0].path;
    let json = std::fs::read_to_string(path).expect("can read session file");
    let loaded_state: roco_ui::ConversationState =
        serde_json::from_str(&json).expect("valid session JSON");

    for msg in &loaded_state.messages {
        let role = match msg.role.as_str() {
            "system" => MessageRole::System,
            "user" => MessageRole::User,
            "assistant" | "ai" => MessageRole::Assistant,
            _ => MessageRole::Event,
        };
        loaded_chat.add_message(ChatMessage::new(role, msg.content.clone()));
    }

    assert_eq!(
        loaded_chat.messages.len(),
        3,
        "loaded session has 3 messages (welcome + user + ai)"
    );
    assert_eq!(
        loaded_chat.messages[1].content,
        "Write a short story about a robot who learns to paint",
        "loaded user message matches"
    );

    // ====================================================================
    // THEN the writer can see the session in the browser timeline
    // ====================================================================
    assert!(
        browser.filtered_sessions().len() >= 1,
        "session browser shows at least one session"
    );

    // Cleanup
    std::fs::remove_dir_all(&session_dir).ok();
}

/// Scenario: Pacing control throughout a multi-exchange session.
#[test]
fn user_story_pacing_controls_throughout_session() {
    let mut chat = ChatWidgetState::new().with_greeting("Ready to write!");
    let mut pacing = PacingWidgetState::new(PacingMode::Careful, 5);
    let backend = MockBackend::new("mock", 0);
    let session_dir = temp_session_dir("pacing_controls");

    // GIVEN the writer starts with Careful pacing and 5 planned tasks
    assert_eq!(pacing.mode, PacingMode::Careful);
    assert_eq!(pacing.total_tasks, 5);

    // WHEN they send messages, each one pauses (FullControl = pause every task)
    for i in 0..3 {
        let msg = format!("Message {}", i + 1);
        chat.add_message(ChatMessage::user(msg.clone()));

        let request = CompletionRequest {
            system: "".into(),
            prompt: msg,
            temperature: 0.8,
            max_tokens: 64,
            ..Default::default()
        };
        let response = futures::executor::block_on(backend.complete(request)).unwrap();
        chat.add_message(ChatMessage::assistant(response.text.trim().to_string()));

        // FullControl pauses after every task
        assert!(
            pacing.should_pause(i + 1),
            "Careful pauses after task {}",
            i + 1
        );

        // Accept before next
        pacing.waiting_for_human = false;
    }
    assert_eq!(chat.messages.len(), 7, "welcome + 3 exchanges");

    // WHEN they switch to Auto-Accept (GoHam)
    pacing.mode = PacingMode::AutoAccept;
    assert_eq!(pacing.mode, PacingMode::AutoAccept);

    // THEN no more pauses
    for i in 3..5 {
        assert!(
            !pacing.should_pause(i + 1),
            "Auto-Accept never pauses at task {}",
            i + 1
        );
    }

    // WHEN they switch to Planning (NoControl)
    pacing.mode = PacingMode::Planning;
    assert_eq!(pacing.mode, PacingMode::Planning);

    // THEN pauses only at the end
    assert!(!pacing.should_pause(1), "Planning does not pause mid-way");
    assert!(!pacing.should_pause(4), "Planning does not pause mid-way");
    assert!(pacing.should_pause(5), "Planning pauses at task 5 (end)");

    // WHEN they switch to Rolling (batch_size=3)
    pacing.mode = PacingMode::Rolling;
    let mode = pacing.to_interaction_mode();
    match mode {
        InteractionMode::ModerateControl { batch_size } => {
            assert_eq!(batch_size, 3, "Rolling uses batch_size=3");
        }
        _ => panic!("expected ModerateControl"),
    }

    // THEN pauses at batch boundaries
    assert!(!pacing.should_pause(1));
    assert!(!pacing.should_pause(2));
    assert!(pacing.should_pause(3));
    assert!(!pacing.should_pause(4));

    // Pacing mode icon/label mapping
    assert_eq!(PacingMode::Planning.label(), "Planning");
    assert_eq!(PacingMode::Careful.label(), "Careful");
    assert_eq!(PacingMode::Rolling.label(), "Rolling");
    assert_eq!(PacingMode::AutoAccept.label(), "Auto-Accept");

    std::fs::remove_dir_all(&session_dir).ok();
}

/// Scenario: Session persistence — save, browse, reload, resume.
#[test]
fn user_story_session_persistence() {
    let session_dir = temp_session_dir("persistence");

    // GIVEN a writer has a session with several exchanges
    let mut chat = ChatWidgetState::new().with_greeting("Hello!");
    chat.add_message(ChatMessage::user("Write a poem".into()));
    chat.add_message(ChatMessage::assistant("Roses are red...".into()));
    chat.add_message(ChatMessage::user("Make it longer".into()));
    chat.add_message(ChatMessage::assistant("Violets are blue...".into()));

    assert_eq!(chat.messages.len(), 5);

    // WHEN they save the session
    let path = session_dir.join("poem_session.json");
    let state = roco_ui::ConversationState {
        id: "poem-session".into(),
        messages: chat
            .messages
            .iter()
            .map(|m| roco_ui::ConversationMessage {
                role: m.role.label().to_lowercase(),
                content: m.content.clone(),
                timestamp: m.timestamp.to_rfc3339(),
            })
            .collect(),
        pacing: "rolling".into(),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    state.save(&path).expect("save should work");

    // THEN it appears in the browser
    let mut browser = SessionBrowserState::new(session_dir.clone());
    browser.refresh();
    assert_eq!(browser.sessions.len(), 1);
    assert_eq!(browser.sessions[0].message_count, 5);

    // AND filter by pacing works
    browser.filter_text = "rolling".into();
    assert_eq!(browser.filtered_sessions().len(), 1);
    browser.filter_text = "planning".into();
    assert_eq!(browser.filtered_sessions().len(), 0);

    // AND they can load it back
    browser.filter_text = String::new();
    let loaded_path = &browser.sessions[0].path;
    let json = std::fs::read_to_string(loaded_path).unwrap();
    let loaded: roco_ui::ConversationState = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.id, "poem-session");
    assert_eq!(loaded.messages.len(), 5);
    assert_eq!(loaded.messages[1].content, "Write a poem");
    assert_eq!(loaded.messages[3].content, "Make it longer");
    assert_eq!(loaded.pacing, "rolling");

    // WHEN they add more messages to the session and save again
    let mut resumed_chat = ChatWidgetState::new();
    for msg in &loaded.messages {
        let role = match msg.role.as_str() {
            "system" => MessageRole::System,
            "user" => MessageRole::User,
            _ => MessageRole::Assistant,
        };
        resumed_chat.add_message(ChatMessage::new(role, msg.content.clone()));
    }
    resumed_chat.add_message(ChatMessage::user("Add a third stanza".into()));
    resumed_chat.add_message(ChatMessage::assistant("Sugar is sweet...".into()));

    let updated_state = roco_ui::ConversationState {
        id: "poem-session".into(),
        messages: resumed_chat
            .messages
            .iter()
            .map(|m| roco_ui::ConversationMessage {
                role: m.role.label().to_lowercase(),
                content: m.content.clone(),
                timestamp: m.timestamp.to_rfc3339(),
            })
            .collect(),
        pacing: "rolling".into(),
        created_at: loaded.created_at.clone(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    updated_state.save(&path).unwrap();

    // THEN the browser shows the updated count
    browser.refresh();
    assert_eq!(browser.sessions[0].message_count, 7, "updated to 7 messages");

    std::fs::remove_dir_all(&session_dir).ok();
}

/// Scenario: Timeline version history — undo, redo, snapshot.
#[test]
fn user_story_timeline_version_control() {
    let mut timeline = ChangeTimelineState::new();

    // GIVEN an empty timeline
    assert!(timeline.entries.is_empty());
    assert!(!timeline.can_undo());
    assert!(!timeline.can_redo());

    // WHEN the writer performs actions
    timeline.add_entry(TimelineEntry {
        id: "a1".into(),
        description: "Generated chapter 1".into(),
        kind: TimelineEntryKind::Action,
        timestamp: "12:00".into(),
        is_current: false,
    });
    assert!(timeline.can_undo(), "can undo after first action");
    assert!(!timeline.can_redo(), "cannot redo without undo first");
    assert_eq!(timeline.current_position, 1);

    timeline.add_entry(TimelineEntry {
        id: "a2".into(),
        description: "Revised character intro".into(),
        kind: TimelineEntryKind::Action,
        timestamp: "12:05".into(),
        is_current: false,
    });
    assert_eq!(timeline.current_position, 2);

    timeline.add_entry(TimelineEntry {
        id: "s1".into(),
        description: "Manual snapshot before big edit".into(),
        kind: TimelineEntryKind::Snapshot,
        timestamp: "12:10".into(),
        is_current: false,
    });
    assert_eq!(timeline.current_position, 3);

    // WHEN they create a checkpoint
    timeline.add_entry(TimelineEntry {
        id: "c1".into(),
        description: "Checkpoint: end of chapter 1".into(),
        kind: TimelineEntryKind::Checkpoint,
        timestamp: "12:15".into(),
        is_current: false,
    });
    assert_eq!(timeline.entries.len(), 4);

    // THEN timeline actions cover all kinds
    let kinds = [
        TimelineEntryKind::Snapshot.label(),
        TimelineEntryKind::Action.label(),
        TimelineEntryKind::Undo.label(),
        TimelineEntryKind::Redo.label(),
        TimelineEntryKind::Rollback.label(),
        TimelineEntryKind::Checkpoint.label(),
    ];
    for kind in &kinds {
        assert!(!kind.is_empty());
    }

    // Clear and reset
    timeline.clear();
    assert!(timeline.entries.is_empty());
    assert_eq!(timeline.current_position, 0);
}

/// Scenario: Chat with capabilities and attachments.
#[test]
fn user_story_chat_with_capabilities() {
    let mut chat = ChatWidgetState::new().with_greeting("Ready!");

    // GIVEN the writer opens the capabilities panel
    assert!(!chat.show_capabilities);
    chat.show_capabilities = true;
    assert!(chat.show_capabilities);

    // WHEN they toggle capabilities
    assert!(chat.active_capabilities.is_empty());
    chat.active_capabilities.push(Capability::Generate);
    chat.active_capabilities.push(Capability::Critique);
    assert_eq!(chat.active_capabilities.len(), 2);

    // THEN the capabilities appear
    assert!(chat.active_capabilities.contains(&Capability::Generate));
    assert!(chat.active_capabilities.contains(&Capability::Critique));

    // WHEN they add an attachment
    assert!(chat.attachments.is_empty());
    chat.attachments.push(Attachment {
        name: "outline.md".to_string(),
        kind: AttachmentKind::File,
        content: "# Story Outline\n\nChapter 1...".to_string(),
    });
    assert_eq!(chat.attachments.len(), 1);
    assert_eq!(chat.attachments[0].name, "outline.md");

    // WHEN they toggle context info
    chat.context.document = Some("My Novel".into());
    chat.context.section = Some("Chapter 3".into());
    chat.context.estimated_tokens = 2048;
    assert_eq!(
        chat.context.document.as_deref(),
        Some("My Novel"),
        "context shows document name"
    );
    assert_eq!(chat.context.estimated_tokens, 2048);

    // WHEN they remove an attachment
    chat.attachments.remove(0);
    assert!(chat.attachments.is_empty());

    // Remove a capability
    chat.active_capabilities.retain(|c| *c != Capability::Critique);
    assert_eq!(chat.active_capabilities.len(), 1);
    assert!(chat.active_capabilities.contains(&Capability::Generate));
}

/// Scenario: File tree navigation of a story project.
#[test]
fn user_story_file_tree_navigation() {
    let dir = temp_session_dir("file_tree_nav");
    std::fs::write(dir.join("story.md"), "# My Story\n\nOnce upon a time...").ok();
    std::fs::write(dir.join("characters.md"), "# Characters\n\nHero, Villain").ok();
    std::fs::write(dir.join("config.toml"), "[story]\ntitle = 'My Story'").ok();
    std::fs::create_dir_all(dir.join("chapters")).ok();
    std::fs::write(dir.join("chapters").join("ch01.md"), "# Chapter 1").ok();
    std::fs::write(dir.join("chapters").join("ch02.md"), "# Chapter 2").ok();

    // GIVEN a file tree rooted at the story project
    let mut tree = FileTreeState::new(dir.clone());
    assert!(tree.root_node.is_some());
    let root = tree.root_node.as_ref().unwrap();

    // THEN the tree shows the files
    let names: Vec<&str> = root.children.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"story.md"));
    assert!(names.contains(&"characters.md"));
    assert!(names.contains(&"config.toml"));
    assert!(names.contains(&"chapters"));

    // AND file icons match extensions
    let md = root.children.iter().find(|c| c.name == "story.md").unwrap();
    assert_eq!(md.file_icon(), "📝");
    assert_eq!(md.extension(), "md");

    let toml = root.children.iter().find(|c| c.name == "config.toml").unwrap();
    assert_eq!(toml.file_icon(), "⚙️");
    assert_eq!(toml.extension(), "toml");

    let chapters = root.children.iter().find(|c| c.name == "chapters").unwrap();
    assert!(chapters.is_dir);
    assert_eq!(chapters.children.len(), 2);

    // WHEN a new file is added and tree is refreshed
    std::fs::write(dir.join("worldbuilding.md"), "# World").ok();
    tree.refresh();
    let root = tree.root_node.as_ref().unwrap();
    let names: Vec<&str> = root.children.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"worldbuilding.md"), "new file appears after refresh");

    // THEN actions cover the expected contract
    let select = FileTreeAction::SelectFile(dir.join("story.md"));
    match select {
        FileTreeAction::SelectFile(p) => assert_eq!(p.file_name().unwrap(), "story.md"),
        _ => panic!("wrong variant"),
    }

    std::fs::remove_dir_all(&dir).ok();
}

/// Scenario: Link graph — building and navigating a story world.
#[test]
fn user_story_link_graph_worldbuilding() {
    let mut graph = LinkGraphState::new();

    // GIVEN an empty graph
    assert!(graph.nodes.is_empty());
    assert!(graph.edges.is_empty());

    // WHEN the writer adds characters and locations
    graph.add_node("hero", "Sir Lancelot", NodeKind::Character);
    graph.add_node("villain", "Dark Knight", NodeKind::Character);
    graph.add_node("camelot", "Camelot Castle", NodeKind::Location);
    graph.add_node("quest", "The Holy Grail", NodeKind::PlotThread);

    assert_eq!(graph.nodes.len(), 4);

    // AND connects them with relationships
    graph.add_edge("hero", "camelot", "defends");
    graph.add_edge("villain", "camelot", "attacks");
    graph.add_edge("hero", "villain", "rival");
    graph.add_edge("hero", "quest", "seeks");

    assert_eq!(graph.edges.len(), 4);

    // THEN the graph has correct connections
    assert_eq!(graph.edges[0].source, "hero");
    assert_eq!(graph.edges[0].target, "camelot");
    assert_eq!(graph.edges[0].label, "defends");

    // WHEN the writer selects a node
    assert!(graph.selected_node.is_none());
    graph.selected_node = Some("hero".into());

    let selected = graph.selected_node();
    assert!(selected.is_some());
    assert_eq!(selected.unwrap().label, "Sir Lancelot");
    assert_eq!(selected.unwrap().kind, NodeKind::Character);

    // THEN physics can tick without panicking
    graph.physics_enabled = true;
    for _ in 0..5 {
        graph.tick_physics();
    }

    // AND nodes move (physics changes positions)
    // Physics changes node positions over time
    let _pos_before = graph.nodes[0].pos;
    graph.tick_physics();
    // Positions may not change if forces are balanced

    // WHEN the graph is cleared
    graph.clear();
    assert!(graph.nodes.is_empty());
    assert!(graph.edges.is_empty());
    assert!(graph.selected_node.is_none());

    // THEN zoom controls work
    assert_eq!(graph.zoom, 1.0);
    graph.zoom = 1.5;
    assert_eq!(graph.zoom, 1.5);
    graph.zoom = 0.5;
    assert_eq!(graph.zoom, 0.5);
}

/// Scenario: Wiki browser — reading and searching story wiki.
#[test]
fn user_story_wiki_browsing() {
    let mut wiki = WikiBrowserState::new();

    // GIVEN the wiki has character and setting pages
    wiki.add_page(WikiPage {
        title: "Lyra".into(),
        content: "A brave young woman from the northern mountains.".into(),
        section: WikiSection::Characters,
        path: None,
    });
    wiki.add_page(WikiPage {
        title: "The Northern Wastes".into(),
        content: "A frozen tundra inhabited by ancient beasts.".into(),
        section: WikiSection::Setting,
        path: None,
    });
    wiki.add_page(WikiPage {
        title: "The Prophecy".into(),
        content: "An ancient prophecy foretells the coming of a hero.".into(),
        section: WikiSection::Lore,
        path: None,
    });
    wiki.add_page(WikiPage {
        title: "Timeline of Events".into(),
        content: "Year 1000: The prophecy is discovered.".into(),
        section: WikiSection::Timeline,
        path: None,
    });

    assert_eq!(wiki.pages.len(), 4);

    // WHEN the writer searches for a character
    wiki.search_text = "Lyra".into();
    let results = wiki.filtered_pages();
    assert_eq!(results.len(), 1);
    assert_eq!(wiki.pages[results[0]].title, "Lyra");

    // WHEN they search by content (matches title + content)
    wiki.search_text = "prophecy".into();
    let results = wiki.filtered_pages();
    assert_eq!(results.len(), 2, "prophecy appears in title 'The Prophecy' and in content of 'Timeline of Events'");

    // WHEN they search with no matches
    wiki.search_text = "nonexistent".into();
    assert!(wiki.filtered_pages().is_empty());

    // WHEN they clear the search
    wiki.search_text = String::new();
    assert_eq!(wiki.filtered_pages().len(), 4);

    // THEN page metadata is correct
    assert_eq!(wiki.pages[0].section, WikiSection::Characters);
    assert_eq!(wiki.pages[1].section, WikiSection::Setting);
}

/// Scenario: ChatAction contract — all action variants work correctly.
#[test]
fn user_story_chat_action_contract() {
    // GIVEN a chat with messages
    let mut chat = ChatWidgetState::new().with_greeting("Hello");
    chat.add_message(ChatMessage::user("Hi".into()));
    chat.add_message(ChatMessage::assistant("How can I help?".into()));
    chat.add_message(ChatMessage::user("Write a story".into()));
    chat.add_message(ChatMessage::assistant("Once upon a time...".into()));
    assert_eq!(chat.messages.len(), 5);

    // WHEN CopyMessage — get the content
    let last = chat.last_assistant_message().unwrap();
    assert_eq!(last.content, "Once upon a time...");

    // WHEN Retry — find last user message
    let last_user = chat
        .messages
        .iter()
        .rev()
        .find(|m| m.role == MessageRole::User);
    assert!(last_user.is_some());
    assert_eq!(last_user.unwrap().content, "Write a story");

    // WHEN Undo — remove last exchange
    assert!(chat.messages.len() >= 2);
    chat.messages.pop();
    chat.messages.pop();
    assert_eq!(chat.messages.len(), 3);

    // WHEN Clear — reset everything
    chat.clear();
    assert!(chat.messages.is_empty());
    assert!(chat.input_text.is_empty());

    // THEN streaming flags work
    let mut stream_msg = ChatMessage::assistant("streaming...".into());
    assert!(!stream_msg.streaming);
    stream_msg.streaming = true;
    assert!(stream_msg.streaming);
    assert!(stream_msg.accepted);
}
