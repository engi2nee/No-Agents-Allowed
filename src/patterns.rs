/// A single pattern line from `.noagents`, gitignore semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    /// The line as written (after trimming), e.g. `!.env.example`, `secrets/`.
    pub raw: String,
    /// Pattern body without `!` prefix, leading `/`, or trailing `/`.
    pub body: String,
    pub negated: bool,
    pub dir_only: bool,
    /// True when the pattern is anchored to the root (leading `/` or an
    /// interior `/`), per gitignore rules.
    pub anchored: bool,
}

impl Pattern {
    /// Parses one config line. Returns `None` for blanks and comments.
    pub fn parse(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }
        let raw = trimmed.to_string();
        let mut body = trimmed;
        let negated = body.starts_with('!');
        if negated {
            body = &body[1..];
        }
        let dir_only = body.ends_with('/');
        if dir_only {
            body = &body[..body.len() - 1];
        }
        let anchored = body.starts_with('/') || body.contains('/');
        let body = body.trim_start_matches('/').to_string();
        if body.is_empty() {
            return None;
        }
        Some(Pattern {
            raw,
            body,
            negated,
            dir_only,
            anchored,
        })
    }
}

/// Translates a pattern into Claude Code `permissions.deny` entries.
/// Negations have no equivalent and yield nothing (callers warn once per run).
/// Unanchored patterns emit both a root-level and a `**/` entry because `**`
/// zero-match semantics vary between glob matchers.
pub fn to_claude_deny(p: &Pattern) -> Vec<String> {
    if p.negated {
        return vec![];
    }
    let b = &p.body;
    if p.anchored {
        if p.dir_only {
            vec![format!("Read(./{b}/**)")]
        } else {
            vec![format!("Read(./{b})")]
        }
    } else if p.dir_only {
        vec![format!("Read(./{b}/**)"), format!("Read(./**/{b}/**)")]
    } else {
        vec![format!("Read(./{b})"), format!("Read(./**/{b})")]
    }
}

/// Translates a pattern into a plain glob (Zed `private_files`, Qodo excludes).
/// Negations yield `None`.
pub fn to_glob(p: &Pattern) -> Option<String> {
    if p.negated {
        return None;
    }
    let mut g = if p.anchored {
        p.body.clone()
    } else {
        format!("**/{}", p.body)
    };
    if p.dir_only {
        g.push_str("/**");
    }
    Some(g)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Pattern {
        Pattern::parse(s).unwrap()
    }

    #[test]
    fn skips_blanks_and_comments() {
        assert!(Pattern::parse("").is_none());
        assert!(Pattern::parse("   ").is_none());
        assert!(Pattern::parse("# comment").is_none());
        assert!(Pattern::parse("!").is_none());
        assert!(Pattern::parse("/").is_none());
    }

    #[test]
    fn parses_flags() {
        let p = parse(".env");
        assert!(!p.negated && !p.dir_only && !p.anchored);
        assert_eq!(p.body, ".env");

        let p = parse("secrets/");
        assert!(p.dir_only && !p.anchored);
        assert_eq!(p.body, "secrets");

        let p = parse("/foo/bar");
        assert!(p.anchored && !p.dir_only);
        assert_eq!(p.body, "foo/bar");

        let p = parse("foo/bar");
        assert!(p.anchored, "interior slash anchors");

        let p = parse("!.env.example");
        assert!(p.negated);
        assert_eq!(p.body, ".env.example");
        assert_eq!(p.raw, "!.env.example");
    }

    #[test]
    fn claude_deny_translation() {
        assert_eq!(
            to_claude_deny(&parse(".env")),
            vec!["Read(./.env)", "Read(./**/.env)"]
        );
        assert_eq!(
            to_claude_deny(&parse("*.pem")),
            vec!["Read(./*.pem)", "Read(./**/*.pem)"]
        );
        assert_eq!(
            to_claude_deny(&parse("secrets/")),
            vec!["Read(./secrets/**)", "Read(./**/secrets/**)"]
        );
        assert_eq!(to_claude_deny(&parse("/foo/bar")), vec!["Read(./foo/bar)"]);
        assert_eq!(
            to_claude_deny(&parse("/secrets/")),
            vec!["Read(./secrets/**)"]
        );
        assert!(to_claude_deny(&parse("!.env.example")).is_empty());
    }

    #[test]
    fn glob_translation() {
        assert_eq!(to_glob(&parse(".env")).unwrap(), "**/.env");
        assert_eq!(to_glob(&parse("secrets/")).unwrap(), "**/secrets/**");
        assert_eq!(to_glob(&parse("/foo/bar")).unwrap(), "foo/bar");
        assert_eq!(to_glob(&parse("/secrets/")).unwrap(), "secrets/**");
        assert!(to_glob(&parse("!.env.example")).is_none());
    }

    #[test]
    fn interior_slash_is_anchored() {
        // gitignore: a slash anywhere but the trailing position anchors it
        let p = parse("a/b/*.key");
        assert!(p.anchored);
        assert_eq!(to_claude_deny(&p), vec!["Read(./a/b/*.key)"]);
        assert_eq!(to_glob(&p).unwrap(), "a/b/*.key");
    }

    #[test]
    fn double_star_passes_through() {
        let p = parse("**/build");
        assert!(p.anchored, "contains slash");
        assert_eq!(to_glob(&p).unwrap(), "**/build");
        assert_eq!(to_claude_deny(&p), vec!["Read(./**/build)"]);
    }

    #[test]
    fn whitespace_only_and_tabs_skipped() {
        assert!(Pattern::parse("\t").is_none());
        assert!(Pattern::parse("  \t  ").is_none());
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let p = parse("  .env  ");
        assert_eq!(p.body, ".env");
        assert_eq!(p.raw, ".env");
    }

    #[test]
    fn anchored_dir_single_entry_for_claude() {
        // leading-slash dir must not get the extra **/ form
        assert_eq!(to_claude_deny(&parse("/build/")), vec!["Read(./build/**)"]);
    }
}
