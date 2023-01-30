use regex::Regex;
use serde::{
    Deserialize,
    Deserializer,
    de::{Error as DeserializerError},
};

const FILE_SIZE_RE: &str = r#"^(?i)(?P<size>\d+)(?P<unit>[kmg]?)b?$"#;

#[derive(thiserror::Error, Debug)]
#[error("{0}")]
struct ConfigError(&'static str);

fn parse_file_size(value: &str) -> Result<usize, ConfigError> {
    let file_size_re = Regex::new(FILE_SIZE_RE)
        .expect("regexp should be valid");
    let caps = file_size_re.captures(value)
        .ok_or(ConfigError("invalid file size"))?;
    let size: usize = caps["size"].to_string().parse()
        .map_err(|_| ConfigError("invalid file size"))?;
    let unit = caps["unit"].to_string().to_lowercase();
    let multiplier = match unit.as_str() {
        "k" => usize::pow(10, 3),
        "m" => usize::pow(10, 6),
        "g" => usize::pow(10, 9),
        "" => 1,
        _ => return Err(ConfigError("invalid file size unit")),
    };
    Ok(size * multiplier)
}

fn deserialize_file_size<'de, D>(
    deserializer: D,
) -> Result<usize, D::Error>
    where D: Deserializer<'de>
{
    let file_size_str = String::deserialize(deserializer)?;
    let file_size = parse_file_size(&file_size_str)
        .map_err(DeserializerError::custom)?;
    Ok(file_size)
}

const fn default_file_size_limit() -> usize { 20_000_000 } // 20 MB

#[derive(Clone, Deserialize)]
pub struct MediaLimits {
    #[serde(
        default = "default_file_size_limit",
        deserialize_with = "deserialize_file_size",
    )]
    pub file_size_limit: usize,
}

impl Default for MediaLimits {
    fn default() -> Self {
        Self {
            file_size_limit: default_file_size_limit(),
        }
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct Limits {
    #[serde(default)]
    pub media: MediaLimits,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file_size() {
        let file_size = parse_file_size("1234").unwrap();
        assert_eq!(file_size, 1234);
        let file_size = parse_file_size("89kB").unwrap();
        assert_eq!(file_size, 89_000);
        let file_size = parse_file_size("12M").unwrap();
        assert_eq!(file_size, 12_000_000);
    }
}
