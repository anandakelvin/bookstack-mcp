//! Fork-only tools that cut tokens per session (github.com/anandakelvin/bookstack-mcp).
//! Pure text helpers live here so mcp.rs only carries the dispatch lines.

/// Level (1-6) of an ATX markdown heading line, else None.
fn heading_level(line: &str) -> Option<usize> {
    let t = line.trim_start();
    let n = t.chars().take_while(|c| *c == '#').count();
    let rest = &t[n..];
    if (1..=6).contains(&n) && (rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')) {
        Some(n)
    } else {
        None
    }
}

fn heading_text(line: &str) -> &str {
    line.trim()
        .trim_start_matches('#')
        .trim()
        .trim_end_matches('#')
        .trim()
}

/// (line index, level) of every heading outside fenced code blocks.
fn headings(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut in_fence = false;
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            if let Some(level) = heading_level(line) {
                out.push((i, level));
            }
        }
    }
    out
}

/// Line range `[start, end)` of the section under `heading`: the heading line
/// up to the next heading of the same or higher level. Case-insensitive; the
/// caller may pass "## Name" or just "Name".
pub fn section_range(lines: &[&str], heading: &str) -> Option<(usize, usize)> {
    let want = heading.trim().trim_start_matches('#').trim();
    let hs = headings(lines);
    let pos = hs
        .iter()
        .position(|(i, _)| heading_text(lines[*i]).eq_ignore_ascii_case(want))?;
    let (start, level) = hs[pos];
    let end = hs[pos + 1..]
        .iter()
        .find(|(_, l)| *l <= level)
        .map(|(i, _)| *i)
        .unwrap_or(lines.len());
    Some((start, end))
}

/// Short list of a page's headings, for "heading not found" errors.
pub fn heading_list(md: &str) -> String {
    let lines: Vec<&str> = md.lines().collect();
    headings(&lines)
        .iter()
        .map(|(i, _)| lines[*i].trim())
        .collect::<Vec<_>>()
        .join(" | ")
}

/// The markdown of one section (heading line included), or an error that
/// lists the page's headings so the caller can retry without reading the page.
pub fn export_section(md: &str, heading: &str, page_id: i64) -> Result<String, String> {
    let lines: Vec<&str> = md.lines().collect();
    match section_range(&lines, heading) {
        Some((start, end)) => Ok(lines[start..end].join("\n").trim_end().to_string()),
        None => Err(format!(
            "Heading '{heading}' not found in page {page_id}. Headings: {}",
            heading_list(md)
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "# Title\n\nIntro\n\n## Before you work\n\n1. Read\n\n### Detail\n\ntext\n\n## Always\n\n- rule\n\n```sh\n# not a heading\n```\n\n## Task table\n\n| a | b |\n";

    #[test]
    fn section_stops_at_same_level() {
        let s = export_section(PAGE, "Before you work", 44).unwrap();
        assert!(s.starts_with("## Before you work"));
        assert!(s.contains("### Detail"));
        assert!(!s.contains("Always"));
    }

    #[test]
    fn heading_with_hashes_and_case() {
        let s = export_section(PAGE, "### detail", 44).unwrap();
        assert_eq!(s, "### Detail\n\ntext");
    }

    #[test]
    fn code_fence_comment_is_not_a_heading() {
        let s = export_section(PAGE, "Always", 44).unwrap();
        assert!(s.contains("# not a heading"));
        assert!(!s.contains("Task table"));
        assert!(export_section(PAGE, "not a heading", 44).is_err());
    }

    #[test]
    fn last_section_runs_to_end() {
        let s = export_section(PAGE, "Task table", 44).unwrap();
        assert_eq!(s, "## Task table\n\n| a | b |");
    }

    #[test]
    fn missing_heading_lists_headings() {
        let e = export_section(PAGE, "Nope", 44).unwrap_err();
        assert!(e.contains("## Always"));
        assert!(e.contains("page 44"));
    }

    #[test]
    fn hashtag_word_is_not_a_heading() {
        assert_eq!(heading_level("#tag"), None);
        assert_eq!(heading_level("## Ok"), Some(2));
    }
}
