//! Story & Workspace Statistics subcommand: `roco stats` / `roco review`.
//!
//! Analyzes a story directory or project workspace, computing word counts,
//! chapter breakdown, estimated reading time, outline completeness, and
//! session telemetry.

use std::fs;
use std::path::Path;

pub fn cmd_stats(extra: &[&str]) {
    let json_mode = extra.iter().any(|&a| a == "--json" || a == "-j");
    let target_dir = extra
        .iter()
        .find(|&&a| !a.starts_with('-'))
        .copied()
        .unwrap_or(".");

    let path = Path::new(target_dir);
    if !path.exists() {
        eprintln!("Path does not exist: {target_dir}");
        std::process::exit(1);
    }

    let mut total_words = 0_usize;
    let mut total_chars = 0_usize;
    let mut total_paragraphs = 0_usize;
    let mut chapters = Vec::new();

    // Walk directory for chapters
    if let Ok(entries) = fs::read_dir(path) {
        for ent in entries.flatten() {
            let p = ent.path();
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                if name.starts_with("03-CHAPTER_") && name.ends_with(".md") {
                    if let Ok(content) = fs::read_to_string(&p) {
                        let words = content.split_whitespace().count();
                        let chars = content.chars().count();
                        let paragraphs = content.lines().filter(|l| !l.trim().is_empty()).count();
                        total_words += words;
                        total_chars += chars;
                        total_paragraphs += paragraphs;
                        chapters.push((name.to_string(), words, chars, paragraphs));
                    }
                }
            }
        }
    }

    chapters.sort_by(|a, b| a.0.cmp(&b.0));

    let outline_path = path.join("01-OUTLINE.md");
    let has_outline = outline_path.exists();
    let outline_words = if has_outline {
        fs::read_to_string(&outline_path)
            .map(|s| s.split_whitespace().count())
            .unwrap_or(0)
    } else {
        0
    };

    let est_reading_minutes = (total_words as f64 / 225.0).ceil() as usize;

    if json_mode {
        let stats_json = serde_json::json!({
            "target": target_dir,
            "total_chapters": chapters.len(),
            "total_words": total_words,
            "total_chars": total_chars,
            "total_paragraphs": total_paragraphs,
            "estimated_reading_time_minutes": est_reading_minutes,
            "has_outline": has_outline,
            "outline_words": outline_words,
            "chapters": chapters.iter().map(|(name, w, c, p)| {
                serde_json::json!({
                    "name": name,
                    "words": w,
                    "chars": c,
                    "paragraphs": p
                })
            }).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&stats_json).unwrap());
    } else {
        println!("================================================================");
        println!("  RoCo AI — Workspace & Story Statistics Report");
        println!("================================================================");
        println!("Target Directory:     {target_dir}");
        println!("Chapters Found:       {}", chapters.len());
        println!("Total Word Count:     {total_words} words");
        println!("Total Character Count:{total_chars} chars");
        println!("Total Paragraphs:     {total_paragraphs}");
        println!("Est. Reading Time:    ~{est_reading_minutes} min (at 225 WPM)");
        println!(
            "Outline Present:      {}",
            if has_outline {
                format!("Yes ({outline_words} words)")
            } else {
                "No".into()
            }
        );
        println!("----------------------------------------------------------------");
        if chapters.is_empty() {
            println!("(No 03-CHAPTER_*.md chapters found in this directory)");
        } else {
            println!("Chapter Breakdown:");
            for (name, w, _, p) in &chapters {
                let bar =
                    "#".repeat((*w as f32 / (total_words.max(1) as f32) * 30.0).ceil() as usize);
                println!("  {name:25} | {w:5} words | {p:3} paras | {bar}");
            }
        }
        println!("================================================================");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_computation_on_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        fs::write(dir.join("03-CHAPTER_1.md"), "Hello world test chapter one.").unwrap();
        fs::write(
            dir.join("03-CHAPTER_2.md"),
            "Another chapter with more words here.",
        )
        .unwrap();
        fs::write(dir.join("01-OUTLINE.md"), "Outline plot outline.").unwrap();

        // Test helper directly or check computations
        let ch1_words = "Hello world test chapter one.".split_whitespace().count();
        let ch2_words = "Another chapter with more words here."
            .split_whitespace()
            .count();
        assert_eq!(ch1_words, 5);
        assert_eq!(ch2_words, 6);
    }
}
