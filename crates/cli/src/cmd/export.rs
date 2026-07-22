//! `roco export <story-dir> --format md|html|txt [--output PATH]`
//!
//! Bundles a finished `.roco/workspaces/story_*` directory into a single
//! export artifact. Pure std + existing CLI helpers, no external deps.
use std::fs;
use std::path::{Path, PathBuf};

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
    let fmt = match format_hint {
        Some(s) => s,
        None => "md",
    };
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
        let expected = "<p>A &".to_string()
            + "amp; B &"
            + "lt; C &"
            + "gt; D &"
            + "quot;E&"
            + "quot;</p>";
        assert!(
            out.contains(&expected),
            "expected escaped paragraph, got:\n{out}"
        );
    }
}
