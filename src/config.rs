use crate::patterns::Pattern;
use anyhow::{Context, Result};
use std::fmt;
use std::path::{Path, PathBuf};

pub const CONFIG_FILE: &str = ".noagents";
pub const STATE_FILE: &str = ".noagents.state";

/// Error caused by how the tool was invoked (missing config, bad input).
/// Mapped to exit code 2 in `main`.
#[derive(Debug)]
pub struct UsageError(pub String);

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

pub fn usage_err<T>(msg: impl Into<String>) -> Result<T> {
    Err(UsageError(msg.into()).into())
}

/// Resolves the project root: explicit `--root`, else the nearest ancestor
/// containing `.noagents`, else the nearest containing `.git`, else cwd.
pub fn find_root(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(root) = explicit {
        return root
            .canonicalize()
            .with_context(|| format!("--root {} does not exist", root.display()));
    }
    let cwd = std::env::current_dir().context("cannot determine current directory")?;
    for anc in cwd.ancestors() {
        if anc.join(CONFIG_FILE).is_file() {
            return Ok(anc.to_path_buf());
        }
    }
    for anc in cwd.ancestors() {
        if anc.join(".git").exists() {
            return Ok(anc.to_path_buf());
        }
    }
    Ok(cwd)
}

pub struct Config {
    pub patterns: Vec<Pattern>,
}

impl Config {
    pub fn parse(text: &str) -> Config {
        Config {
            patterns: text.lines().filter_map(Pattern::parse).collect(),
        }
    }
}

pub fn load(root: &Path) -> Result<Config> {
    let path = root.join(CONFIG_FILE);
    if !path.is_file() {
        return usage_err(format!(
            "no {} found at {} — run `noagents init` first",
            CONFIG_FILE,
            root.display()
        ));
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    Ok(Config::parse(&text))
}

/// Appends patterns to `.noagents`, skipping ones already present.
/// Returns the number actually added.
pub fn add_patterns(root: &Path, new: &[String]) -> Result<usize> {
    let path = root.join(CONFIG_FILE);
    if !path.is_file() {
        return usage_err(format!(
            "no {} found at {} — run `noagents init` first",
            CONFIG_FILE,
            root.display()
        ));
    }
    let mut text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let existing: Vec<String> = text
        .lines()
        .filter_map(Pattern::parse)
        .map(|p| p.raw)
        .collect();

    let mut added = 0;
    for raw in new {
        let Some(p) = Pattern::parse(raw) else {
            return usage_err(format!("invalid pattern: {raw:?}"));
        };
        if existing.contains(&p.raw) {
            continue;
        }
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&p.raw);
        text.push('\n');
        added += 1;
    }
    if added > 0 {
        std::fs::write(&path, &text).with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(added)
}

pub const DEFAULT_TEMPLATE: &str = "\
# noagents configuration — gitignore syntax.
# Patterns here are fanned out to every AI-agent ignore file.
# After editing, run: noagents generate

# --- Environment & secrets ---
.env
.env.*
!.env.example
*.pem
*.key
*.p12
*.pfx
id_rsa*
id_ed25519*
*.keystore
credentials.json
service-account*.json
secrets/
.secrets/

# --- Cloud & infra credentials ---
.aws/
kubeconfig
*.tfstate
*.tfstate.*
.terraform/

# --- Tokens & auth files ---
.npmrc
.netrc
.pypirc
.git-credentials

# --- Proprietary code (add your own) ---
# internal/
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_template() {
        let cfg = Config::parse(DEFAULT_TEMPLATE);
        assert!(cfg.patterns.iter().any(|p| p.raw == ".env"));
        assert!(cfg.patterns.iter().any(|p| p.negated));
        // comments not parsed as patterns
        assert!(!cfg.patterns.iter().any(|p| p.raw.starts_with('#')));
    }
}
