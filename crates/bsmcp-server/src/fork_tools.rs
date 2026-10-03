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

// --- briefing(task) ---

/// Rows of the first markdown table that has a "Task" and a "Read first"
/// column: (task cell, read-first cell).
pub fn task_table(md: &str) -> Vec<(String, String)> {
    let split = |line: &str| -> Vec<String> {
        line.trim()
            .trim_start_matches('|')
            .trim_end_matches('|')
            .split('|')
            .map(|c| c.trim().to_string())
            .collect()
    };
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with('|') {
            let header = split(lines[i]);
            let col = |name: &str| header.iter().position(|h| h.eq_ignore_ascii_case(name));
            let mut j = i + 1;
            if let (Some(task_col), Some(read_col)) = (col("Task"), col("Read first")) {
                let mut rows = Vec::new();
                while j < lines.len() && lines[j].trim_start().starts_with('|') {
                    let cells = split(lines[j]);
                    let is_separator = cells.iter().all(|c| c.chars().all(|ch| matches!(ch, '-' | ':' | ' ')));
                    if !is_separator {
                        if let (Some(t), Some(r)) = (cells.get(task_col), cells.get(read_col)) {
                            rows.push((t.clone(), r.clone()));
                        }
                    }
                    j += 1;
                }
                return rows;
            }
            while j < lines.len() && lines[j].trim_start().starts_with('|') {
                j += 1;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    Vec::new()
}

/// First row whose task cell contains every word of `task` (case-insensitive).
pub fn match_task<'a>(rows: &'a [(String, String)], task: &str) -> Option<&'a (String, String)> {
    let words: Vec<String> = task.split_whitespace().map(|w| w.to_lowercase()).collect();
    if words.is_empty() {
        return None;
    }
    rows.iter().find(|(t, _)| {
        let t = t.to_lowercase();
        words.iter().all(|w| t.contains(w.as_str()))
    })
}

/// Page ids written in brackets, e.g. "Resume facts (49)" -> 49. In order, no duplicates.
pub fn bracket_ids(cell: &str) -> Vec<i64> {
    let mut ids = Vec::new();
    let mut rest = cell;
    while let Some(open) = rest.find('(') {
        rest = &rest[open + 1..];
        if let Some(close) = rest.find(')') {
            if let Ok(id) = rest[..close].trim().parse::<i64>() {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }
    ids
}

pub fn briefing_page_id() -> Result<i64, String> {
    std::env::var("BSMCP_BRIEFING_PAGE_ID")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .ok_or_else(|| "briefing is not configured: set BSMCP_BRIEFING_PAGE_ID to the start page id".to_string())
}

/// The start page plus the "Read first" pages of the matching task row, each
/// once, as markdown with a small header per page.
pub async fn briefing(
    client: &bsmcp_common::bookstack::BookStackClient,
    start_id: i64,
    task: &str,
) -> Result<String, String> {
    use bsmcp_common::bookstack::ExportFormat;
    let start = client.export_page(start_id, ExportFormat::Markdown).await?;
    let rows = task_table(&start);
    let mut out = format!("=== page {start_id} (start page) ===\n{}\n", start.trim_end());

    let Some((task_cell, read_cell)) = match_task(&rows, task) else {
        let names: Vec<&str> = rows.iter().map(|(t, _)| t.as_str()).collect();
        out.push_str(&format!(
            "\n=== no task row matched '{task}' ===\nTasks: {}\n",
            names.join(" | ")
        ));
        return Ok(out);
    };

    let ids: Vec<i64> = bracket_ids(read_cell).into_iter().filter(|id| *id != start_id).collect();
    let pages = futures::future::join_all(
        ids.iter().map(|id| client.export_page(*id, ExportFormat::Markdown)),
    )
    .await;
    out.push_str(&format!("\n=== task: {task_cell} | read first: {ids:?} ===\n"));
    for (id, page) in ids.iter().zip(pages) {
        match page {
            Ok(md) => out.push_str(&format!("\n=== page {id} ===\n{}\n", md.trim_end())),
            Err(e) => out.push_str(&format!("\n=== page {id}: error: {e} ===\n")),
        }
    }
    Ok(out)
}

// --- compact semantic_search ---

const COMPACT_SEARCH_HINT: &str =
    "Previews only (chunks of ~200 chars; … = cut). Read a page with export_page (format markdown) or one part with export_section.";

/// Keep page_id, page_name and each chunk's heading_path + content. Drops
/// scores, scoring breakdown, stats and dates. `stats.unknown_scopes`, if
/// any, moves to the top level so a typo in a scope name still shows.
pub fn compact_search(payload: &serde_json::Value) -> serde_json::Value {
    use serde_json::{json, Value};
    let results: Vec<Value> = payload["results"]
        .as_array()
        .map(|rs| {
            rs.iter()
                .map(|r| {
                    let chunks: Vec<Value> = r["chunks"]
                        .as_array()
                        .map(|cs| {
                            cs.iter()
                                .map(|c| json!({ "heading_path": c["heading_path"], "content": c["content"] }))
                                .collect()
                        })
                        .unwrap_or_default();
                    json!({ "page_id": r["page_id"], "page_name": r["page_name"], "chunks": chunks })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut out = json!({ "hint": COMPACT_SEARCH_HINT, "results": results });
    if let Some(unknown) = payload["stats"].get("unknown_scopes") {
        out["unknown_scopes"] = unknown.clone();
    }
    out
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

    const START: &str = "# Start here\n\n## Task table\n\n| Task | Read first | Write back |\n|---|---|---|\n| Cover letter, application answers or email for a job | [Application rules](https://x/page/a) (116) · [About me](https://x/b) (46) · [Resume facts](https://x/c) (49). Follow the steps in 116 | 118 |\n| Interview prep | Overview (48) · About me (46) | 48 |\n| Anything else | nothing more | ask |\n";

    #[test]
    fn task_table_rows() {
        let rows = task_table(START);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].0, "Interview prep");
    }

    #[test]
    fn keyword_match_all_words_any_case() {
        let rows = task_table(START);
        assert_eq!(match_task(&rows, "cover letter").unwrap().0.split(',').next(), Some("Cover letter"));
        assert_eq!(match_task(&rows, "INTERVIEW").unwrap().0, "Interview prep");
        assert!(match_task(&rows, "letter interview").is_none());
        assert!(match_task(&rows, "  ").is_none());
    }

    #[test]
    fn ids_only_from_brackets_in_order() {
        let rows = task_table(START);
        let row = match_task(&rows, "cover letter").unwrap();
        assert_eq!(bracket_ids(&row.1), vec![116, 46, 49]);
        assert_eq!(bracket_ids("a (1) b (x) c (1) (https://x) (2)"), vec![1, 2]);
    }

    #[test]
    fn table_without_columns_is_skipped() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |\n\n| Task | Read first |\n|--|--|\n| T | (5) |\n";
        assert_eq!(task_table(md), vec![("T".to_string(), "(5)".to_string())]);
    }

    #[test]
    fn compact_search_keeps_only_text_fields() {
        let full = serde_json::json!({
            "hint": "long hint",
            "results": [{
                "book_id": 36, "page_id": 113, "page_name": "Token cost", "score": -1.3,
                "scoring": {"vector": 0.6}, "updated_at": "x",
                "chunks": [{"content": "abc…", "heading_path": "H", "score": 0.6, "truncated": true}]
            }],
            "stats": {"total_chunks": 324, "unknown_scopes": ["typo"]}
        });
        let c = compact_search(&full);
        assert_eq!(c["results"][0], serde_json::json!({"page_id": 113, "page_name": "Token cost", "chunks": [{"heading_path": "H", "content": "abc…"}]}));
        assert_eq!(c["unknown_scopes"], serde_json::json!(["typo"]));
        assert!(c.get("stats").is_none());
        assert!(serde_json::to_string(&c).unwrap().len() < serde_json::to_string_pretty(&full).unwrap().len());
    }

    #[test]
    fn hashtag_word_is_not_a_heading() {
        assert_eq!(heading_level("#tag"), None);
        assert_eq!(heading_level("## Ok"), Some(2));
    }
}
