use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value as JsonValue;
use toml_edit::{DocumentMut, Item, value};

const CLAUDE_SETTINGS: &str = ".claude/settings.json";
const CODEX_CONFIG: &str = ".codex/config.toml";
const CODEX_AUTH: &str = ".codex/auth.json";

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mode = parse_mode(env::args().skip(1))?;
    let home = dirs::home_dir().ok_or_else(|| "failed to resolve home directory".to_owned())?;

    match mode {
        Mode::ClaudeCode => update_claude_code(&home),
        Mode::Codex => update_codex(&home),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    ClaudeCode,
    Codex,
}

fn parse_mode<I>(args: I) -> Result<Mode, String>
where
    I: IntoIterator<Item = String>,
{
    let mut selected = None;

    for arg in args {
        let mode = match arg.as_str() {
            "--cc" => Mode::ClaudeCode,
            "--cx" => Mode::Codex,
            "--help" | "-h" => {
                print_usage();
                process::exit(0);
            }
            _ => return Err(format!("unsupported argument: {arg}\n{}", usage_text())),
        };

        if selected.replace(mode).is_some() {
            return Err(format!("choose exactly one mode\n{}", usage_text()));
        }
    }

    selected.ok_or_else(|| format!("choose exactly one mode\n{}", usage_text()))
}

fn update_claude_code(home: &Path) -> Result<(), String> {
    let settings_path = home.join(CLAUDE_SETTINGS);
    println!("Updating {}.", settings_path.display());

    let token = prompt_value("ANTHROPIC_AUTH_TOKEN")?;
    let base_url = prompt_value("ANTHROPIC_BASE_URL")?;

    update_json_file(
        &settings_path,
        &[
            (vec!["env", "ANTHROPIC_AUTH_TOKEN"], token.as_str()),
            (vec!["env", "ANTHROPIC_BASE_URL"], base_url.as_str()),
        ],
    )?;

    println!(
        "Updated {} and {} in {}.",
        "env.ANTHROPIC_AUTH_TOKEN",
        "env.ANTHROPIC_BASE_URL",
        settings_path.display()
    );
    Ok(())
}

fn update_codex(home: &Path) -> Result<(), String> {
    let config_path = home.join(CODEX_CONFIG);
    let auth_path = home.join(CODEX_AUTH);
    println!(
        "Updating {} and {}.",
        config_path.display(),
        auth_path.display()
    );

    let base_url = prompt_value("base_url")?;
    let api_key = prompt_value("OPENAI_API_KEY")?;

    update_toml_string(
        &config_path,
        &["model_providers", "mirror", "base_url"],
        &base_url,
    )?;
    update_json_file(&auth_path, &[(vec!["OPENAI_API_KEY"], api_key.as_str())])?;

    println!(
        "Updated {} in {}.",
        "model_providers.mirror.base_url",
        config_path.display()
    );
    println!("Updated {} in {}.", "OPENAI_API_KEY", auth_path.display());
    Ok(())
}

fn usage_text() -> &'static str {
    "Usage: cxc --cc | --cx"
}

fn print_usage() {
    println!("{}", usage_text());
}

fn prompt_value(label: &str) -> Result<String, String> {
    print!("{label}: ");
    io::stdout().flush().map_err(|err| err.to_string())?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|err| format!("failed reading {label}: {err}"))?;

    let trimmed = input.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        return Err(format!("{label} cannot be empty"));
    }

    Ok(trimmed.to_owned())
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

fn read_file(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("failed reading {}: {err}", path.display()))
}

fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("missing parent directory for {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid filename for {}", path.display()))?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_nanos();
    let tmp_path = parent.join(format!(".{name}.tmp-{}-{stamp}", process::id()));

    let mut tmp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .map_err(|err| format!("failed creating {}: {err}", tmp_path.display()))?;

    if let Ok(metadata) = fs::metadata(path) {
        tmp_file
            .set_permissions(metadata.permissions())
            .map_err(|err| {
                format!(
                    "failed setting permissions on {}: {err}",
                    tmp_path.display()
                )
            })?;
    }

    tmp_file
        .write_all(content.as_bytes())
        .map_err(|err| format!("failed writing {}: {err}", tmp_path.display()))?;
    tmp_file
        .sync_all()
        .map_err(|err| format!("failed syncing {}: {err}", tmp_path.display()))?;
    drop(tmp_file);

    replace_file(&tmp_path, path)?;
    sync_directory(parent)?;
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, target: &Path) -> Result<(), String> {
    fs::rename(source, target)
        .map_err(|err| format!("failed replacing {}: {err}", target.display()))
}

#[cfg(windows)]
fn replace_file(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source_wide: Vec<u16> = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let target_wide: Vec<u16> = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            target_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    if result == 0 {
        let err = std::io::Error::last_os_error();
        return Err(format!("failed replacing {}: {err}", target.display()));
    }

    Ok(())
}

#[cfg(not(windows))]
fn sync_directory(path: &Path) -> Result<(), String> {
    let dir =
        File::open(path).map_err(|err| format!("failed opening {}: {err}", path.display()))?;
    dir.sync_all()
        .map_err(|err| format!("failed syncing {}: {err}", path.display()))
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> Result<(), String> {
    Ok(())
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

    #[test]
    fn atomic_write_replaces_contents() {
        let root = env::temp_dir().join(format!(
            "cxc-test-{}-{}",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("sample.txt");
        fs::write(&file_path, "old").unwrap();

        atomic_write(&file_path, "new").unwrap();

        assert_eq!(fs::read_to_string(&file_path).unwrap(), "new");

        fs::remove_file(&file_path).unwrap();
        fs::remove_dir(&root).unwrap();
    }
}
