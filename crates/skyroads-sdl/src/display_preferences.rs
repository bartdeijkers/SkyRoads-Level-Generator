use std::fs;
use std::path::Path;

use skyroads_core::{DisplaySettings, VideoMode};

type Result<T> = std::result::Result<T, String>;

pub fn load(path: &Path) -> Result<Option<DisplaySettings>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "could not read display preferences from {}: {error}",
                path.display()
            ));
        }
    };

    parse(&contents).map(Some)
}

pub fn save(path: &Path, settings: DisplaySettings) -> Result<()> {
    fs::write(path, format!("{}\n", encode(settings))).map_err(|error| {
        format!(
            "could not save display preferences to {}: {error}",
            path.display()
        )
    })
}

fn parse(contents: &str) -> Result<DisplaySettings> {
    let fields = contents.split_whitespace().collect::<Vec<_>>();
    match fields.as_slice() {
        ["windowed"] => Ok(DisplaySettings::Windowed),
        ["borderless"] => Ok(DisplaySettings::BorderlessDesktop),
        ["exclusive", width, height, refresh_hz] => {
            let width = parse_positive_u32("width", width)?;
            let height = parse_positive_u32("height", height)?;
            let refresh_hz = if *refresh_hz == "auto" {
                None
            } else {
                Some(parse_positive_u32("refresh rate", refresh_hz)?)
            };
            let mode = VideoMode::new(width, height, refresh_hz)
                .ok_or_else(|| "display preferences contain an invalid video mode".to_string())?;
            Ok(DisplaySettings::ExclusiveFullscreen(mode))
        }
        _ => Err(
            "display preferences must be 'windowed', 'borderless', or 'exclusive WIDTH HEIGHT HZ'"
                .to_string(),
        ),
    }
}

fn parse_positive_u32(label: &str, value: &str) -> Result<u32> {
    value
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("display preference {label} must be a positive integer"))
}

fn encode(settings: DisplaySettings) -> String {
    match settings {
        DisplaySettings::Windowed => "windowed".to_string(),
        DisplaySettings::BorderlessDesktop => "borderless".to_string(),
        DisplaySettings::ExclusiveFullscreen(mode) => {
            let refresh_hz = mode
                .refresh_hz()
                .map(|refresh_hz| refresh_hz.to_string())
                .unwrap_or_else(|| "auto".to_string());
            format!("exclusive {} {} {refresh_hz}", mode.width(), mode.height())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use skyroads_core::{DisplaySettings, VideoMode};

    use super::{encode, load, parse, save};

    #[test]
    fn settings_round_trip_without_optional_or_ambiguous_states() {
        let modes = [
            DisplaySettings::Windowed,
            DisplaySettings::BorderlessDesktop,
            DisplaySettings::ExclusiveFullscreen(VideoMode::new(3840, 2160, Some(144)).unwrap()),
            DisplaySettings::ExclusiveFullscreen(VideoMode::new(1920, 1080, None).unwrap()),
        ];

        for settings in modes {
            assert_eq!(parse(&encode(settings)).unwrap(), settings);
        }
    }

    #[test]
    fn malformed_or_zero_modes_are_rejected() {
        for malformed in [
            "fullscreen",
            "exclusive 3840 2160",
            "exclusive 0 2160 144",
            "exclusive 3840 2160 0",
            "exclusive wide 2160 144",
        ] {
            assert!(parse(malformed).is_err(), "accepted {malformed:?}");
        }
    }

    #[test]
    fn saved_preferences_load_from_the_same_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "skyroads-display-preferences-{}-{unique}.cfg",
            std::process::id()
        ));
        let settings =
            DisplaySettings::ExclusiveFullscreen(VideoMode::new(3840, 2160, Some(144)).unwrap());

        save(&path, settings).unwrap();
        assert_eq!(load(&path).unwrap(), Some(settings));

        fs::remove_file(path).unwrap();
    }
}
