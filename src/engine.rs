use crate::config::{self, Config, usage_err};
use crate::merge::{block, json, toml};
use crate::patterns::{Pattern, to_claude_deny, to_glob};
use crate::state::{State, StateEntry};
use crate::targets::{JsonRender, Strategy, TargetSpec, selected};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct RunOpts {
    pub root: PathBuf,
    pub only: Vec<String>,
    pub exclude: Vec<String>,
    pub dry_run: bool,
    pub quiet: bool,
}

enum Outcome {
    Created,
    Updated,
    Unchanged,
    Skipped(String),
}

/// Desired content + ownership for one target, computed without touching disk.
struct Computed {
    content: Option<String>, // None for advisory targets
    owned: Vec<String>,
}

fn render_lines(patterns: &[Pattern], supports_negation: bool) -> Vec<String> {
    patterns
        .iter()
        .filter(|p| supports_negation || !p.negated)
        .map(|p| p.raw.clone())
        .collect()
}

fn render_entries(patterns: &[Pattern], render: JsonRender) -> Vec<String> {
    match render {
        JsonRender::ClaudeDeny => patterns.iter().flat_map(to_claude_deny).collect(),
        JsonRender::Globs => patterns.iter().filter_map(to_glob).collect(),
    }
}

fn compute(
    spec: &TargetSpec,
    existing: Option<&str>,
    cfg: &Config,
    entry: &StateEntry,
) -> Result<Computed> {
    match spec.strategy {
        Strategy::LineBlock { supports_negation } => {
            let lines = render_lines(&cfg.patterns, supports_negation);
            let content = block::apply(existing, &lines)?;
            Ok(Computed {
                content: Some(content),
                owned: vec![],
            })
        }
        Strategy::JsonArray { pointer, render } => {
            let desired = render_entries(&cfg.patterns, render);
            let merged = json::apply(existing, pointer, &entry.entries, &desired)?;
            Ok(Computed {
                content: Some(merged.content),
                owned: merged.owned,
            })
        }
        Strategy::TomlList { table, key } => {
            let desired = render_entries(&cfg.patterns, JsonRender::Globs);
            let merged = toml::apply(existing, table, key, &entry.entries, &desired)?;
            Ok(Computed {
                content: Some(merged.content),
                owned: merged.owned,
            })
        }
        Strategy::Advisory { .. } => Ok(Computed {
            content: None,
            owned: vec![],
        }),
    }
}

fn read_existing(path: &Path) -> Result<Option<String>> {
    if path.is_file() {
        Ok(Some(std::fs::read_to_string(path).with_context(|| {
            format!("cannot read {}", path.display())
        })?))
    } else {
        Ok(None)
    }
}

fn negation_warning(cfg: &Config, specs: &[&TargetSpec]) -> Option<String> {
    let negated: Vec<&str> = cfg
        .patterns
        .iter()
        .filter(|p| p.negated)
        .map(|p| p.raw.as_str())
        .collect();
    if negated.is_empty() {
        return None;
    }
    let affected: Vec<&str> = specs
        .iter()
        .filter(|s| match s.strategy {
            Strategy::LineBlock { supports_negation } => !supports_negation,
            Strategy::JsonArray { .. } | Strategy::TomlList { .. } => true,
            Strategy::Advisory { .. } => false,
        })
        .map(|s| s.id)
        .collect();
    if affected.is_empty() {
        return None;
    }
    Some(format!(
        "warning: negated pattern(s) {} unsupported by {} — skipped there",
        negated.join(", "),
        affected.join(", ")
    ))
}

fn print_outcome(spec: &TargetSpec, outcome: &Outcome, quiet: bool) {
    let path = spec.rel_path.unwrap_or("-");
    match outcome {
        Outcome::Created => println!("  created    {path}  ({})", spec.id),
        Outcome::Updated => println!("  updated    {path}  ({})", spec.id),
        Outcome::Unchanged => {
            if !quiet {
                println!("  unchanged  {path}  ({})", spec.id);
            }
        }
        Outcome::Skipped(reason) => eprintln!("  skipped    {path}  ({}): {reason}", spec.id),
    }
}

fn print_diff(rel_path: &str, old: &str, new: &str) {
    let diff = similar::TextDiff::from_lines(old, new);
    println!("--- {rel_path}");
    println!("+++ {rel_path}");
    print!("{}", diff.unified_diff().context_radius(2));
}

pub fn generate(opts: &RunOpts) -> Result<()> {
    let cfg = config::load(&opts.root)?;
    let specs = selected(&opts.only, &opts.exclude)?;
    let mut state = State::load(&opts.root)?;
    let mut advisories: Vec<(&str, &str)> = Vec::new();

    for spec in &specs {
        if let Strategy::Advisory { message } = spec.strategy {
            advisories.push((spec.name, message));
            continue;
        }
        let rel = spec.rel_path.expect("non-advisory targets have a path");
        let path = opts.root.join(rel);
        let existing = read_existing(&path)?;
        let mut entry = state.entry(spec.id);

        let computed = match compute(spec, existing.as_deref(), &cfg, &entry) {
            Ok(c) => c,
            Err(e) => {
                print_outcome(spec, &Outcome::Skipped(format!("{e:#}")), opts.quiet);
                continue;
            }
        };
        let new_content = computed.content.expect("non-advisory");

        let outcome = if existing.as_deref() == Some(new_content.as_str()) {
            Outcome::Unchanged
        } else if opts.dry_run {
            print_diff(rel, existing.as_deref().unwrap_or(""), &new_content);
            if existing.is_none() {
                Outcome::Created
            } else {
                Outcome::Updated
            }
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("cannot create {}", parent.display()))?;
            }
            std::fs::write(&path, &new_content)
                .with_context(|| format!("cannot write {}", path.display()))?;
            if existing.is_none() {
                entry.created_file = true;
            }
            if existing.is_none() {
                Outcome::Created
            } else {
                Outcome::Updated
            }
        };

        if !opts.dry_run {
            entry.entries = computed.owned;
            state.set_entry(spec.id, entry);
        }
        print_outcome(spec, &outcome, opts.quiet);
    }

    if !opts.dry_run {
        state.save(&opts.root)?;
    }
    if let Some(w) = negation_warning(&cfg, &specs) {
        eprintln!("{w}");
    }
    if !opts.quiet {
        for (name, msg) in advisories {
            println!("  note       {name}: {msg}");
        }
    }
    Ok(())
}

pub fn remove(opts: &RunOpts) -> Result<()> {
    let specs = selected(&opts.only, &opts.exclude)?;
    let mut state = State::load(&opts.root)?;

    for spec in &specs {
        let Some(rel) = spec.rel_path else { continue };
        let path = opts.root.join(rel);
        let Some(existing) = read_existing(&path)? else {
            state.set_entry(spec.id, StateEntry::default());
            continue;
        };
        let entry = state.entry(spec.id);

        let remaining = match spec.strategy {
            Strategy::LineBlock { .. } => block::remove(&existing)?,
            Strategy::JsonArray { pointer, .. } => {
                json::remove(&existing, pointer, &entry.entries)?
            }
            Strategy::TomlList { table, key } => {
                toml::remove(&existing, table, key, &entry.entries)?
            }
            Strategy::Advisory { .. } => continue,
        };

        match remaining {
            Some(content) if content == existing => {
                if !opts.quiet {
                    println!("  untouched  {rel}  ({})", spec.id);
                }
            }
            Some(content) => {
                if !opts.dry_run {
                    std::fs::write(&path, &content)
                        .with_context(|| format!("cannot write {}", path.display()))?;
                }
                println!("  stripped   {rel}  ({})", spec.id);
            }
            None => {
                if entry.created_file {
                    if !opts.dry_run {
                        std::fs::remove_file(&path)
                            .with_context(|| format!("cannot remove {}", path.display()))?;
                        // Drop the parent dir too if the file was the only
                        // thing in it (fails when non-empty; that's fine).
                        if rel.contains('/')
                            && let Some(parent) = path.parent()
                        {
                            let _ = std::fs::remove_dir(parent);
                        }
                    }
                    println!("  deleted    {rel}  ({})", spec.id);
                } else {
                    if !opts.dry_run {
                        std::fs::write(&path, "")
                            .with_context(|| format!("cannot write {}", path.display()))?;
                    }
                    println!("  emptied    {rel}  ({})", spec.id);
                }
            }
        }
        if !opts.dry_run {
            state.set_entry(spec.id, StateEntry::default());
        }
    }
    if !opts.dry_run {
        state.save(&opts.root)?;
    }
    Ok(())
}

/// Returns true when any target has drifted from the generated state.
pub fn status(opts: &RunOpts, check: bool) -> Result<bool> {
    let cfg = config::load(&opts.root)?;
    let specs = selected(&opts.only, &opts.exclude)?;
    let state = State::load(&opts.root)?;
    let mut drift = false;

    for spec in &specs {
        let Some(rel) = spec.rel_path else {
            if !check && !opts.quiet {
                println!("  advisory   -  ({})", spec.id);
            }
            continue;
        };
        let path = opts.root.join(rel);
        let existing = read_existing(&path)?;
        let entry = state.entry(spec.id);

        let label = match compute(spec, existing.as_deref(), &cfg, &entry) {
            Err(e) => {
                drift = true;
                format!("error: {e:#}")
            }
            Ok(c) => {
                let expected = c.content.expect("non-advisory");
                match &existing {
                    None => {
                        drift = true;
                        "missing".to_string()
                    }
                    Some(cur) if *cur == expected => "in-sync".to_string(),
                    Some(_) => {
                        drift = true;
                        "stale".to_string()
                    }
                }
            }
        };
        let is_drift = label != "in-sync";
        if check {
            if is_drift {
                println!("DRIFT: {}  {rel}  ({label})", spec.id);
            }
        } else if is_drift || !opts.quiet {
            println!("  {label:<10} {rel}  ({})", spec.id);
        }
    }
    if check && drift {
        eprintln!("run `noagents generate` to fix");
    }
    Ok(drift)
}

pub fn list(root: &Path) -> Result<()> {
    println!(
        "{:<13} {:<25} {:<25} {:<9} present",
        "ID", "TOOL", "FILE", "DEFAULT"
    );
    for spec in crate::targets::TARGETS {
        let path = spec.rel_path.unwrap_or("(advisory)");
        let present = spec
            .rel_path
            .map(|p| root.join(p).is_file())
            .unwrap_or(false);
        println!(
            "{:<13} {:<25} {:<25} {:<9} {}",
            spec.id,
            spec.name,
            path,
            if spec.default_enabled { "yes" } else { "no" },
            if present { "yes" } else { "-" }
        );
    }
    Ok(())
}

pub fn init(root: &Path, force: bool) -> Result<()> {
    let path = root.join(config::CONFIG_FILE);
    if path.exists() && !force {
        return usage_err(format!(
            "{} already exists (use --force to overwrite)",
            path.display()
        ));
    }
    std::fs::write(&path, config::DEFAULT_TEMPLATE)
        .with_context(|| format!("cannot write {}", path.display()))?;
    println!("created {}", path.display());
    println!("edit it to taste, then run: noagents generate");
    Ok(())
}

pub fn add(patterns: &[String], opts: &RunOpts) -> Result<()> {
    let added = config::add_patterns(&opts.root, patterns)?;
    println!(
        "added {added} pattern(s) to {}",
        opts.root.join(config::CONFIG_FILE).display()
    );
    generate(opts)
}
