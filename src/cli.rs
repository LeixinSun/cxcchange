#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    ClaudeCode,
    Codex,
    Current,
    Save { name: String },
    Use { name: Option<String> },
    List,
    Completion { shell: String },
    InstallCompletion { shell: String },
    CompleteProfiles { prefix: String },
    Help,
}

pub fn parse_command<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    match args.as_slice() {
        [flag] if flag == "--cc" => Ok(Command::ClaudeCode),
        [flag] if flag == "--cx" => Ok(Command::Codex),
        [cmd] if cmd == "current" => Ok(Command::Current),
        [cmd] if cmd == "list" => Ok(Command::List),
        [cmd, name] if cmd == "save" => {
            validate_profile_name(name)?;
            Ok(Command::Save { name: name.clone() })
        }
        [cmd] if cmd == "use" => Ok(Command::Use { name: None }),
        [cmd, name] if cmd == "use" => {
            validate_stored_profile_name(name)?;
            Ok(Command::Use {
                name: Some(name.clone()),
            })
        }
        [cmd, action, shell] if cmd == "completion" && action == "install" => {
            Ok(Command::InstallCompletion {
                shell: shell.clone(),
            })
        }
        [cmd, shell] if cmd == "completion" => Ok(Command::Completion {
            shell: shell.clone(),
        }),
        [cmd, prefix] if cmd == "__complete_profiles" => Ok(Command::CompleteProfiles {
            prefix: prefix.clone(),
        }),
        [flag] if flag == "--help" || flag == "-h" => Ok(Command::Help),
        [] => Err(format!("choose a command\n{}", usage_text())),
        _ => Err(format!("unsupported arguments\n{}", usage_text())),
    }
}

pub fn usage_text() -> &'static str {
    "Usage:
  cxc --cc
  cxc --cx
  cxc current
  cxc save <name>
  cxc use [cc-name|cx-name]
  cxc list
  cxc completion <zsh|bash>
  cxc completion install <zsh|bash>"
}

pub fn validate_profile_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("profile name cannot be empty".to_owned());
    }
    if name.contains(['/', '\\']) {
        return Err("profile name cannot contain path separators".to_owned());
    }
    Ok(())
}

pub fn validate_stored_profile_name(name: &str) -> Result<(), String> {
    validate_profile_name(name)?;
    if !(name.starts_with("cc-") || name.starts_with("cx-")) {
        return Err("profile name must start with cc- or cx-".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_save_use_and_list_commands() {
        assert_eq!(
            parse_command(vec!["current".to_owned()]).unwrap(),
            Command::Current
        );
        assert_eq!(
            parse_command(vec!["list".to_owned()]).unwrap(),
            Command::List
        );
        assert_eq!(
            parse_command(vec!["save".to_owned(), "work".to_owned()]).unwrap(),
            Command::Save {
                name: "work".to_owned()
            }
        );
        assert_eq!(
            parse_command(vec!["use".to_owned(), "cc-work".to_owned()]).unwrap(),
            Command::Use {
                name: Some("cc-work".to_owned())
            }
        );
        assert_eq!(
            parse_command(vec!["use".to_owned()]).unwrap(),
            Command::Use { name: None }
        );
    }

    #[test]
    fn rejects_invalid_profile_names() {
        let save_err = parse_command(vec!["save".to_owned(), "bad/name".to_owned()]).unwrap_err();
        assert!(save_err.contains("path separators"));

        let use_err = parse_command(vec!["use".to_owned(), "work".to_owned()]).unwrap_err();
        assert!(use_err.contains("cc- or cx-"));
    }
}
