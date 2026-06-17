pub mod docker_mode;
pub mod model;
pub mod validate;

use crate::error::CboxError;
use model::Boxfile;

/// Parse a Boxfile from TOML text, then validate. Unknown top-level keys emit warnings.
pub fn parse_and_validate(toml_text: &str) -> Result<(Boxfile, Vec<String>), CboxError> {
    // We use toml::Value first to detect unknown top-level keys (warning, not error).
    let top_value: toml::Value = toml::from_str(toml_text)
        .map_err(|e| CboxError::dataerr(format!("Boxfile TOML parse error: {e}")))?;

    let known_top_level = &[
        "name",
        "image",
        "packages",
        "docker",
        "mounts",
        "sandbox",
        "box",
        "provision",
    ];

    let mut warnings = Vec::new();
    if let toml::Value::Table(ref t) = top_value {
        for key in t.keys() {
            if !known_top_level.contains(&key.as_str()) {
                warnings.push(format!(
                    "Unknown Boxfile key \"{key}\" (ignored; may be a future field)."
                ));
            }
        }
    }

    let bf: Boxfile = toml::from_str(toml_text)
        .map_err(|e| CboxError::dataerr(format!("Boxfile TOML parse error: {e}")))?;

    let result = validate::validate(&bf)?;
    warnings.extend(result.warnings);

    Ok((bf, warnings))
}

/// Parse a Boxfile from a file path.
pub fn parse_file(path: &str) -> Result<(Boxfile, Vec<String>), CboxError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| CboxError::ioerr(format!("Cannot read Boxfile at \"{path}\": {e}")))?;
    parse_and_validate(&text)
}
