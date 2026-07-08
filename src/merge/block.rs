use anyhow::{Result, bail};

pub const START: &str = "# >>> noagents managed — DO NOT EDIT; run `noagents generate` >>>";
pub const END: &str = "# <<< noagents managed <<<";

fn is_marker(line: &str, marker: &str) -> bool {
    line.trim_end_matches('\r').trim() == marker
}

/// Byte spans of each line including its terminator.
fn line_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            spans.push((start, i + 1));
            start = i + 1;
        }
    }
    if start < bytes.len() {
        spans.push((start, bytes.len()));
    }
    spans
}

fn find_markers(text: &str) -> Result<Option<(usize, usize)>> {
    let spans = line_spans(text);
    let mut start_span = None;
    let mut end_span = None;
    for &(s, e) in &spans {
        let line = &text[s..e];
        if start_span.is_none() && is_marker(line, START) {
            start_span = Some((s, e));
        } else if start_span.is_some() && end_span.is_none() && is_marker(line, END) {
            end_span = Some((s, e));
        } else if start_span.is_none() && is_marker(line, END) {
            bail!("managed block corrupted: end marker appears before start marker");
        }
    }
    match (start_span, end_span) {
        (Some((s, _)), Some((_, e))) => Ok(Some((s, e))),
        (None, None) => Ok(None),
        _ => bail!(
            "managed block corrupted: found only one of the two markers; \
             fix the file manually or delete both marker lines"
        ),
    }
}

fn render_block(body_lines: &[String]) -> String {
    let mut s = String::new();
    s.push_str(START);
    s.push('\n');
    for line in body_lines {
        s.push_str(line);
        s.push('\n');
    }
    s.push_str(END);
    s.push('\n');
    s
}

/// Inserts or replaces the managed block. Content outside the block is
/// preserved byte-for-byte.
pub fn apply(existing: Option<&str>, body_lines: &[String]) -> Result<String> {
    let block = render_block(body_lines);
    let Some(text) = existing else {
        return Ok(block);
    };
    match find_markers(text)? {
        Some((start, end)) => {
            let mut out = String::with_capacity(text.len());
            out.push_str(&text[..start]);
            out.push_str(&block);
            out.push_str(&text[end..]);
            Ok(out)
        }
        None => {
            let mut out = text.to_string();
            if !out.is_empty() {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push('\n');
            }
            out.push_str(&block);
            Ok(out)
        }
    }
}

/// Strips the managed block. Returns `None` when nothing but whitespace
/// remains (caller decides whether to delete the file), `Some(unchanged)`
/// when no block is present.
pub fn remove(existing: &str) -> Result<Option<String>> {
    let Some((start, mut end)) = find_markers(existing)? else {
        return Ok(Some(existing.to_string()));
    };
    let mut head = &existing[..start];
    // Collapse the separator blank line `apply` added before the block.
    if head.ends_with("\n\n") {
        head = &head[..head.len() - 1];
    }
    // Skip a blank line directly after the block.
    let tail_full = &existing[end..];
    if tail_full.starts_with('\n') && !head.is_empty() {
        end += 1;
    }
    let mut out = String::with_capacity(existing.len());
    out.push_str(head);
    out.push_str(&existing[end..]);
    if out.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn creates_block_in_empty() {
        let out = apply(None, &lines(&[".env", "secrets/"])).unwrap();
        assert_eq!(out, format!("{START}\n.env\nsecrets/\n{END}\n"));
    }

    #[test]
    fn appends_to_existing_content() {
        let out = apply(Some("node_modules\n"), &lines(&[".env"])).unwrap();
        assert_eq!(out, format!("node_modules\n\n{START}\n.env\n{END}\n"));
    }

    #[test]
    fn appends_adds_missing_trailing_newline() {
        let out = apply(Some("node_modules"), &lines(&[".env"])).unwrap();
        assert_eq!(out, format!("node_modules\n\n{START}\n.env\n{END}\n"));
    }

    #[test]
    fn replaces_existing_block() {
        let first = apply(Some("user\n"), &lines(&["old"])).unwrap();
        let second = apply(Some(&first), &lines(&["new1", "new2"])).unwrap();
        assert_eq!(second, format!("user\n\n{START}\nnew1\nnew2\n{END}\n"));
    }

    #[test]
    fn idempotent() {
        let once = apply(Some("user\n"), &lines(&[".env"])).unwrap();
        let twice = apply(Some(&once), &lines(&[".env"])).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn preserves_content_after_block() {
        let text = format!("before\n\n{START}\nold\n{END}\nafter\n");
        let out = apply(Some(&text), &lines(&["new"])).unwrap();
        assert_eq!(out, format!("before\n\n{START}\nnew\n{END}\nafter\n"));
    }

    #[test]
    fn one_marker_errors() {
        let text = format!("{START}\n.env\n");
        assert!(apply(Some(&text), &lines(&[".env"])).is_err());
        assert!(remove(&text).is_err());
        let text = format!(".env\n{END}\n");
        assert!(apply(Some(&text), &lines(&[".env"])).is_err());
    }

    #[test]
    fn tolerates_crlf_markers() {
        let text = format!("user\r\n\r\n{START}\r\nold\r\n{END}\r\n");
        let out = apply(Some(&text), &lines(&["new"])).unwrap();
        assert!(out.contains("new\n"));
        assert!(out.starts_with("user\r\n"));
    }

    #[test]
    fn remove_restores_original() {
        let original = "node_modules\n*.log\n";
        let with_block = apply(Some(original), &lines(&[".env"])).unwrap();
        let restored = remove(&with_block).unwrap().unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn remove_returns_none_when_only_block() {
        let only_block = apply(None, &lines(&[".env"])).unwrap();
        assert!(remove(&only_block).unwrap().is_none());
    }

    #[test]
    fn remove_without_block_is_noop() {
        assert_eq!(remove("plain\n").unwrap().unwrap(), "plain\n");
    }
}
