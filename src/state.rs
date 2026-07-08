use crate::config::STATE_FILE;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Committed sidecar tracking what noagents owns inside files that cannot
/// carry comment markers (JSON/TOML), plus which files it created.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct State {
    pub version: u32,
    #[serde(default)]
    pub targets: BTreeMap<String, StateEntry>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct StateEntry {
    /// Exact entries we inserted into a structured target (JSON/TOML).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
    /// True when noagents created the target file from scratch.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub created_file: bool,
    /// True when noagents created the target's parent directory (e.g. `.zed/`,
    /// `.trae/`). Only such directories are removed on `remove`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub created_dir: bool,
}

impl State {
    pub fn load(root: &Path) -> Result<State> {
        let path = root.join(STATE_FILE);
        if !path.is_file() {
            return Ok(State {
                version: 1,
                targets: BTreeMap::new(),
            });
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let state: State = serde_json::from_str(&text)
            .with_context(|| format!("{} is corrupted; delete it and rerun", path.display()))?;
        Ok(state)
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let path = root.join(STATE_FILE);
        if self.targets.is_empty() {
            if path.is_file() {
                std::fs::remove_file(&path)
                    .with_context(|| format!("cannot remove {}", path.display()))?;
            }
            return Ok(());
        }
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        std::fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))
    }

    pub fn entry(&self, id: &str) -> StateEntry {
        self.targets.get(id).cloned().unwrap_or_default()
    }

    pub fn set_entry(&mut self, id: &str, entry: StateEntry) {
        if entry.entries.is_empty() && !entry.created_file && !entry.created_dir {
            self.targets.remove(id);
        } else {
            self.targets.insert(id.to_string(), entry);
        }
    }
}
