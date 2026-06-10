# Anthropic Cybersecurity Skills (bundled)

This directory contains **754** production-grade cybersecurity skills from the community project [Anthropic-Cybersecurity-Skills](https://github.com/mukul975/Anthropic-Cybersecurity-Skills), integrated into OmniNovalClaw under `skills/cybersecurity/`.

> **Attribution:** Independent community project; not affiliated with Anthropic PBC. Each skill may include its own `LICENSE` file (typically Apache-2.0). See `LICENSE` in this directory for the upstream repository license.

## Layout

- `<skill-name>/SKILL.md` — skill definition (YAML frontmatter + markdown body)
- `<skill-name>/scripts/`, `references/`, `assets/` — optional supporting files per skill
- `_index.json` — upstream skill catalog metadata
- `_SECURITY.md` — upstream security reporting policy

## Enabling in OmniNovalClaw

Skills are loaded recursively when `[skills] open_skills_enabled = true` and `open_skills_dir` points at the workspace or repo `skills/` directory (see `crates/omninova-core/src/skills/mod.rs`).

**Recommended** for this pack:

```toml
[skills]
open_skills_enabled = true
open_skills_dir = "/path/to/omninovalclaw/skills"   # or your workspace skills dir
prompt_injection_mode = "summary"
```

`summary` injects only skill names and descriptions into the system prompt; the agent should read the full `SKILL.md` on demand via file tools.

## Updating from upstream

```bash
rsync -a --exclude '.DS_Store' \
  /path/to/Anthropic-Cybersecurity-Skills-main/skills/ \
  /path/to/omninovalclaw/skills/cybersecurity/
cp /path/to/Anthropic-Cybersecurity-Skills-main/LICENSE skills/cybersecurity/
cp /path/to/Anthropic-Cybersecurity-Skills-main/index.json skills/cybersecurity/_index.json
```
