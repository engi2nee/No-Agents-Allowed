use anyhow::{Context, Result, anyhow};
use toml_edit::{Array, DocumentMut, Item, Value};

pub struct Merged {
    pub content: String,
    pub owned: Vec<String>,
}

fn as_str(v: &Value) -> Option<&str> {
    v.as_str()
}

/// Merges string entries into `[table] key = [...]`, preserving user
/// comments and formatting via toml_edit.
pub fn apply(
    existing: Option<&str>,
    table: &str,
    key: &str,
    old_owned: &[String],
    desired: &[String],
) -> Result<Merged> {
    let mut doc: DocumentMut = existing.unwrap_or("").parse().context("invalid TOML")?;
    let tbl = doc
        .entry(table)
        .or_insert(toml_edit::table())
        .as_table_mut()
        .ok_or_else(|| anyhow!("`{table}` exists but is not a TOML table"))?;
    let arr = tbl
        .entry(key)
        .or_insert(toml_edit::value(Array::new()))
        .as_array_mut()
        .ok_or_else(|| anyhow!("`{table}.{key}` exists but is not an array"))?;

    for o in old_owned {
        let pos = arr.iter().position(|v| as_str(v) == Some(o));
        if let Some(pos) = pos {
            arr.remove(pos);
        }
    }
    let mut owned = Vec::new();
    for d in desired {
        if arr.iter().any(|v| as_str(v) == Some(d)) {
            continue;
        }
        arr.push(d.as_str());
        owned.push(d.clone());
    }
    Ok(Merged {
        content: doc.to_string(),
        owned,
    })
}

/// Removes owned entries; prunes empty array/table. Returns `None` when the
/// document becomes empty.
pub fn remove(existing: &str, table: &str, key: &str, owned: &[String]) -> Result<Option<String>> {
    let mut doc: DocumentMut = existing.parse().context("invalid TOML")?;
    let Some(tbl) = doc.get_mut(table).and_then(Item::as_table_mut) else {
        return Ok(Some(existing.to_string()));
    };
    let Some(arr) = tbl.get_mut(key).and_then(Item::as_array_mut) else {
        return Ok(Some(existing.to_string()));
    };
    for o in owned {
        let pos = arr.iter().position(|v| as_str(v) == Some(o));
        if let Some(pos) = pos {
            arr.remove(pos);
        }
    }
    if arr.is_empty() {
        tbl.remove(key);
    }
    if tbl.is_empty() {
        doc.remove(table);
    }
    let out = doc.to_string();
    if out.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn creates_from_scratch() {
        let m = apply(None, "file_filters", "exclude", &[], &strs(&["**/.env"])).unwrap();
        assert!(m.content.contains("[file_filters]"));
        assert!(m.content.contains("**/.env"));
        assert_eq!(m.owned, strs(&["**/.env"]));
    }

    #[test]
    fn preserves_comments_and_formatting() {
        let existing = "# my config\n[file_filters]\n# keep this\ninclude = [\"src/**\"]\n";
        let m = apply(
            Some(existing),
            "file_filters",
            "exclude",
            &[],
            &strs(&["**/.env"]),
        )
        .unwrap();
        assert!(m.content.contains("# my config"));
        assert!(m.content.contains("# keep this"));
        assert!(m.content.contains("include = [\"src/**\"]"));
    }

    #[test]
    fn stale_entries_replaced_and_user_kept() {
        let existing = "[file_filters]\nexclude = [\"user-glob\"]\n";
        let m1 = apply(
            Some(existing),
            "file_filters",
            "exclude",
            &[],
            &strs(&["old"]),
        )
        .unwrap();
        let m2 = apply(
            Some(&m1.content),
            "file_filters",
            "exclude",
            &m1.owned,
            &strs(&["new"]),
        )
        .unwrap();
        assert!(m2.content.contains("user-glob"));
        assert!(m2.content.contains("new"));
        assert!(!m2.content.contains("old"));
    }

    #[test]
    fn remove_round_trip() {
        let existing = "# top\n[other]\nx = 1\n";
        let m = apply(
            Some(existing),
            "file_filters",
            "exclude",
            &[],
            &strs(&["**/.env"]),
        )
        .unwrap();
        let out = remove(&m.content, "file_filters", "exclude", &m.owned)
            .unwrap()
            .unwrap();
        assert!(out.contains("# top"));
        assert!(out.contains("[other]"));
        assert!(!out.contains("file_filters"));
    }

    #[test]
    fn remove_everything_returns_none() {
        let m = apply(None, "file_filters", "exclude", &[], &strs(&["**/.env"])).unwrap();
        assert!(
            remove(&m.content, "file_filters", "exclude", &m.owned)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn malformed_toml_errors() {
        assert!(apply(Some("not [ toml"), "t", "k", &[], &[]).is_err());
    }
}
