use std::path::Path;

use serde_json::Value as JsonValue;
use toml_edit::{DocumentMut, Item, value};

use crate::fsutil::{atomic_write, read_file};

const CLAUDE_SETTINGS: &str = ".claude/settings.json";
const CODEX_CONFIG: &str = ".codex/config.toml";
const CODEX_AUTH: &str = ".codex/auth.json";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentConfig {
    pub claude_token: String,
    pub claude_base_url: String,
    pub codex_base_url: String,
    pub codex_api_key: String,
}

pub fn update_claude_code(home: &Path, token: &str, base_url: &str) -> Result<(), String> {
    let settings_path = home.join(CLAUDE_SETTINGS);
    update_json_file(
        &settings_path,
        &[
            (vec!["env", "ANTHROPIC_AUTH_TOKEN"], token),
            (vec!["env", "ANTHROPIC_BASE_URL"], base_url),
        ],
    )
}

pub fn update_codex(home: &Path, base_url: &str, api_key: &str) -> Result<(), String> {
    let config_path = home.join(CODEX_CONFIG);
    let auth_path = home.join(CODEX_AUTH);

    update_toml_string(
        &config_path,
        &["model_providers", "mirror", "base_url"],
        base_url,
    )?;
    update_json_file(&auth_path, &[(vec!["OPENAI_API_KEY"], api_key)])?;
    Ok(())
}

pub fn read_current_config(home: &Path) -> Result<CurrentConfig, String> {
    let settings_path = home.join(CLAUDE_SETTINGS);
    let config_path = home.join(CODEX_CONFIG);
    let auth_path = home.join(CODEX_AUTH);

    let settings_doc: JsonValue = serde_json::from_str(&read_file(&settings_path)?)
        .map_err(|err| format!("failed parsing {} as JSON: {err}", settings_path.display()))?;
    let config_doc = read_file(&config_path)?
        .parse::<DocumentMut>()
        .map_err(|err| format!("failed parsing {} as TOML: {err}", config_path.display()))?;
    let auth_doc: JsonValue = serde_json::from_str(&read_file(&auth_path)?)
        .map_err(|err| format!("failed parsing {} as JSON: {err}", auth_path.display()))?;

    Ok(CurrentConfig {
        claude_token: get_json_string(&settings_doc, &["env", "ANTHROPIC_AUTH_TOKEN"])?.to_owned(),
        claude_base_url: get_json_string(&settings_doc, &["env", "ANTHROPIC_BASE_URL"])?.to_owned(),
        codex_base_url: get_toml_string(&config_doc, &["model_providers", "mirror", "base_url"])?
            .to_owned(),
        codex_api_key: get_json_string(&auth_doc, &["OPENAI_API_KEY"])?.to_owned(),
    })
}

fn update_json_file(path: &Path, updates: &[(Vec<&str>, &str)]) -> Result<(), String> {
    let original = read_file(path)?;
    let mut document: JsonValue = serde_json::from_str(&original)
        .map_err(|err| format!("failed parsing {} as JSON: {err}", path.display()))?;

    for (segments, new_value) in updates {
        set_existing_json_string(&mut document, segments, new_value)?;
    }

    let rendered = serde_json::to_string_pretty(&document)
        .map_err(|err| format!("failed rendering JSON for {}: {err}", path.display()))?;
    atomic_write(path, &(rendered + "\n"))?;

    let written = read_file(path)?;
    let written_document: JsonValue = serde_json::from_str(&written)
        .map_err(|err| format!("failed parsing written JSON {}: {err}", path.display()))?;

    for (segments, expected) in updates {
        let actual = get_json_string(&written_document, segments)?;
        if actual != *expected {
            return Err(format!(
                "verification failed for {} in {}",
                segments.join("."),
                path.display()
            ));
        }
    }

    Ok(())
}

fn update_toml_string(path: &Path, segments: &[&str], new_value: &str) -> Result<(), String> {
    let original = read_file(path)?;
    let mut document = original
        .parse::<DocumentMut>()
        .map_err(|err| format!("failed parsing {} as TOML: {err}", path.display()))?;

    set_existing_toml_string(&mut document, segments, new_value)?;
    atomic_write(path, &document.to_string())?;

    let written = read_file(path)?;
    let written_document = written
        .parse::<DocumentMut>()
        .map_err(|err| format!("failed parsing written TOML {}: {err}", path.display()))?;
    let actual = get_toml_string(&written_document, segments)?;
    if actual != new_value {
        return Err(format!(
            "verification failed for {} in {}",
            segments.join("."),
            path.display()
        ));
    }

    Ok(())
}

fn set_existing_json_string(
    root: &mut JsonValue,
    path: &[&str],
    new_value: &str,
) -> Result<(), String> {
    let Some((last, parents)) = path.split_last() else {
        return Err("empty JSON path".to_owned());
    };

    let mut current = root;
    for segment in parents {
        current = current
            .get_mut(*segment)
            .ok_or_else(|| format!("missing JSON key {}", path.join(".")))?;
    }

    let value = current
        .get_mut(*last)
        .ok_or_else(|| format!("missing JSON key {}", path.join(".")))?;

    if !value.is_string() {
        return Err(format!("expected string at JSON key {}", path.join(".")));
    }

    *value = JsonValue::String(new_value.to_owned());
    Ok(())
}

fn set_existing_toml_string(
    document: &mut DocumentMut,
    segments: &[&str],
    new_value: &str,
) -> Result<(), String> {
    let Some((last, parents)) = segments.split_last() else {
        return Err("empty TOML path".to_owned());
    };

    let mut current: &mut Item = document.as_item_mut();
    for segment in parents {
        current = current
            .get_mut(*segment)
            .ok_or_else(|| format!("missing TOML key {}", segments.join(".")))?;
    }

    let Some(target) = current.get_mut(*last) else {
        return Err(format!("missing TOML key {}", segments.join(".")));
    };

    if target.is_none() {
        return Err(format!("missing TOML key {}", segments.join(".")));
    }

    if target.as_value().and_then(|value| value.as_str()).is_none() {
        return Err(format!(
            "expected string at TOML key {}",
            segments.join(".")
        ));
    }

    *target = value(new_value);
    Ok(())
}

fn get_json_string<'a>(root: &'a JsonValue, path: &[&str]) -> Result<&'a str, String> {
    let mut current = root;
    for segment in path {
        current = current
            .get(*segment)
            .ok_or_else(|| format!("missing JSON key {}", path.join(".")))?;
    }

    current
        .as_str()
        .ok_or_else(|| format!("expected string at JSON key {}", path.join(".")))
}

fn get_toml_string<'a>(document: &'a DocumentMut, segments: &[&str]) -> Result<&'a str, String> {
    let mut current: &Item = document.as_item();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_nested_json_strings() {
        let mut document: JsonValue = serde_json::from_str(
            r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"old-token","ANTHROPIC_BASE_URL":"https://old.example"}}"#,
        )
        .unwrap();

        set_existing_json_string(&mut document, &["env", "ANTHROPIC_AUTH_TOKEN"], "new-token")
            .unwrap();

        assert_eq!(
            get_json_string(&document, &["env", "ANTHROPIC_AUTH_TOKEN"]).unwrap(),
            "new-token"
        );
        assert_eq!(
            get_json_string(&document, &["env", "ANTHROPIC_BASE_URL"]).unwrap(),
            "https://old.example"
        );
    }

    #[test]
    fn errors_on_missing_json_key() {
        let mut document: JsonValue =
            serde_json::from_str(r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"old-token"}}"#).unwrap();
        let err =
            set_existing_json_string(&mut document, &["env", "ANTHROPIC_BASE_URL"], "new-url")
                .unwrap_err();
        assert!(err.contains("missing JSON key"));
    }

    #[test]
    fn updates_target_toml_string() {
        let mut document = r#"base_url = "https://ignored.example"

[model_providers.mirror]
base_url = "https://old.example"
wire_api = "responses"
"#
        .parse::<DocumentMut>()
        .unwrap();

        set_existing_toml_string(
            &mut document,
            &["model_providers", "mirror", "base_url"],
            "https://new.example",
        )
        .unwrap();

        assert_eq!(
            get_toml_string(&document, &["model_providers", "mirror", "base_url"]).unwrap(),
            "https://new.example"
        );
        assert!(document.to_string().contains("https://ignored.example"));
    }

    #[test]
    fn errors_on_missing_toml_key() {
        let mut document = "[model_providers.mirror]\nwire_api = \"responses\"\n"
            .parse::<DocumentMut>()
            .unwrap();
        let err = set_existing_toml_string(
            &mut document,
            &["model_providers", "mirror", "base_url"],
            "https://new.example",
        )
        .unwrap_err();
        assert!(err.contains("missing TOML key"));
    }

    #[test]
    fn errors_on_non_string_toml_value() {
        let mut document = "[model_providers.mirror]\nbase_url = 1\n"
            .parse::<DocumentMut>()
            .unwrap();
        let err = set_existing_toml_string(
            &mut document,
            &["model_providers", "mirror", "base_url"],
            "https://new.example",
        )
        .unwrap_err();
        assert!(err.contains("expected string"));
    }
}
