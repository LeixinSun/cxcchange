use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

const CLAUDE_SETTINGS: &str = ".claude/settings.json";
const CODEX_CONFIG: &str = ".codex/config.toml";
const CODEX_AUTH: &str = ".codex/auth.json";
const MIRROR_SECTION: &str = "model_providers.mirror";

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mode = parse_mode(env::args().skip(1))?;
    let home = home_dir()?;

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

    let original = read_file(&settings_path)?;
    let updated = replace_json_string_at_path(&original, &["env", "ANTHROPIC_AUTH_TOKEN"], &token)?;
    let updated = replace_json_string_at_path(&updated, &["env", "ANTHROPIC_BASE_URL"], &base_url)?;

    atomic_write(&settings_path, &updated)?;

    let written = read_file(&settings_path)?;
    verify_json_string(&written, &["env", "ANTHROPIC_AUTH_TOKEN"], &token)?;
    verify_json_string(&written, &["env", "ANTHROPIC_BASE_URL"], &base_url)?;

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

    let config_original = read_file(&config_path)?;
    let config_updated =
        replace_toml_string_in_section(&config_original, MIRROR_SECTION, "base_url", &base_url)?;
    atomic_write(&config_path, &config_updated)?;

    let auth_original = read_file(&auth_path)?;
    let auth_updated = replace_json_string_at_path(&auth_original, &["OPENAI_API_KEY"], &api_key)?;
    atomic_write(&auth_path, &auth_updated)?;

    let written_config = read_file(&config_path)?;
    verify_toml_string(&written_config, MIRROR_SECTION, "base_url", &base_url)?;

    let written_auth = read_file(&auth_path)?;
    verify_json_string(&written_auth, &["OPENAI_API_KEY"], &api_key)?;

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

    fs::rename(&tmp_path, path)
        .map_err(|err| format!("failed replacing {}: {err}", path.display()))?;

    sync_directory(parent)?;
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), String> {
    let dir =
        File::open(path).map_err(|err| format!("failed opening {}: {err}", path.display()))?;
    dir.sync_all()
        .map_err(|err| format!("failed syncing {}: {err}", path.display()))
}

fn verify_json_string(content: &str, path: &[&str], expected: &str) -> Result<(), String> {
    let actual = json_string_at_path(content, path)?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "verification failed for {}: expected written value",
            path.join(".")
        ))
    }
}

fn json_string_at_path(content: &str, path: &[&str]) -> Result<String, String> {
    let span = find_json_string_value_span(content, path)?;
    decode_json_string_literal(&content[span.start..span.end])
}

fn replace_json_string_at_path(
    content: &str,
    path: &[&str],
    new_value: &str,
) -> Result<String, String> {
    let span = find_json_string_value_span(content, path)?;
    let mut updated = String::with_capacity(content.len() + new_value.len());
    updated.push_str(&content[..span.start]);
    updated.push_str(&encode_json_string(new_value));
    updated.push_str(&content[span.end..]);
    Ok(updated)
}

fn find_json_string_value_span(
    content: &str,
    path: &[&str],
) -> Result<std::ops::Range<usize>, String> {
    let bytes = content.as_bytes();
    validate_json_document(bytes)?;

    let mut index = skip_json_ws(bytes, 0);
    find_json_value_span(bytes, &mut index, path)
}

fn validate_json_document(bytes: &[u8]) -> Result<(), String> {
    let mut index = skip_json_ws(bytes, 0);
    index = skip_json_value(bytes, index)?;
    index = skip_json_ws(bytes, index);
    if index == bytes.len() {
        Ok(())
    } else {
        Err("invalid JSON: trailing content".to_owned())
    }
}

fn find_json_value_span(
    bytes: &[u8],
    index: &mut usize,
    path: &[&str],
) -> Result<std::ops::Range<usize>, String> {
    match current_byte(bytes, *index) {
        Some(b'{') => find_json_in_object(bytes, index, path),
        Some(_) => Err(format!("missing JSON object path {}", path.join("."))),
        None => Err("invalid JSON: unexpected end of input".to_owned()),
    }
}

fn find_json_in_object(
    bytes: &[u8],
    index: &mut usize,
    path: &[&str],
) -> Result<std::ops::Range<usize>, String> {
    if path.is_empty() {
        return Err("empty JSON path".to_owned());
    }

    expect_byte(bytes, index, b'{', "object start")?;
    *index = skip_json_ws(bytes, *index);

    if current_byte(bytes, *index) == Some(b'}') {
        *index += 1;
        return Err(format!("missing JSON key {}", path.join(".")));
    }

    loop {
        *index = skip_json_ws(bytes, *index);
        let (key, key_end) = parse_json_string(bytes, *index)?;
        *index = skip_json_ws(bytes, key_end);
        expect_byte(bytes, index, b':', "colon after key")?;
        *index = skip_json_ws(bytes, *index);

        if key == path[0] {
            if path.len() == 1 {
                let start = *index;
                let end = skip_json_string_value(bytes, *index)?;
                *index = end;
                return Ok(start..end);
            }

            let span = match current_byte(bytes, *index) {
                Some(b'{') => find_json_in_object(bytes, index, &path[1..])?,
                _ => {
                    return Err(format!(
                        "expected object at {}",
                        path[..path.len() - 1].join(".")
                    ));
                }
            };
            return Ok(span);
        }

        *index = skip_json_value(bytes, *index)?;
        *index = skip_json_ws(bytes, *index);

        match current_byte(bytes, *index) {
            Some(b',') => {
                *index += 1;
            }
            Some(b'}') => {
                *index += 1;
                return Err(format!("missing JSON key {}", path.join(".")));
            }
            Some(other) => return Err(format!("invalid JSON object separator: {}", other as char)),
            None => return Err("invalid JSON: unexpected end of object".to_owned()),
        }
    }
}

fn skip_json_value(bytes: &[u8], index: usize) -> Result<usize, String> {
    match current_byte(bytes, index) {
        Some(b'"') => skip_json_string_value(bytes, index),
        Some(b'{') => skip_json_object(bytes, index),
        Some(b'[') => skip_json_array(bytes, index),
        Some(b'-' | b'0'..=b'9') => skip_json_number(bytes, index),
        Some(b't') => skip_json_literal(bytes, index, b"true"),
        Some(b'f') => skip_json_literal(bytes, index, b"false"),
        Some(b'n') => skip_json_literal(bytes, index, b"null"),
        Some(other) => Err(format!("invalid JSON value start: {}", other as char)),
        None => Err("invalid JSON: unexpected end of input".to_owned()),
    }
}

fn skip_json_object(bytes: &[u8], mut index: usize) -> Result<usize, String> {
    expect_byte(bytes, &mut index, b'{', "object start")?;
    index = skip_json_ws(bytes, index);

    if current_byte(bytes, index) == Some(b'}') {
        return Ok(index + 1);
    }

    loop {
        let (_, key_end) = parse_json_string(bytes, index)?;
        index = skip_json_ws(bytes, key_end);
        expect_byte(bytes, &mut index, b':', "colon after key")?;
        index = skip_json_ws(bytes, index);
        index = skip_json_value(bytes, index)?;
        index = skip_json_ws(bytes, index);

        match current_byte(bytes, index) {
            Some(b',') => index = skip_json_ws(bytes, index + 1),
            Some(b'}') => return Ok(index + 1),
            Some(other) => return Err(format!("invalid JSON object separator: {}", other as char)),
            None => return Err("invalid JSON: unexpected end of object".to_owned()),
        }
    }
}

fn skip_json_array(bytes: &[u8], mut index: usize) -> Result<usize, String> {
    expect_byte(bytes, &mut index, b'[', "array start")?;
    index = skip_json_ws(bytes, index);

    if current_byte(bytes, index) == Some(b']') {
        return Ok(index + 1);
    }

    loop {
        index = skip_json_value(bytes, index)?;
        index = skip_json_ws(bytes, index);

        match current_byte(bytes, index) {
            Some(b',') => index = skip_json_ws(bytes, index + 1),
            Some(b']') => return Ok(index + 1),
            Some(other) => return Err(format!("invalid JSON array separator: {}", other as char)),
            None => return Err("invalid JSON: unexpected end of array".to_owned()),
        }
    }
}

fn skip_json_string_value(bytes: &[u8], index: usize) -> Result<usize, String> {
    let (_, end) = parse_json_string(bytes, index)?;
    Ok(end)
}

fn parse_json_string(bytes: &[u8], index: usize) -> Result<(String, usize), String> {
    let start = index;
    if current_byte(bytes, start) != Some(b'"') {
        return Err("invalid JSON string".to_owned());
    }

    let mut i = start + 1;
    let mut decoded = String::new();

    while let Some(byte) = current_byte(bytes, i) {
        match byte {
            b'"' => return Ok((decoded, i + 1)),
            b'\\' => {
                i += 1;
                let escaped =
                    current_byte(bytes, i).ok_or_else(|| "invalid JSON escape".to_owned())?;
                match escaped {
                    b'"' => decoded.push('"'),
                    b'\\' => decoded.push('\\'),
                    b'/' => decoded.push('/'),
                    b'b' => decoded.push('\u{0008}'),
                    b'f' => decoded.push('\u{000C}'),
                    b'n' => decoded.push('\n'),
                    b'r' => decoded.push('\r'),
                    b't' => decoded.push('\t'),
                    b'u' => {
                        let end = i + 5;
                        if end > bytes.len() {
                            return Err("invalid JSON unicode escape".to_owned());
                        }
                        let code = std::str::from_utf8(&bytes[i + 1..end])
                            .map_err(|_| "invalid JSON unicode escape".to_owned())?;
                        let value = u16::from_str_radix(code, 16)
                            .map_err(|_| "invalid JSON unicode escape".to_owned())?;
                        let ch = char::from_u32(value as u32)
                            .ok_or_else(|| "invalid JSON unicode codepoint".to_owned())?;
                        decoded.push(ch);
                        i = end - 1;
                    }
                    _ => return Err("invalid JSON escape".to_owned()),
                }
            }
            b if b < 0x20 => return Err("invalid JSON control character in string".to_owned()),
            _ => decoded.push(byte as char),
        }
        i += 1;
    }

    Err("unterminated JSON string".to_owned())
}

fn skip_json_literal(bytes: &[u8], index: usize, literal: &[u8]) -> Result<usize, String> {
    let end = index + literal.len();
    if bytes.get(index..end) == Some(literal) {
        Ok(end)
    } else {
        Err("invalid JSON literal".to_owned())
    }
}

fn skip_json_number(bytes: &[u8], mut index: usize) -> Result<usize, String> {
    if current_byte(bytes, index) == Some(b'-') {
        index += 1;
    }

    match current_byte(bytes, index) {
        Some(b'0') => index += 1,
        Some(b'1'..=b'9') => {
            index += 1;
            while matches!(current_byte(bytes, index), Some(b'0'..=b'9')) {
                index += 1;
            }
        }
        _ => return Err("invalid JSON number".to_owned()),
    }

    if current_byte(bytes, index) == Some(b'.') {
        index += 1;
        let start = index;
        while matches!(current_byte(bytes, index), Some(b'0'..=b'9')) {
            index += 1;
        }
        if index == start {
            return Err("invalid JSON number".to_owned());
        }
    }

    if matches!(current_byte(bytes, index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(current_byte(bytes, index), Some(b'+' | b'-')) {
            index += 1;
        }
        let start = index;
        while matches!(current_byte(bytes, index), Some(b'0'..=b'9')) {
            index += 1;
        }
        if index == start {
            return Err("invalid JSON number".to_owned());
        }
    }

    Ok(index)
}

fn skip_json_ws(bytes: &[u8], mut index: usize) -> usize {
    while matches!(
        current_byte(bytes, index),
        Some(b' ' | b'\n' | b'\r' | b'\t')
    ) {
        index += 1;
    }
    index
}

fn current_byte(bytes: &[u8], index: usize) -> Option<u8> {
    bytes.get(index).copied()
}

fn expect_byte(bytes: &[u8], index: &mut usize, expected: u8, context: &str) -> Result<(), String> {
    match current_byte(bytes, *index) {
        Some(actual) if actual == expected => {
            *index += 1;
            Ok(())
        }
        Some(actual) => Err(format!(
            "invalid JSON: expected {} in {}, found {}",
            expected as char, context, actual as char
        )),
        None => Err(format!(
            "invalid JSON: expected {} in {}",
            expected as char, context
        )),
    }
}

fn encode_json_string(value: &str) -> String {
    let mut encoded = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            '\u{0008}' => encoded.push_str("\\b"),
            '\u{000C}' => encoded.push_str("\\f"),
            c if c <= '\u{001F}' => encoded.push_str(&format!("\\u{:04x}", c as u32)),
            c => encoded.push(c),
        }
    }
    encoded.push('"');
    encoded
}

fn decode_json_string_literal(literal: &str) -> Result<String, String> {
    let bytes = literal.as_bytes();
    let (decoded, end) = parse_json_string(bytes, 0)?;
    if end == bytes.len() {
        Ok(decoded)
    } else {
        Err("invalid JSON string literal".to_owned())
    }
}

fn verify_toml_string(
    content: &str,
    section: &str,
    key: &str,
    expected: &str,
) -> Result<(), String> {
    let actual = toml_string_in_section(content, section, key)?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "verification failed for {section}.{key}: expected written value"
        ))
    }
}

fn toml_string_in_section(content: &str, section: &str, key: &str) -> Result<String, String> {
    let (_, value_range) = find_toml_key_line(content, section, key)?;
    parse_toml_string_literal(content[value_range].trim())
}

fn replace_toml_string_in_section(
    content: &str,
    section: &str,
    key: &str,
    new_value: &str,
) -> Result<String, String> {
    let (line_range, value_range) = find_toml_key_line(content, section, key)?;
    let line = &content[line_range.clone()];
    let value_in_line = value_range.start - line_range.start..value_range.end - line_range.start;
    let mut updated_line = String::with_capacity(line.len() + new_value.len());
    updated_line.push_str(&line[..value_in_line.start]);
    updated_line.push_str(&encode_toml_string(new_value));
    updated_line.push_str(&line[value_in_line.end..]);

    let mut updated = String::with_capacity(content.len() + new_value.len());
    updated.push_str(&content[..line_range.start]);
    updated.push_str(&updated_line);
    updated.push_str(&content[line_range.end..]);
    Ok(updated)
}

fn find_toml_key_line(
    content: &str,
    section: &str,
    key: &str,
) -> Result<(std::ops::Range<usize>, std::ops::Range<usize>), String> {
    let mut current_section = String::new();
    let mut offset = 0usize;

    for line in content.split_inclusive('\n') {
        let line_without_newline = line.trim_end_matches('\n');
        let trimmed = line_without_newline.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = trimmed[1..trimmed.len() - 1].trim().to_owned();
        } else if current_section == section && !trimmed.is_empty() && !trimmed.starts_with('#') {
            if let Some((value, relative_range)) =
                parse_toml_key_assignment(line_without_newline, key)?
            {
                let _ = value;
                let line_start = offset;
                let line_end = offset + line_without_newline.len();
                let value_start = line_start + relative_range.start;
                let value_end = line_start + relative_range.end;
                return Ok((line_start..line_end, value_start..value_end));
            }
        }

        offset += line.len();
    }

    Err(format!("missing TOML key {section}.{key}"))
}

fn parse_toml_key_assignment<'a>(
    line: &'a str,
    expected_key: &str,
) -> Result<Option<(&'a str, std::ops::Range<usize>)>, String> {
    let bytes = line.as_bytes();
    let mut index = 0usize;

    while matches!(bytes.get(index), Some(b' ' | b'\t')) {
        index += 1;
    }

    let key_start = index;
    while matches!(
        bytes.get(index),
        Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-')
    ) {
        index += 1;
    }

    if key_start == index {
        return Ok(None);
    }

    let key = &line[key_start..index];
    if key != expected_key {
        return Ok(None);
    }

    while matches!(bytes.get(index), Some(b' ' | b'\t')) {
        index += 1;
    }

    if bytes.get(index) != Some(&b'=') {
        return Err(format!("invalid TOML assignment for key {expected_key}"));
    }
    index += 1;

    while matches!(bytes.get(index), Some(b' ' | b'\t')) {
        index += 1;
    }

    let value_start = index;
    let value_end = scan_toml_basic_string(line, value_start)?;
    Ok(Some((
        &line[value_start..value_end],
        value_start..value_end,
    )))
}

fn scan_toml_basic_string(line: &str, start: usize) -> Result<usize, String> {
    let bytes = line.as_bytes();
    if bytes.get(start) != Some(&b'"') {
        return Err("expected TOML string value".to_owned());
    }

    let mut index = start + 1;
    let mut escaped = false;
    while let Some(byte) = bytes.get(index) {
        if escaped {
            escaped = false;
        } else if *byte == b'\\' {
            escaped = true;
        } else if *byte == b'"' {
            return Ok(index + 1);
        }
        index += 1;
    }

    Err("unterminated TOML string value".to_owned())
}

fn parse_toml_string_literal(literal: &str) -> Result<String, String> {
    let bytes = literal.as_bytes();
    if bytes.first() != Some(&b'"') || bytes.last() != Some(&b'"') {
        return Err("expected TOML string value".to_owned());
    }

    let mut index = 1usize;
    let end = bytes.len() - 1;
    let mut decoded = String::new();

    while index < end {
        match bytes[index] {
            b'\\' => {
                index += 1;
                let escaped = *bytes
                    .get(index)
                    .ok_or_else(|| "invalid TOML escape".to_owned())?;
                match escaped {
                    b'"' => decoded.push('"'),
                    b'\\' => decoded.push('\\'),
                    b'b' => decoded.push('\u{0008}'),
                    b't' => decoded.push('\t'),
                    b'n' => decoded.push('\n'),
                    b'f' => decoded.push('\u{000C}'),
                    b'r' => decoded.push('\r'),
                    b'u' => {
                        let hex_end = index + 5;
                        if hex_end > end + 1 {
                            return Err("invalid TOML unicode escape".to_owned());
                        }
                        let hex = std::str::from_utf8(&bytes[index + 1..hex_end])
                            .map_err(|_| "invalid TOML unicode escape".to_owned())?;
                        let value = u32::from_str_radix(hex, 16)
                            .map_err(|_| "invalid TOML unicode escape".to_owned())?;
                        let ch = char::from_u32(value)
                            .ok_or_else(|| "invalid TOML unicode escape".to_owned())?;
                        decoded.push(ch);
                        index = hex_end - 1;
                    }
                    _ => return Err("invalid TOML escape".to_owned()),
                }
            }
            byte => decoded.push(byte as char),
        }
        index += 1;
    }

    Ok(decoded)
}

fn encode_toml_string(value: &str) -> String {
    let mut encoded = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '\u{0008}' => encoded.push_str("\\b"),
            '\t' => encoded.push_str("\\t"),
            '\n' => encoded.push_str("\\n"),
            '\u{000C}' => encoded.push_str("\\f"),
            '\r' => encoded.push_str("\\r"),
            c if c <= '\u{001F}' => encoded.push_str(&format!("\\u{:04X}", c as u32)),
            c => encoded.push(c),
        }
    }
    encoded.push('"');
    encoded
}

fn home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_nested_json_string() {
        let input = r#"{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "old-token",
    "ANTHROPIC_BASE_URL": "https://old.example"
  },
  "model": "claude-opus"
}"#;

        let updated =
            replace_json_string_at_path(input, &["env", "ANTHROPIC_AUTH_TOKEN"], "new-token")
                .unwrap();

        assert_eq!(
            json_string_at_path(&updated, &["env", "ANTHROPIC_AUTH_TOKEN"]).unwrap(),
            "new-token"
        );
        assert_eq!(
            json_string_at_path(&updated, &["env", "ANTHROPIC_BASE_URL"]).unwrap(),
            "https://old.example"
        );
    }

    #[test]
    fn errors_on_missing_json_key() {
        let input = r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"old-token"}}"#;
        let err = replace_json_string_at_path(input, &["env", "ANTHROPIC_BASE_URL"], "new-url")
            .unwrap_err();
        assert!(err.contains("missing JSON key"));
    }

    #[test]
    fn replaces_top_level_json_string() {
        let input = r#"{"OPENAI_API_KEY":"old-key","other":"value"}"#;
        let updated = replace_json_string_at_path(input, &["OPENAI_API_KEY"], "new-key").unwrap();
        assert_eq!(
            json_string_at_path(&updated, &["OPENAI_API_KEY"]).unwrap(),
            "new-key"
        );
        assert_eq!(json_string_at_path(&updated, &["other"]).unwrap(), "value");
    }

    #[test]
    fn replaces_toml_string_in_target_section_only() {
        let input = r#"base_url = "https://ignored.example"

[model_providers.mirror]
base_url = "https://old.example"
wire_api = "responses"
"#;

        let updated = replace_toml_string_in_section(
            input,
            "model_providers.mirror",
            "base_url",
            "https://new.example",
        )
        .unwrap();

        assert_eq!(
            toml_string_in_section(&updated, "model_providers.mirror", "base_url").unwrap(),
            "https://new.example"
        );
        assert!(updated.contains("base_url = \"https://ignored.example\""));
    }

    #[test]
    fn errors_on_missing_toml_key() {
        let input = "[model_providers.mirror]\nwire_api = \"responses\"\n";
        let err = replace_toml_string_in_section(
            input,
            "model_providers.mirror",
            "base_url",
            "https://new.example",
        )
        .unwrap_err();
        assert!(err.contains("missing TOML key"));
    }

    #[test]
    fn rejects_non_string_toml_assignment() {
        let input = "[model_providers.mirror]\nbase_url = 1\n";
        let err = replace_toml_string_in_section(
            input,
            "model_providers.mirror",
            "base_url",
            "https://new.example",
        )
        .unwrap_err();
        assert!(err.contains("expected TOML string value"));
    }

    #[test]
    fn atomic_write_replaces_file_contents() {
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
