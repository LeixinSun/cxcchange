use std::io::{self, Write};

pub fn prompt_value(label: &str) -> Result<String, String> {
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
