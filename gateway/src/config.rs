use ariel_os::{config::str_from_env, time::Duration};

// Env variables.
pub const KUZZLE_ENDPOINT: &str =
    str_from_env!("KUZZLE_ENDPOINT", "Kuzzle endpoint to connect to.");
pub const KUZZLE_TOKEN: &str = str_from_env!("KUZZLE_TOKEN", "Kuzzle token.");
pub const BEARER_HEADER_VALUE: &str = const_str::format!("Bearer {}", KUZZLE_TOKEN);

pub const TIME_BETWEEN_UPDATES: Duration = Duration::from_secs(15 * 60);
