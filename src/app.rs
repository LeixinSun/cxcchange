use std::env;
use std::path::Path;
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
    let cc_names = profiles::list_profiles_by_kind(home, ProfileKind::ClaudeCode)?;
    let cx_names = profiles::list_profiles_by_kind(home, ProfileKind::Codex)?;
    if cc_names.is_empty() && cx_names.is_empty() {
        println!("No saved profiles.");
        return Ok(());
    }

    println!("Claude Code");
    print_profile_group(cc_names.iter());
    println!("Codex");
    print_profile_group(cx_names.iter());
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

fn prompt_profile_kind() -> Result<ProfileKind, String> {
    loop {
        let raw = prompt_value("Choose profile kind (cc/cx)")?;
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
}
