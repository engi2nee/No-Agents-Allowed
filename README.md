# noagents

**One config to keep every AI coding agent out of your secrets.**

`noagents` fans out a single `.noagents` file (gitignore syntax) into the ignore/exclusion format of every known AI coding agent — Cursor, Claude Code, Windsurf, Aider, JetBrains AI, Gemini, Cline, Roo, Zed, and a dozen more. One command, no runtime dependencies, no elevated permissions.

```console
$ noagents init        # scaffold .noagents with sensible secret patterns
$ noagents generate    # write/update every agent's ignore file
```

## Why

Each AI agent invents its own ignore mechanism. Maintaining `.cursorignore`, `.codeiumignore`, `.aiderignore`, `.clineignore`, `.claude/settings.json` deny rules, … by hand means the tool you forgot is the one reading your `.env`.

## Install

**Homebrew** (macOS / Linux):

```console
brew install engi2nee/noagents/noagents
```

**Shell installer** (prebuilt binary, no toolchain needed):

```console
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/engi2nee/No-Agents-Allowed/releases/latest/download/noagents-installer.sh | sh
```

**Cargo** (builds from source):

```console
cargo install noagents
```

**Prebuilt binaries** for macOS (Apple Silicon / Intel), Linux (x86-64 / ARM64), and Windows are attached to every [GitHub release](https://github.com/engi2nee/No-Agents-Allowed/releases/latest) — download, extract, and put `noagents` on your `PATH`.

## Usage

```console
noagents init              # create .noagents (defaults cover .env, keys, cloud creds, tokens)
noagents generate          # fan out to all targets (alias: sync)
noagents add "internal/"   # append pattern(s) to .noagents and regenerate
noagents status            # per-target: in-sync | stale | missing
noagents check             # CI gate: exit 1 on drift, prints DRIFT: lines
noagents remove            # strip everything noagents manages, restore originals
noagents list              # all supported targets
```

Common flags: `--only cursor,aider` · `--exclude zed` · `--dry-run` (unified diffs, writes nothing) · `--root <path>` · `--quiet`.

## How it works

- **`.noagents` is the source of truth.** gitignore syntax, order preserved.
- **Line-file targets** get a managed block; everything outside it is yours and never touched:

  ```gitignore
  # your own rules stay here

  # >>> noagents managed — DO NOT EDIT; run `noagents generate` >>>
  .env
  secrets/
  # <<< noagents managed <<<
  ```

- **JSON/TOML targets** (Claude Code, Zed, Qodo) are merged structurally: sibling keys, key order, comments (TOML) preserved. Ownership is tracked in a committed `.noagents.state` sidecar so regeneration removes exactly the entries it added — never yours.
- **Idempotent.** Re-running `generate` with no config change touches nothing.
- **Commit the generated files** (and `.noagents.state`) so every clone and CI run is protected.

## Supported targets

| ID | Tool | File | Notes |
|---|---|---|---|
| `cursor` | Cursor | `.cursorignore` | best-effort per Cursor docs |
| `cursor-index` | Cursor (indexing) | `.cursorindexingignore` | opt-in via `--only` (index-only) |
| `windsurf` | Windsurf | `.codeiumignore` | |
| `aider` | Aider | `.aiderignore` | |
| `jetbrains` | JetBrains AI / Junie | `.aiignore` | enable in IDE settings |
| `gemini-ca` | Gemini Code Assist | `.aiexclude` | no `!` negation — such lines are skipped |
| `gemini-cli` | Gemini CLI | `.geminiignore` | |
| `continue` | Continue.dev | `.continueignore` | indexing only |
| `cline` | Cline | `.clineignore` | |
| `roo` | Roo Code | `.rooignore` | |
| `tabnine` | Tabnine | `.tabnineignore` | indexing only |
| `augment` | Augment | `.augmentignore` | indexing only |
| `kilocode` | Kilo Code | `.kilocodeignore` | |
| `goose` | Goose | `.gooseignore` | primarily blocks modification |
| `kiro` | Kiro | `.kiroignore` | |
| `trae` | Trae | `.trae/.ignore` | |
| `claude-code` | Claude Code | `.claude/settings.json` | `permissions.deny` `Read(...)` rules — enforced permission boundary |
| `zed` | Zed | `.zed/settings.json` | `private_files` globs; JSONC files with comments are skipped with a warning |
| `qodo` | Qodo | `.ai_config.toml` | `[file_filters] exclude` |
| `copilot` | GitHub Copilot | — | advisory: configure Content Exclusion in GitHub settings; not enforced in agent mode/CLI |
| `codex` | OpenAI Codex CLI | — | advisory: no ignore-file support exists |

## Caveats

- **Negations (`!pattern`)** are supported by most line targets, but skipped (with a warning) for `.aiexclude`, Claude Code, Zed, and Qodo — those formats have no negation concept.
- **Ignore files are not a security boundary.** Most tools document them as best-effort; bypass bugs happen. Treat `noagents` as defense-in-depth: it makes agents *far less likely* to read secrets, not incapable of it. Real secrets belong outside the repo or encrypted.
- Some targets only affect *indexing* (noted above); the agent may still open a file if explicitly pointed at it.
- **Commit `.noagents.state`.** It records which entries noagents added to JSON/TOML settings files. `remove` still cleans up from your current `.noagents` if the state file is lost, but if you *also* changed patterns since the last `generate`, previously-added entries can only be cleaned while the state file exists.
- **Structured files get canonically formatted.** On first merge, `.claude/settings.json` and `.zed/settings.json` are rewritten with 2-space indentation (`.ai_config.toml` keeps its formatting via `toml_edit`). This is a one-time diff; subsequent runs are stable. Files containing `//` comments (JSONC) are skipped with a warning, never rewritten.
- `remove` restores your original content but normalizes files to end with a single trailing newline.

## Development

```console
cargo test        # unit + integration + snapshot tests
cargo clippy --all-targets
```

## License

MIT
