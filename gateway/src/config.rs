use ariel_os::time::Duration;

// Env variables for HTTP backend
#[cfg(feature = "http-backend")]
mod http {
    use ariel_os::config::str_from_env;

    pub const KUZZLE_ENDPOINT: &str =
        str_from_env!("KUZZLE_ENDPOINT", "Kuzzle endpoint to connect to.");
    pub const KUZZLE_TOKEN: &str = str_from_env!("KUZZLE_TOKEN", "Kuzzle token.");
    pub const BEARER_HEADER_VALUE: &str = const_str::format!("Bearer {}", KUZZLE_TOKEN);
}
#[cfg(feature = "http-backend")]
pub use http::*;

#[cfg(feature = "coap-backend")]
mod coap {
    use ariel_os::config::str_from_env;

    pub const COAP_ENDPOINT: &str =
        str_from_env!("COAP_ENDPOINT", "The CoAP endpoint to connect to.");
}
#[cfg(feature = "coap-backend")]
pub use coap::*;

pub const TIME_BETWEEN_UPDATES: Duration = Duration::from_secs(15 * 60);

pub const GNNS_AQUISITION_TIMEOUT_SEC: u16 = 360;

pub const LED_OFF_DURATION: Duration = Duration::from_secs(2);
pub const LED_ON_DURATION: Duration = Duration::from_millis(200);
