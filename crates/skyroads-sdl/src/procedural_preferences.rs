use std::fs;
use std::path::Path;

use skyroads_core::GenerationId;

type Result<T> = std::result::Result<T, String>;

const GENERATION_ID_KEY: &str = "generation_id";

pub fn load(path: &Path) -> Result<Option<GenerationId>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "could not read procedural preferences from {}: {error}",
                path.display()
            ));
        }
    };

    parse(&contents).map(Some)
}

pub fn save(path: &Path, generation_id: GenerationId) -> Result<()> {
    fs::write(path, format!("{GENERATION_ID_KEY}={generation_id}\n")).map_err(|error| {
        format!(
            "could not save procedural preferences to {}: {error}",
            path.display()
        )
    })
}

fn parse(contents: &str) -> Result<GenerationId> {
    let line = contents.trim();
    let Some((key, value)) = line.split_once('=') else {
        return Err("procedural preferences must be 'generation_id=ID'".to_string());
    };
    if key != GENERATION_ID_KEY || value.is_empty() || value.contains('=') {
        return Err("procedural preferences must be 'generation_id=ID'".to_string());
    }
    value
        .parse()
        .map_err(|error| format!("procedural preferences contain an invalid ID: {error}"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use skyroads_core::{GenerationId, ProceduralDifficulty};

    use super::{load, parse, save};

    #[test]
    fn preference_round_trips_in_canonical_form() {
        let id = GenerationId::new(0x0012_3456_789A, ProceduralDifficulty::Hard);
        assert_eq!(parse(&format!("generation_id={id}\n")).unwrap(), id);
    }

    #[test]
    fn missing_file_is_not_an_error_and_invalid_data_is_rejected() {
        let path = unique_temp_path("missing");
        assert_eq!(load(&path).unwrap(), None);
        assert!(parse("seed=42").is_err());
        assert!(parse("generation_id=SR9-C-0000-0000-00").is_err());
    }

    #[test]
    fn saved_id_survives_a_fresh_load() {
        let path = unique_temp_path("round-trip");
        let id = GenerationId::new(0x000F_EDCB_A987, ProceduralDifficulty::Easy);
        save(&path, id).unwrap();
        assert_eq!(load(&path).unwrap(), Some(id));
        fs::remove_file(path).unwrap();
    }

    fn unique_temp_path(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "skyroads-procedural-preferences-{label}-{}-{unique}.cfg",
            std::process::id()
        ))
    }
}
