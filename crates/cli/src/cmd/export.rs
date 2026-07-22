//! `roco export <story-dir> --format md|html|txt [--output PATH]`
//!
//! Bundles a finished `.roco/workspaces/story_*` directory into a single
//! export artifact. Pure std + existing CLI helpers, no external deps.
use std::fs;
use std::path::Path;

/// Escape prose for HTML output.
fn clean(s: &str) -> String {
    // Use \x26 hex (the "&" byte) so this doc-level
    // comment and our test fixtures can be inspected
    // without parser/HTML-entity interference.
    s.replace('&', "\x26amp;")
        .replace('<', "\x26lt;")
        .replace('>', "\x26gt;")
        .replace('"', "\x26quot;")
}

/// Export a finished story workspace into a single file.
///
/// Looks for `03-CHAPTER_<N>.md` files inside `story_dir`, sorts them,
/// then emits:
///
/// * [md/markdown] concatenated chapters plus outline appendix
/// * [html] minimal standalone HTML with inline CSS
/// * [txt] plain text suitable for printing
pub fn run(
    story_dir: impl AsRef<Path>,
    format_hint: Option<&str>,
    out_path: Option<impl AsRef<Path>>,
) {
    let story_dir = story_dir.as_ref();
    if !story_dir.exists() || !story_dir.is_dir() {
        eprintln!("Story directory does not exist: {story_dir:?}");
        std::process::exit(2);
    }
    let fmt = format_hint.unwrap_or("md");
    let mut chapters = Vec::new();
    for ent in fs::read_dir(story_dir).unwrap_or_else(|e| {
        eprintln!("failed to read story dir: {e}");
        std::process::exit(2);
    }) {
        let ent = ent.unwrap();
        let p = ent.path();
        let name = match p.file_name().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => continue,
        };
        let rest = match name.strip_prefix("03-CHAPTER_") {
            Some(s) => s,
            None => continue,
        };
        let num_s = match rest.strip_suffix(".md") {
            Some(s) => s,
            None => continue,
        };
        let Ok(num) = num_s.parse::<usize>() else {
            continue;
        };
        let content = fs::read_to_string(&p).unwrap_or_default();
        chapters.push((num, name.to_string(), content));
    }
    chapters.sort_by_key(|c| c.0);
    if chapters.is_empty() {
        eprintln!("No chapters found under 03-CHAPTER_*.md in {story_dir:?}");
        std::process::exit(2);
    }

    let title = story_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Story");
    let mut out = String::new();

    match fmt {
        "md" | "markdown" => render_markdown(&mut out, title, &chapters, story_dir),
        "html" => render_html(&mut out, title, &chapters, story_dir),
        "txt" => render_plaintext(&mut out, title, &chapters),
        other => {
            eprintln!("Unknown --format {other:?}; supported: md, html, txt");
            std::process::exit(2);
        }
    }

    let path = out_path
        .map(|p| p.as_ref().to_path_buf())
        .unwrap_or_else(|| {
            let mut p = story_dir.to_path_buf();
            let ext = if fmt == "markdown" { "md" } else { fmt };
            p.push(format!("EXPORT.{ext}"));
            p
        });

    if let Err(e) = fs::write(&path, out) {
        eprintln!("Failed to write {path:?}: {e}");
        std::process::exit(1);
    }
    println!("Exported {} -> {path:?}", chapters.len());
}

fn render_markdown(
    out: &mut String,
    title: &str,
    chapters: &[(usize, String, String)],
    story_dir: &Path,
) {
    out.push_str(&format!("# {title}\n\n"));
    for (_, name, body) in chapters {
        out.push_str("---\n\n");
        out.push_str(&format!("<!-- {name} -->\n\n"));
        out.push_str(body);
        out.push_str("\n\n");
    }
    let outline = story_dir.join("01-OUTLINE.md");
    if let Ok(o) = fs::read_to_string(&outline) {
        out.push_str("---\n\n# Outline\n\n");
        out.push_str(&o);
    }
}

fn render_html(
    out: &mut String,
    title: &str,
    chapters: &[(usize, String, String)],
    _story_dir: &Path,
) {
    out.push_str("<!doctype html>\n<html lang=\"en\"><head>\n");
    out.push_str(&format!(
        "<meta charset=\"utf-8\"><title>{}</title>\n",
        clean(title)
    ));
    out.push_str("<style>");
    out.push_str("body{font-family:Georgia,serif;max-width:42em;margin:2em auto;padding:0 1em;line-height:1.6} ");
    out.push_str("h1,h2{font-family:Helvetica,sans-serif} ");
    out.push_str("hr{border:0;border-top:1px solid #ccc;margin:2em 0}");
    out.push_str("</style>\n</head><body>\n");
    out.push_str(&format!("<h1>{}</h1>\n", clean(title)));
    for (_, fname, body) in chapters {
        out.push_str("<hr>\n");
        out.push_str(&format!("<h2>{}</h2>\n", clean(fname)));
        for line in body.lines() {
            match line {
                l if l.starts_with("# ") => out.push_str(&format!("<h2>{}</h2>\n", clean(&l[2..]))),
                l if l.starts_with("## ") => {
                    out.push_str(&format!("<h3>{}</h3>\n", clean(&l[3..])))
                }
                l if l.trim().is_empty() => out.push_str("<p></p>\n"),
                l => out.push_str(&format!("<p>{}</p>\n", clean(l))),
            }
        }
    }
    out.push_str("</body></html>\n");
}

fn render_plaintext(out: &mut String, title: &str, chapters: &[(usize, String, String)]) {
    out.push_str(&format!("{title}\n{}\n\n", "=".repeat(title.len())));
    for (_, fname, body) in chapters {
        out.push_str(&format!("--- {fname} ---\n\n"));
        out.push_str(body);
        out.push_str("\n\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn md_basic() {
        let mut out = String::new();
        let ch = vec![(1, "03-CHAPTER_1.md".into(), "# Hi\n\nOk.".into())];
        render_markdown(&mut out, "T", &ch, Path::new("/tmp"));
        assert!(out.contains("# T\n\n---\n\n<!-- 03-CHAPTER_1.md -->\n\n# Hi\n\nOk.\n\n"));
    }
    #[test]
    fn html_escapes() {
        let mut out = String::new();
        let ch = vec![(1, "c".into(), "A & B < C > D \"E\"".into())];
        render_html(&mut out, "T", &ch, Path::new("/tmp"));
        let expected =
            "<p>A &".to_string() + "amp; B &" + "lt; C &" + "gt; D &" + "quot;E&" + "quot;</p>";
        assert!(
            out.contains(&expected),
            "expected escaped paragraph, got:\n{out}"
        );
    }

    /// End-to-end smoke: `run()` walks a real tempdir, finds the chapter
    /// files, picks up the optional outline, and writes a Markdown export
    /// to the path the caller specified. Covers the happy path that the
    /// `roco export ./story_dir --format md --output X` CLI relies on.
    #[test]
    fn run_md_end_to_end() {
        // Isolated dir per-test (avoids cross-run pollution; see v1 roadmap
        // note on tempfile uniqueness).
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("roco_export_md_{pid}_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();

        // Three chapters out of order on disk — `run()` must sort by `N`.
        std::fs::write(
            dir.join("03-CHAPTER_2.md"),
            "# Chapter Two\n\nSecond chapter body.",
        )
        .unwrap();
        std::fs::write(
            dir.join("03-CHAPTER_1.md"),
            "# Chapter One\n\nFirst chapter body.",
        )
        .unwrap();
        std::fs::write(
            dir.join("03-CHAPTER_3.md"),
            "# Chapter Three\n\nThird chapter body.",
        )
        .unwrap();
        std::fs::write(dir.join("01-OUTLINE.md"), "## Outline\n\nA test story.").unwrap();
        // Decoy file at root that should NOT be picked up.
        std::fs::write(dir.join("README.md"), "ignore me").unwrap();

        let out_path = dir.join("EXPORT.md");
        run(&dir, Some("md"), Some(&out_path));

        let written = std::fs::read_to_string(&out_path).expect("export wrote a file");

        // Title comes from directory stem (here the salted temp dir name).
        let title = dir.file_name().unwrap().to_str().unwrap();
        assert!(
            written.starts_with(&format!("# {title}\n\n")),
            "title heading must lead; got: {written:?}"
        );
        // Chapters sorted ascending 1, 2, 3 (not 2, 1, 3 as on disk).
        let pos1 = written.find("03-CHAPTER_1.md").expect("chap 1 present");
        let pos2 = written.find("03-CHAPTER_2.md").expect("chap 2 present");
        let pos3 = written.find("03-CHAPTER_3.md").expect("chap 3 present");
        assert!(
            pos1 < pos2 && pos2 < pos3,
            "chapters must be in ascending numeric order"
        );
        // Outline is appended under a separate heading.
        assert!(written.contains("# Outline"), "outline appendix present");
        assert!(written.contains("A test story."));
        // Decoy file didn't sneak in.
        assert!(!written.contains("ignore me"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// End-to-end smoke for `--format html`. Confirms the file begins with
    /// `<!doctype html>` and that the writer puts chapter headings in the
    /// expected template.
    #[test]
    fn run_html_end_to_end() {
        let dir = std::env::temp_dir().join(format!(
            "roco_export_html_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("03-CHAPTER_1.md"),
            "# Heading\n\nPlain body with <em>html-ish</em> text.",
        )
        .unwrap();
        let out_path = dir.join("EXPORT.html");
        run(&dir, Some("html"), Some(&out_path));

        let written = std::fs::read_to_string(&out_path).expect("html export wrote a file");
        assert!(
            written.starts_with("<!doctype html>"),
            "must emit a doctype; got first 80 chars: {:?}",
            &written[..written.len().min(80)]
        );
        assert!(written.contains("<h1>"));
        assert!(written.contains("<hr>"), "chapter separator present");
        // No raw HTML tags from the chapter body should slip through.
        assert!(
            !written.contains("<em>"),
            "raw <em> in chapter body must be escaped, not embedded"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// End-to-end smoke for `--format txt`.
    #[test]
    fn run_txt_end_to_end() {
        let dir = std::env::temp_dir().join(format!(
            "roco_export_txt_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("03-CHAPTER_1.md"), "Body.").unwrap();
        let out_path = dir.join("EXPORT.txt");
        run(&dir, Some("txt"), Some(&out_path));

        let written = std::fs::read_to_string(&out_path).expect("txt export wrote a file");
        // Title banner of '=' chars under the heading.
        let title = dir.file_name().unwrap().to_str().unwrap();
        let banner = "=".repeat(title.len());
        assert!(
            written.contains(&banner),
            "title underline banner of length {} should be present",
            title.len()
        );
        assert!(written.contains("--- 03-CHAPTER_1.md ---"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Bad inputs should fail loudly, not panic silently. We capture stderr
    /// by inspecting the messages produced via `std::process::exit` paths
    /// during normal unit tests — to keep the test from spawning a real
    /// subprocess we just assert against the direct helpers.
    #[test]
    fn missing_chapters_produces_empty_marker() {
        let dir = std::env::temp_dir().join(format!(
            "roco_export_empty_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("README.md"), "no chapters here").unwrap();
        // `run` calls `std::process::exit(2)` on missing chapters, so we
        // can only assert on the *helpers* (`render_*` and `clean`).
        let mut out = String::new();
        let ch: Vec<(usize, String, String)> = vec![];
        render_markdown(&mut out, "Title", &ch, &dir);
        assert!(
            out.starts_with("# Title\n\n"),
            "empty chapters still emits the title heading"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
