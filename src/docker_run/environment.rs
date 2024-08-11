use std::collections::HashMap;
use std::env;
use std::fmt;
use std::str::FromStr;

pub type Environment = HashMap<String, String>;

pub fn get_environment() -> Environment {
    env::vars().collect()
}

pub fn lookup<T>(environment: &Environment, key: &'static str) -> Result<T, Error>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    environment
        .get(key)
        .ok_or(Error::KeyNotFound(key))
        .and_then(|string_value| {
            string_value.parse::<T>().map_err(|err| Error::Parse {
                key,
                details: err.to_string(),
            })
        })
}

pub fn lookup_optional<T>(environment: &Environment, key: &'static str) -> Result<Option<T>, Error>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    match environment.get(key) {
        None => Ok(None),

        Some(string_value) => string_value
            .parse::<T>()
            .map(Some)
            .map_err(|err| Error::Parse {
                key,
                details: err.to_string(),
            }),
    }
}

#[derive(Debug)]
pub enum Error {
    KeyNotFound(&'static str),
    Parse { key: &'static str, details: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::KeyNotFound(key) => write!(f, "Environment key not found: «{0}»", key),

            Error::Parse { key, details } => write!(
                f,
                "Failed to parse value for environment key: «{0}», details: {1}",
                key, details
            ),
        }
    }
}

pub fn space_separated_string(s: String) -> Vec<String> {
    s.split(' ')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
