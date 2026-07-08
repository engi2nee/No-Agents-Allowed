use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map, Value, json};

pub struct Merged {
    pub content: String,
    /// Entries we actually inserted (pre-existing user entries are never claimed).
    pub owned: Vec<String>,
}

fn parse(existing: Option<&str>) -> Result<Value> {
    let root = match existing {
        Some(t) if !t.trim().is_empty() => {
            serde_json::from_str(t).context("invalid JSON (JSONC comments are not supported)")?
        }
        _ => json!({}),
    };
    if !root.is_object() {
        bail!("top-level JSON value is not an object");
    }
    Ok(root)
}

fn navigate<'a>(
    root: &'a mut Value,
    parents: &[&str],
    create: bool,
) -> Result<Option<&'a mut Map<String, Value>>> {
    let mut cur = root;
    for seg in parents {
        let obj = cur
            .as_object_mut()
            .ok_or_else(|| anyhow!("`{seg}` parent is not a JSON object"))?;
        if !obj.contains_key(*seg) {
            if !create {
                return Ok(None);
            }
            obj.insert(seg.to_string(), json!({}));
        }
        cur = obj.get_mut(*seg).unwrap();
        if !cur.is_object() {
            bail!("`{seg}` exists but is not a JSON object");
        }
    }
    Ok(cur.as_object_mut())
}

fn to_pretty(root: &Value) -> Result<String> {
    let mut text = serde_json::to_string_pretty(root)?;
    text.push('\n');
    Ok(text)
}

/// Merges string entries into the array at `pointer`: removes exactly the
/// previously-owned entries, then appends `desired` entries not already
/// present. Sibling keys and key order are preserved.
pub fn apply(
    existing: Option<&str>,
    pointer: &[&str],
    old_owned: &[String],
    desired: &[String],
) -> Result<Merged> {
    let mut root = parse(existing)?;
    let (last, parents) = pointer.split_last().expect("pointer must be non-empty");
    let obj = navigate(&mut root, parents, true)?.expect("created");
    if !obj.contains_key(*last) {
        obj.insert(last.to_string(), json!([]));
    }
    let arr = obj
        .get_mut(*last)
        .unwrap()
        .as_array_mut()
        .ok_or_else(|| anyhow!("`{last}` exists but is not a JSON array"))?;

    for o in old_owned {
        if let Some(pos) = arr.iter().position(|v| v.as_str() == Some(o)) {
            arr.remove(pos);
        }
    }
    let mut owned = Vec::new();
    for d in desired {
        if arr.iter().any(|v| v.as_str() == Some(d)) {
            continue;
        }
        arr.push(json!(d));
        owned.push(d.clone());
    }
    Ok(Merged {
        content: to_pretty(&root)?,
        owned,
    })
}

/// Removes owned entries; prunes the array/parent objects if they end up
/// empty. Returns `None` when the whole document becomes an empty object.
pub fn remove(existing: &str, pointer: &[&str], owned: &[String]) -> Result<Option<String>> {
    let mut root = parse(Some(existing))?;
    let (last, parents) = pointer.split_last().expect("pointer must be non-empty");
    {
        let Some(obj) = navigate(&mut root, parents, false)? else {
            return Ok(Some(existing.to_string()));
        };
        let Some(arr) = obj.get_mut(*last).and_then(|v| v.as_array_mut()) else {
            return Ok(Some(existing.to_string()));
        };
        for o in owned {
            if let Some(pos) = arr.iter().position(|v| v.as_str() == Some(o)) {
                arr.remove(pos);
            }
        }
        if arr.is_empty() {
            obj.remove(*last);
        }
    }
    // Prune now-empty parent objects, innermost first.
    for depth in (0..parents.len()).rev() {
        let Some(obj) = navigate(&mut root, &parents[..depth], false)? else {
            break;
        };
        let key = parents[depth];
        if obj
            .get(key)
            .and_then(|v| v.as_object())
            .is_some_and(|m| m.is_empty())
        {
            obj.remove(key);
        }
    }
    if root.as_object().is_some_and(|m| m.is_empty()) {
        return Ok(None);
    }
    Ok(Some(to_pretty(&root)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PTR: &[&str] = &["permissions", "deny"];

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn creates_from_scratch() {
        let m = apply(None, PTR, &[], &strs(&["Read(./.env)"])).unwrap();
        let v: Value = serde_json::from_str(&m.content).unwrap();
        assert_eq!(v["permissions"]["deny"][0], "Read(./.env)");
        assert_eq!(m.owned, strs(&["Read(./.env)"]));
    }

    #[test]
    fn preserves_siblings_and_order() {
        let existing =
            r#"{"model": "opus", "permissions": {"allow": ["Bash(ls:*)"]}, "env": {"FOO": "1"}}"#;
        let m = apply(Some(existing), PTR, &[], &strs(&["Read(./.env)"])).unwrap();
        let v: Value = serde_json::from_str(&m.content).unwrap();
        assert_eq!(v["model"], "opus");
        assert_eq!(v["permissions"]["allow"][0], "Bash(ls:*)");
        assert_eq!(v["env"]["FOO"], "1");
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["model", "permissions", "env"]);
    }

    #[test]
    fn removes_stale_owned_entries() {
        let m1 = apply(None, PTR, &[], &strs(&["Read(./old)"])).unwrap();
        let m2 = apply(Some(&m1.content), PTR, &m1.owned, &strs(&["Read(./new)"])).unwrap();
        let v: Value = serde_json::from_str(&m2.content).unwrap();
        let deny = v["permissions"]["deny"].as_array().unwrap();
        assert_eq!(deny.len(), 1);
        assert_eq!(deny[0], "Read(./new)");
    }

    #[test]
    fn never_claims_user_entries() {
        let existing = r#"{"permissions": {"deny": ["Read(./.env)"]}}"#;
        let m = apply(
            Some(existing),
            PTR,
            &[],
            &strs(&["Read(./.env)", "Read(./x)"]),
        )
        .unwrap();
        // .env already user-owned: not claimed, not duplicated
        assert_eq!(m.owned, strs(&["Read(./x)"]));
        let v: Value = serde_json::from_str(&m.content).unwrap();
        assert_eq!(v["permissions"]["deny"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn missing_state_degrades_to_add_only() {
        let existing = r#"{"permissions": {"deny": ["Read(./mystery)"]}}"#;
        let m = apply(Some(existing), PTR, &[], &strs(&["Read(./x)"])).unwrap();
        let v: Value = serde_json::from_str(&m.content).unwrap();
        let deny = v["permissions"]["deny"].as_array().unwrap();
        assert_eq!(deny.len(), 2, "unknown entries untouched");
    }

    #[test]
    fn malformed_json_errors() {
        assert!(apply(Some("{ not json"), PTR, &[], &[]).is_err());
        assert!(apply(Some(r#"{"a": 1} // comment"#), PTR, &[], &[]).is_err());
    }

    #[test]
    fn remove_prunes_empty_structures() {
        let m = apply(None, PTR, &[], &strs(&["Read(./.env)"])).unwrap();
        assert!(remove(&m.content, PTR, &m.owned).unwrap().is_none());
    }

    #[test]
    fn remove_keeps_user_content() {
        let existing = r#"{"model": "opus", "permissions": {"deny": ["Read(./mine)"]}}"#;
        let m = apply(Some(existing), PTR, &[], &strs(&["Read(./x)"])).unwrap();
        let out = remove(&m.content, PTR, &m.owned).unwrap().unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["model"], "opus");
        assert_eq!(v["permissions"]["deny"][0], "Read(./mine)");
    }

    #[test]
    fn remove_on_untouched_file_is_noop() {
        let existing = "{\n  \"model\": \"opus\"\n}\n";
        let out = remove(existing, PTR, &strs(&["Read(./x)"]))
            .unwrap()
            .unwrap();
        assert_eq!(out, existing);
    }
}
