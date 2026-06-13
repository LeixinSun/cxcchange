use std::fs;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, value};

use crate::config;
use crate::fsutil::{atomic_write, read_file};

const CXC_PROFILES_DIR: &str = ".cxc/profiles";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileKind {
    ClaudeCode,
    Codex,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedProfile {
    pub kind: ProfileKind,
    pub name: String,
    pub value_a: String,
    pub value_b: String,
}

pub fn save_profile(home: &Path, profile: &SavedProfile) -> Result<PathBuf, String> {
    let dir = profiles_dir(home);
    fs::create_dir_all(&dir).map_err(|err| format!("failed creating {}: {err}", dir.display()))?;

    let profile_path = dir.join(format!("{}.toml", profile.name));
    atomic_write(&profile_path, &render_profile_document(profile))?;

    let written = read_file(&profile_path)?;
    let document = written.parse::<DocumentMut>().map_err(|err| {
        format!(
            "failed parsing written profile {}: {err}",
            profile_path.display()
        )
    })?;
    verify_profile_document(&document, profile, &profile_path)?;
    Ok(profile_path)
}

pub fn apply_profile(home: &Path, name: &str) -> Result<(), String> {
    let profile = read_saved_profile(home, name)?;
    match profile.kind {
        ProfileKind::ClaudeCode => {
            config::update_claude_code(home, &profile.value_a, &profile.value_b)?;
        }
        ProfileKind::Codex => {
            config::update_codex(home, &profile.value_a, &profile.value_b)?;
        }
    }
    Ok(())
}

pub fn list_profile_names(home: &Path) -> Result<Vec<String>, String> {
    let dir = profiles_dir(home);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in
        fs::read_dir(&dir).map_err(|err| format!("failed reading {}: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| format!("failed reading directory entry: {err}"))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
                names.push(stem.to_owned());
            }
        }
    }

    names.sort();
    Ok(names)
}

pub fn list_profiles_by_kind(home: &Path, kind: ProfileKind) -> Result<Vec<String>, String> {
    let prefix = profile_prefix(kind);
    Ok(list_profile_names(home)?
        .into_iter()
        .filter(|name| name.starts_with(prefix))
        .collect())
}

pub fn profile_prefix(kind: ProfileKind) -> &'static str {
    match kind {
        ProfileKind::ClaudeCode => "cc-",
        ProfileKind::Codex => "cx-",
    }
}

fn render_profile_document(profile: &SavedProfile) -> String {
    let mut document = DocumentMut::new();
    document["profile"]["kind"] = value(match profile.kind {
        ProfileKind::ClaudeCode => "claude-code",
        ProfileKind::Codex => "codex",
    });
    document["profile"]["name"] = value(profile.name.as_str());
    match profile.kind {
        ProfileKind::ClaudeCode => {
            document["claude"]["anthropic_auth_token"] = value(profile.value_a.as_str());
            document["claude"]["anthropic_base_url"] = value(profile.value_b.as_str());
        }
        ProfileKind::Codex => {
            document["codex"]["base_url"] = value(profile.value_a.as_str());
            document["codex"]["openai_api_key"] = value(profile.value_b.as_str());
        }
    }
    document.to_string()
}

fn verify_profile_document(
    document: &DocumentMut,
    profile: &SavedProfile,
    path: &Path,
) -> Result<(), String> {
    let actual_kind = get_toml_string(document, &["profile", "kind"])?;
    let expected_kind = match profile.kind {
        ProfileKind::ClaudeCode => "claude-code",
        ProfileKind::Codex => "codex",
    };
    if actual_kind != expected_kind {
        return Err(format!(
            "verification failed for profile.kind in {}",
            path.display()
        ));
    }

    let checks: Vec<([&str; 2], &str)> = match profile.kind {
        ProfileKind::ClaudeCode => vec![
            (["claude", "anthropic_auth_token"], profile.value_a.as_str()),
            (["claude", "anthropic_base_url"], profile.value_b.as_str()),
        ],
        ProfileKind::Codex => vec![
            (["codex", "base_url"], profile.value_a.as_str()),
            (["codex", "openai_api_key"], profile.value_b.as_str()),
        ],
    };

    for (segments, expected) in checks {
        let actual = get_toml_string(document, &segments)?;
        if actual != expected {
            return Err(format!(
                "verification failed for {} in {}",
                segments.join("."),
                path.display()
            ));
        }
    }

    Ok(())
}

fn read_saved_profile(home: &Path, name: &str) -> Result<SavedProfile, String> {
    let profile_path = profiles_dir(home).join(format!("{name}.toml"));
    let document = read_file(&profile_path)?
        .parse::<DocumentMut>()
        .map_err(|err| format!("failed parsing {} as TOML: {err}", profile_path.display()))?;

    let kind = match get_toml_string(&document, &["profile", "kind"])? {
        "claude-code" => ProfileKind::ClaudeCode,
        "codex" => ProfileKind::Codex,
        other => {
            return Err(format!(
                "unsupported profile kind {other} in {}",
                profile_path.display()
            ));
        }
    };

    let (value_a, value_b) = match kind {
        ProfileKind::ClaudeCode => (
            get_toml_string(&document, &["claude", "anthropic_auth_token"])?.to_owned(),
            get_toml_string(&document, &["claude", "anthropic_base_url"])?.to_owned(),
        ),
        ProfileKind::Codex => (
            get_toml_string(&document, &["codex", "base_url"])?.to_owned(),
            get_toml_string(&document, &["codex", "openai_api_key"])?.to_owned(),
        ),
    };

    Ok(SavedProfile {
        kind,
        name: name.to_owned(),
        value_a,
        value_b,
    })
}

fn get_toml_string<'a>(document: &'a DocumentMut, segments: &[&str]) -> Result<&'a str, String> {
    let mut current = document.as_item();
    for segment in segments {
        current = current
            .get(*segment)
            .ok_or_else(|| format!("missing TOML key {}", segments.join(".")))?;
    }
    current
        .as_value()
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("expected string at TOML key {}", segments.join(".")))
}

fn profiles_dir(home: &Path) -> PathBuf {
    home.join(CXC_PROFILES_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_claude_profile_document() {
        let profile = SavedProfile {
            kind: ProfileKind::ClaudeCode,
            name: "cc-work".to_owned(),
            value_a: "claude-token".to_owned(),
            value_b: "https://claude.example".to_owned(),
        };

        let document = render_profile_document(&profile)
            .parse::<DocumentMut>()
            .unwrap();
        verify_profile_document(&document, &profile, Path::new("/tmp/profile.toml")).unwrap();
    }

    #[test]
    fn renders_codex_profile_document() {
        let profile = SavedProfile {
            kind: ProfileKind::Codex,
            name: "cx-work".to_owned(),
            value_a: "https://codex.example".to_owned(),
            value_b: "openai-key".to_owned(),
        };

        let document = render_profile_document(&profile)
            .parse::<DocumentMut>()
            .unwrap();
        verify_profile_document(&document, &profile, Path::new("/tmp/profile.toml")).unwrap();
    }

    #[test]
    fn returns_expected_prefix() {
        assert_eq!(profile_prefix(ProfileKind::ClaudeCode), "cc-");
        assert_eq!(profile_prefix(ProfileKind::Codex), "cx-");
    }
}
