use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;

use crate::cli::{self, Command};
use crate::config::{self, CurrentConfig};
use crate::profiles::{self, ProfileKind, SavedProfile};
use crate::prompt::prompt_value;

pub fn run() -> Result<(), String> {
    let command = cli::parse_command(env::args().skip(1))?;
    let home = dirs::home_dir().ok_or_else(|| "failed to resolve home directory".to_owned())?;

    match command {
        Command::ClaudeCode => run_claude_code(&home),
        Command::Codex => run_codex(&home),
        Command::Current => print_current_config(&home),
        Command::Save { name } => save_profile(&home, &name),
        Command::Use { name } => use_profile(&home, name.as_deref()),
        Command::List => list_profiles(&home),
        Command::Completion { shell } => print_completion(&shell),
        Command::InstallCompletion { shell } => install_completion(&home, &shell),
        Command::CompleteProfiles { prefix } => complete_profiles(&home, &prefix),
        Command::Help => {
            println!("{}", cli::usage_text());
            process::exit(0);
        }
    }
}

fn run_claude_code(home: &Path) -> Result<(), String> {
    let token = prompt_value("ANTHROPIC_AUTH_TOKEN")?;
    let base_url = prompt_value("ANTHROPIC_BASE_URL")?;
    config::update_claude_code(home, &token, &base_url)?;
    println!(
        "Updated {} and {} in ~/.claude/settings.json.",
        "env.ANTHROPIC_AUTH_TOKEN", "env.ANTHROPIC_BASE_URL"
    );
    Ok(())
}

fn run_codex(home: &Path) -> Result<(), String> {
    let base_url = prompt_value("base_url")?;
    let api_key = prompt_value("OPENAI_API_KEY")?;
    config::update_codex(home, &base_url, &api_key)?;
    println!(
        "Updated {} in ~/.codex/config.toml.",
        "model_providers.mirror.base_url"
    );
    println!("Updated {} in ~/.codex/auth.json.", "OPENAI_API_KEY");
    Ok(())
}

fn print_current_config(home: &Path) -> Result<(), String> {
    let current = config::read_current_config(home)?;
    print_current_sections(&current);
    Ok(())
}

fn print_current_sections(current: &CurrentConfig) {
    println!("Claude Code");
    println!("  ANTHROPIC_BASE_URL: {}", current.claude_base_url);
    println!(
        "  ANTHROPIC_AUTH_TOKEN: {}",
        mask_secret(&current.claude_token)
    );
    println!("Codex");
    println!("  base_url: {}", current.codex_base_url);
    println!("  OPENAI_API_KEY: {}", mask_secret(&current.codex_api_key));
}

fn save_profile(home: &Path, name: &str) -> Result<(), String> {
    let kind = prompt_profile_kind()?;
    let stored_name = format!("{}{}", profiles::profile_prefix(kind), name);
    let profile = match kind {
        ProfileKind::ClaudeCode => SavedProfile {
            kind,
            name: stored_name.clone(),
            value_a: prompt_value("ANTHROPIC_AUTH_TOKEN")?,
            value_b: prompt_value("ANTHROPIC_BASE_URL")?,
        },
        ProfileKind::Codex => SavedProfile {
            kind,
            name: stored_name.clone(),
            value_a: prompt_value("base_url")?,
            value_b: prompt_value("OPENAI_API_KEY")?,
        },
    };

    let path = profiles::save_profile(home, &profile)?;
    println!("Saved profile {} to {}.", profile.name, path.display());
    Ok(())
}

fn use_profile(home: &Path, name: Option<&str>) -> Result<(), String> {
    let selected = match name {
        Some(name) => name.to_owned(),
        None => select_profile_interactively(home)?,
    };
    profiles::apply_profile(home, &selected)?;
    println!("Applied profile {}.", selected);
    Ok(())
}

fn list_profiles(home: &Path) -> Result<(), String> {
    let names = profiles::list_profile_names(home)?;
    if names.is_empty() {
        println!("No saved profiles.");
        return Ok(());
    }

    println!("Claude Code");
    print_profile_group(names.iter().filter(|name| name.starts_with("cc-")));
    println!("Codex");
    print_profile_group(names.iter().filter(|name| name.starts_with("cx-")));
    Ok(())
}

fn print_profile_group<'a>(names: impl Iterator<Item = &'a String>) {
    let mut printed = false;
    for name in names {
        println!("  {name}");
        printed = true;
    }
    if !printed {
        println!("  (none)");
    }
}

fn complete_profiles(home: &Path, prefix: &str) -> Result<(), String> {
    for name in profiles::complete_profiles(home, prefix)? {
        println!("{name}");
    }
    Ok(())
}

fn install_completion(home: &Path, shell: &str) -> Result<(), String> {
    let config_path = shell_config_path(home, shell)?;
    let snippet = completion_install_snippet(shell);
    let start_marker = "# >>> cxc completion >>>";
    let end_marker = "# <<< cxc completion <<<";

    let existing = if config_path.exists() {
        std::fs::read_to_string(&config_path)
            .map_err(|err| format!("failed reading {}: {err}", config_path.display()))?
    } else {
        String::new()
    };

    let cleaned = remove_existing_completion_block(&existing, start_marker, end_marker);
    let mut updated = cleaned.trim_end().to_owned();
    if !updated.is_empty() {
        updated.push_str("\n\n");
    }
    updated.push_str(start_marker);
    updated.push('\n');
    updated.push_str(snippet);
    updated.push('\n');
    updated.push_str(end_marker);
    updated.push('\n');

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("failed creating {}: {err}", parent.display()))?;
    }

    std::fs::write(&config_path, updated)
        .map_err(|err| format!("failed writing {}: {err}", config_path.display()))?;
    println!(
        "Installed {} completion into {}.",
        shell,
        config_path.display()
    );
    Ok(())
}

fn print_completion(shell: &str) -> Result<(), String> {
    match shell {
        "zsh" => {
            std::io::stdout()
                .write_all(completion_script(shell).as_bytes())
                .map_err(|err| err.to_string())?;
            Ok(())
        }
        "bash" => {
            std::io::stdout()
                .write_all(completion_script(shell).as_bytes())
                .map_err(|err| err.to_string())?;
            Ok(())
        }
        _ => Err("supported shells: zsh, bash".to_owned()),
    }
}

fn completion_script(shell: &str) -> &'static str {
    match shell {
        "zsh" => {
            r#"#compdef cxc

_cxc() {
  local -a commands
  commands=(
    '--cc:Update Claude Code config'
    '--cx:Update Codex config'
    'current:Show current config'
    'save:Save a profile'
    'use:Apply a saved profile'
    'list:List saved profiles'
    'completion:Print shell completion'
  )

  if (( CURRENT == 2 )); then
    _describe 'command' commands
    return
  fi

  if [[ $words[2] == use && CURRENT == 3 ]]; then
    local -a profiles
    profiles=("${(@f)$(cxc __complete_profiles "$words[CURRENT]")}")
    _describe 'profile' profiles
    return
  fi

  if [[ $words[2] == completion && CURRENT == 3 ]]; then
    _describe 'shell' 'zsh bash'
    return
  fi

  if [[ $words[2] == completion && CURRENT == 4 && $words[3] == install ]]; then
    _describe 'shell' 'zsh bash'
    return
  fi
}

_cxc "$@"
"#
        }
        "bash" => {
            r#"_cxc_complete() {
  local cur
  COMPREPLY=()
  cur="${COMP_WORDS[COMP_CWORD]}"

  if [[ $COMP_CWORD -eq 1 ]]; then
    COMPREPLY=( $(compgen -W "--cc --cx current save use list completion" -- "$cur") )
    return
  fi

  if [[ ${COMP_WORDS[1]} == "use" && $COMP_CWORD -eq 2 ]]; then
    COMPREPLY=( $(compgen -W "$(cxc __complete_profiles "$cur")" -- "$cur") )
    return
  fi

  if [[ ${COMP_WORDS[1]} == "completion" && $COMP_CWORD -eq 2 ]]; then
    COMPREPLY=( $(compgen -W "zsh bash install" -- "$cur") )
    return
  fi

  if [[ ${COMP_WORDS[1]} == "completion" && ${COMP_WORDS[2]} == "install" && $COMP_CWORD -eq 3 ]]; then
    COMPREPLY=( $(compgen -W "zsh bash" -- "$cur") )
  fi
}

complete -F _cxc_complete cxc
"#
        }
        _ => "",
    }
}

fn completion_install_snippet(shell: &str) -> &'static str {
    match shell {
        "zsh" => r#"source <(cxc completion zsh)"#,
        "bash" => r#"source <(cxc completion bash)"#,
        _ => "",
    }
}

fn shell_config_path(home: &Path, shell: &str) -> Result<PathBuf, String> {
    match shell {
        "zsh" => Ok(home.join(".zshrc")),
        "bash" => Ok(home.join(".bashrc")),
        _ => Err("supported shells: zsh, bash".to_owned()),
    }
}

fn remove_existing_completion_block(content: &str, start_marker: &str, end_marker: &str) -> String {
    if let Some(start) = content.find(start_marker) {
        if let Some(end_rel) = content[start..].find(end_marker) {
            let end = start + end_rel + end_marker.len();
            let mut cleaned = String::new();
            cleaned.push_str(content[..start].trim_end());
            let suffix = content[end..].trim_start_matches(['\n', '\r']);
            if !cleaned.is_empty() && !suffix.is_empty() {
                cleaned.push('\n');
            }
            cleaned.push_str(suffix);
            return cleaned;
        }
    }
    content.to_owned()
}

fn prompt_profile_kind() -> Result<ProfileKind, String> {
    loop {
        let raw = prompt_value("Save for (cc/cx)")?;
        match raw.trim() {
            "cc" => return Ok(ProfileKind::ClaudeCode),
            "cx" => return Ok(ProfileKind::Codex),
            _ => println!("Please enter cc or cx."),
        }
    }
}

fn select_profile_interactively(home: &Path) -> Result<String, String> {
    let kind = prompt_profile_kind()?;
    let names = profiles::list_profiles_by_kind(home, kind)?;
    if names.is_empty() {
        return Err(format!("no saved {} profiles", profile_kind_label(kind)));
    }

    println!("Available {} profiles:", profile_kind_label(kind));
    for (index, name) in names.iter().enumerate() {
        println!("  {}. {}", index + 1, name);
    }

    loop {
        let raw = prompt_value("Choose profile number")?;
        let selection: usize = raw
            .parse()
            .map_err(|_| "profile selection must be a number".to_owned())?;
        if let Some(name) = names.get(selection.saturating_sub(1)) {
            return Ok(name.clone());
        }
        println!("Please enter a valid number.");
    }
}

fn profile_kind_label(kind: ProfileKind) -> &'static str {
    match kind {
        ProfileKind::ClaudeCode => "Claude Code",
        ProfileKind::Codex => "Codex",
    }
}

fn mask_secret(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 8 {
        return "*".repeat(chars.len().max(1));
    }

    let prefix: String = chars.iter().take(4).collect();
    let suffix: String = chars
        .iter()
        .rev()
        .take(4)
        .copied()
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}...{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_long_secret() {
        assert_eq!(mask_secret("1234567890abcdef"), "1234...cdef");
    }

    #[test]
    fn removes_existing_completion_block() {
        let content = "line1\n# >>> cxc completion >>>\nold\n# <<< cxc completion <<<\nline2\n";
        let cleaned = remove_existing_completion_block(
            content,
            "# >>> cxc completion >>>",
            "# <<< cxc completion <<<",
        );
        assert_eq!(cleaned, "line1\nline2\n");
    }
}
