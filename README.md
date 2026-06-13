# cxc

`cxc` is a small CLI for people currently using mirror APIs to switch Claude Code and Codex credentials quickly.

The goal is simple: change your key and API endpoint directly from the terminal without opening config files in `vim` or `nano`, and without relying on extra switcher tools like `ccswitch`.

Chinese README: [README.zh-CN.md](./README.zh-CN.md)

## What it updates

- `cxc --cc`
  - updates `~/.claude/settings.json`
  - replaces `env.ANTHROPIC_AUTH_TOKEN`
  - replaces `env.ANTHROPIC_BASE_URL`
- `cxc --cx`
  - updates `~/.codex/config.toml`
  - replaces `[model_providers.mirror].base_url`
  - updates `~/.codex/auth.json`
  - replaces `OPENAI_API_KEY`

If a target file is missing, malformed, or the target key does not exist, the tool exits with an error. It does not silently add fields or modify unrelated content.

## Why

- you are using a mirror API
- you switch keys or proxy endpoints often
- you want a fast config change without editing files manually

## Build

```bash
cargo build --release
```

The binary will be available at:

```bash
target/release/cxc
```

You can move it into `~/bin` on macOS/Linux or any folder on your `PATH` on Windows.

## Usage

Switch Claude Code config:

```bash
cxc --cc
```

Switch Codex config:

```bash
cxc --cx
```

Show the current active config:

```bash
cxc current
```

Save a reusable profile into `~/.cxc/profiles/<name>.toml`:

```bash
cxc save work
```

`cxc` will ask whether this profile is for Claude Code (`cc`) or Codex (`cx`), then store it as `cc-work` or `cx-work`.

Apply a saved profile directly:

```bash
cxc use cc-work
```

Or just run:

```bash
cxc use
```

Then `cxc` will ask whether you want a Claude Code or Codex profile and let you choose from the saved list.

List saved profiles:

```bash
cxc list
```

All inputs are plain text, so typing or pasting works directly.

## Platform support

- macOS and Linux use the standard home directory path
- Windows uses the current user home directory and supports replacing config files with the Windows file move API

## Safety

- only replaces the target fields
- writes through a temporary file in the same directory
- reads the file back and verifies the new value after writing

## Verify

```bash
cargo fmt --check
cargo test
cargo build --release
```
