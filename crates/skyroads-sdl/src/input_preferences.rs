use std::fs;
use std::path::Path;

use skyroads_core::{InputTuning, SensitivityPercent};

type Result<T> = std::result::Result<T, String>;

const MOUSE_SENSITIVITY_KEY: &str = "mouse_sensitivity";
const CONTROLLER_SENSITIVITY_KEY: &str = "controller_sensitivity";

pub fn load(path: &Path) -> Result<InputTuning> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(InputTuning::default());
        }
        Err(error) => {
            return Err(format!(
                "could not read input preferences from {}: {error}",
                path.display()
            ));
        }
    };

    parse(&contents)
}

pub fn save(path: &Path, tuning: InputTuning) -> Result<()> {
    fs::write(path, format!("{}\n", encode(tuning))).map_err(|error| {
        format!(
            "could not save input preferences to {}: {error}",
            path.display()
        )
    })
}

fn parse(contents: &str) -> Result<InputTuning> {
    let mut mouse_sensitivity = None;
    let mut controller_sensitivity = None;

    for (line_index, line) in contents.lines().enumerate() {
        let line_number = line_index + 1;
        let (key, value) = parse_line(line, line_number)?;

        match key {
            MOUSE_SENSITIVITY_KEY => {
                parse_unique_sensitivity(&mut mouse_sensitivity, key, value)?;
            }
            CONTROLLER_SENSITIVITY_KEY => {
                parse_unique_sensitivity(&mut controller_sensitivity, key, value)?;
            }
            _ => return Err(format!("input preferences contain unknown key '{key}'")),
        }
    }

    let mouse_sensitivity = require_key(mouse_sensitivity, MOUSE_SENSITIVITY_KEY)?;
    let controller_sensitivity = require_key(controller_sensitivity, CONTROLLER_SENSITIVITY_KEY)?;

    Ok(InputTuning::new(mouse_sensitivity, controller_sensitivity))
}

fn parse_line(line: &str, line_number: usize) -> Result<(&str, &str)> {
    let Some((key, value)) = line.split_once('=') else {
        return Err(format!(
            "input preferences line {line_number} must be 'key=value'"
        ));
    };

    let has_extra_separator = value.contains('=');
    let has_whitespace =
        key.chars().any(char::is_whitespace) || value.chars().any(char::is_whitespace);
    let has_empty_field = key.is_empty() || value.is_empty();
    if has_extra_separator || has_whitespace || has_empty_field {
        return Err(format!(
            "input preferences line {line_number} must be 'key=value' without spaces"
        ));
    }

    Ok((key, value))
}

fn parse_sensitivity(key: &str, value: &str) -> Result<SensitivityPercent> {
    if !value.chars().all(|character| character.is_ascii_digit()) {
        return Err(format!(
            "input preference '{key}' must be an integer percentage"
        ));
    }

    let percent = value
        .parse::<u64>()
        .map_err(|_| format!("input preference '{key}' must be an integer percentage"))?;
    let percent = u16::try_from(percent).map_err(|_| {
        format!(
            "input preference '{key}' is invalid: sensitivity must be 50% through 200% in 5% steps, got {percent}%"
        )
    })?;

    SensitivityPercent::new(percent)
        .map_err(|error| format!("input preference '{key}' is invalid: {error}"))
}

fn parse_unique_sensitivity(
    destination: &mut Option<SensitivityPercent>,
    key: &str,
    value: &str,
) -> Result<()> {
    if destination.is_some() {
        return Err(format!("input preferences contain duplicate key '{key}'"));
    }

    *destination = Some(parse_sensitivity(key, value)?);
    Ok(())
}

fn require_key(value: Option<SensitivityPercent>, key: &str) -> Result<SensitivityPercent> {
    value.ok_or_else(|| format!("input preferences are missing required key '{key}'"))
}

fn encode(tuning: InputTuning) -> String {
    format!(
        "{MOUSE_SENSITIVITY_KEY}={}\n{CONTROLLER_SENSITIVITY_KEY}={}",
        tuning.mouse_sensitivity().percent(),
        tuning.controller_sensitivity().percent()
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use skyroads_core::{InputTuning, SensitivityPercent};

    use super::{encode, load, parse, save};

    #[test]
    fn settings_round_trip_in_the_canonical_two_key_format() {
        let tuning = InputTuning::new(sensitivity(135), sensitivity(175));

        assert_eq!(
            encode(tuning),
            "mouse_sensitivity=135\ncontroller_sensitivity=175"
        );
        assert_eq!(parse(&encode(tuning)).unwrap(), tuning);
    }

    #[test]
    fn missing_file_returns_defaults_without_creating_a_file() {
        let path = unique_temp_path("missing");

        assert!(!path.exists());
        assert_eq!(load(&path).unwrap(), InputTuning::default());
        assert!(!path.exists());
    }

    #[test]
    fn malformed_lines_are_rejected() {
        for (contents, expected_message) in [
            (
                "mouse_sensitivity 100\ncontroller_sensitivity=100",
                "input preferences line 1 must be 'key=value'",
            ),
            (
                "mouse_sensitivity =100\ncontroller_sensitivity=100",
                "input preferences line 1 must be 'key=value' without spaces",
            ),
            (
                "mouse_sensitivity=100=extra\ncontroller_sensitivity=100",
                "input preferences line 1 must be 'key=value' without spaces",
            ),
            (
                "mouse_sensitivity=fast\ncontroller_sensitivity=100",
                "input preference 'mouse_sensitivity' must be an integer percentage",
            ),
        ] {
            assert_eq!(parse(contents).unwrap_err(), expected_message);
        }
    }

    #[test]
    fn duplicate_unknown_and_missing_keys_are_rejected() {
        for (contents, expected_message) in [
            (
                "mouse_sensitivity=100\nmouse_sensitivity=105\ncontroller_sensitivity=100",
                "input preferences contain duplicate key 'mouse_sensitivity'",
            ),
            (
                "mouse_sensitivity=100\ncontroller_sensitivity=100\nprofile=custom",
                "input preferences contain unknown key 'profile'",
            ),
            (
                "controller_sensitivity=100",
                "input preferences are missing required key 'mouse_sensitivity'",
            ),
            (
                "mouse_sensitivity=100",
                "input preferences are missing required key 'controller_sensitivity'",
            ),
        ] {
            assert_eq!(parse(contents).unwrap_err(), expected_message);
        }
    }

    #[test]
    fn out_of_range_and_non_step_values_are_rejected() {
        for (contents, expected_detail) in [
            (
                "mouse_sensitivity=45\ncontroller_sensitivity=100",
                "input preference 'mouse_sensitivity' is invalid",
            ),
            (
                "mouse_sensitivity=100\ncontroller_sensitivity=205",
                "input preference 'controller_sensitivity' is invalid",
            ),
            (
                "mouse_sensitivity=100\ncontroller_sensitivity=102",
                "input preference 'controller_sensitivity' is invalid",
            ),
            (
                "mouse_sensitivity=65536\ncontroller_sensitivity=100",
                "input preference 'mouse_sensitivity' is invalid",
            ),
        ] {
            let error = parse(contents).unwrap_err();
            assert!(error.contains(expected_detail), "unexpected error: {error}");
            assert!(
                error.contains("50% through 200% in 5% steps"),
                "unexpected error: {error}"
            );
        }
    }

    #[test]
    fn saved_preferences_survive_a_fresh_load() {
        let path = unique_temp_path("restart");
        let saved = InputTuning::new(sensitivity(70), sensitivity(185));

        save(&path, saved).unwrap();
        let loaded_after_restart = load(&path).unwrap();

        assert_eq!(loaded_after_restart, saved);
        fs::remove_file(path).unwrap();
    }

    fn sensitivity(percent: u16) -> SensitivityPercent {
        SensitivityPercent::new(percent).unwrap()
    }

    fn unique_temp_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "skyroads-input-preferences-{label}-{}-{unique}.cfg",
            std::process::id()
        ))
    }
}
