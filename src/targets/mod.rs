use crate::config::usage_err;
use anyhow::Result;

/// How pattern lines are rendered for a JSON-array target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonRender {
    /// Claude Code `Read(glob)` deny entries.
    ClaudeDeny,
    /// Plain globs (Zed `private_files`).
    Globs,
}

#[derive(Debug, Clone, Copy)]
pub enum Strategy {
    /// gitignore-style line file with a managed comment block.
    LineBlock { supports_negation: bool },
    /// String array inside a JSON settings file.
    JsonArray {
        pointer: &'static [&'static str],
        render: JsonRender,
    },
    /// String array inside a TOML file.
    TomlList {
        table: &'static str,
        key: &'static str,
    },
    /// No file to write — print guidance instead.
    Advisory { message: &'static str },
}

pub struct TargetSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub rel_path: Option<&'static str>,
    pub strategy: Strategy,
    /// Targets excluded from the default set (reachable via --only).
    pub default_enabled: bool,
}

const fn line(id: &'static str, name: &'static str, path: &'static str) -> TargetSpec {
    TargetSpec {
        id,
        name,
        rel_path: Some(path),
        strategy: Strategy::LineBlock {
            supports_negation: true,
        },
        default_enabled: true,
    }
}

pub static TARGETS: &[TargetSpec] = &[
    line("cursor", "Cursor", ".cursorignore"),
    TargetSpec {
        default_enabled: false, // index-only, redundant with .cursorignore
        ..line(
            "cursor-index",
            "Cursor (indexing only)",
            ".cursorindexingignore",
        )
    },
    line("windsurf", "Windsurf", ".codeiumignore"),
    line("aider", "Aider", ".aiderignore"),
    line("jetbrains", "JetBrains AI / Junie", ".aiignore"),
    TargetSpec {
        strategy: Strategy::LineBlock {
            supports_negation: false, // .aiexclude has no `!` support
        },
        ..line("gemini-ca", "Gemini Code Assist", ".aiexclude")
    },
    line("gemini-cli", "Gemini CLI", ".geminiignore"),
    line("continue", "Continue.dev", ".continueignore"),
    line("cline", "Cline", ".clineignore"),
    line("roo", "Roo Code", ".rooignore"),
    line("tabnine", "Tabnine", ".tabnineignore"),
    line("augment", "Augment", ".augmentignore"),
    line("kilocode", "Kilo Code", ".kilocodeignore"),
    line("goose", "Goose", ".gooseignore"),
    line("kiro", "Kiro", ".kiroignore"),
    line("trae", "Trae", ".trae/.ignore"),
    TargetSpec {
        id: "claude-code",
        name: "Claude Code",
        rel_path: Some(".claude/settings.json"),
        strategy: Strategy::JsonArray {
            pointer: &["permissions", "deny"],
            render: JsonRender::ClaudeDeny,
        },
        default_enabled: true,
    },
    TargetSpec {
        id: "zed",
        name: "Zed",
        rel_path: Some(".zed/settings.json"),
        strategy: Strategy::JsonArray {
            pointer: &["private_files"],
            render: JsonRender::Globs,
        },
        default_enabled: true,
    },
    TargetSpec {
        id: "qodo",
        name: "Qodo",
        rel_path: Some(".ai_config.toml"),
        strategy: Strategy::TomlList {
            table: "file_filters",
            key: "exclude",
        },
        default_enabled: true,
    },
    TargetSpec {
        id: "copilot",
        name: "GitHub Copilot",
        rel_path: None,
        strategy: Strategy::Advisory {
            message: "GitHub Copilot has no repo-level ignore file; configure Content Exclusion \
                      in repository/org settings on GitHub (Settings → Copilot → Content exclusion). \
                      Note: it does not apply to Copilot agent mode/CLI.",
        },
        default_enabled: true,
    },
    TargetSpec {
        id: "codex",
        name: "OpenAI Codex CLI",
        rel_path: None,
        strategy: Strategy::Advisory {
            message: "OpenAI Codex CLI has no ignore-file support; protect secrets via its \
                      sandbox config (~/.codex/config.toml) or keep them outside the workspace.",
        },
        default_enabled: true,
    },
];

pub fn selected(only: &[String], exclude: &[String]) -> Result<Vec<&'static TargetSpec>> {
    for id in only.iter().chain(exclude) {
        if !TARGETS.iter().any(|t| t.id == id) {
            return usage_err(format!("unknown target id: {id} (see `noagents list`)"));
        }
    }
    Ok(TARGETS
        .iter()
        .filter(|t| {
            if !only.is_empty() {
                only.iter().any(|id| id == t.id)
            } else {
                t.default_enabled && !exclude.iter().any(|id| id == t.id)
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_unique() {
        let mut ids: Vec<_> = TARGETS.iter().map(|t| t.id).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len());
    }

    #[test]
    fn default_set_excludes_cursor_index() {
        let sel = selected(&[], &[]).unwrap();
        assert!(!sel.iter().any(|t| t.id == "cursor-index"));
        assert!(sel.iter().any(|t| t.id == "cursor"));
    }

    #[test]
    fn only_and_exclude() {
        let sel = selected(&["cursor-index".into()], &[]).unwrap();
        assert_eq!(sel.len(), 1);
        let sel = selected(&[], &["cursor".into()]).unwrap();
        assert!(!sel.iter().any(|t| t.id == "cursor"));
        assert!(selected(&["nope".into()], &[]).is_err());
    }
}
