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

#[cfg(feature = "mqtt-backend")]
mod mqtt {
    use ariel_os::config::{ipv4_addr_from_env, str_from_env, str_from_env_or};
    use const_str::parse;
    use core::net::Ipv4Addr;

    pub const MQTT_HOST_NAME: &str = str_from_env!("MQTT_HOST_NAME", "MQTT Host name");
    pub const MQTT_HOST_IP: Ipv4Addr = ipv4_addr_from_env!("MQTT_HOST_IP", "MQTT Host IP");
    pub const MQTT_HOST_PORT: u16 = parse!(
        str_from_env_or!("MQTT_HOST_PORT", "8883", "MQTT port number"),
        u16
    );
    pub const MQTT_USER: &str = str_from_env!("MQTT_USER", "MQTT user");
    pub const MQTT_PWD: &str = str_from_env!("MQTT_PWD", "MQTT password");
    // topic will be built dynamically, example : <root>/<device id>/(<config> || <data>)
    pub const MQTT_TOPIC_ROOT: &str = "trm";
    pub const MQTT_TOPIC_CONFIG: &str = "config";
    pub const MQTT_TOPIC_DATA: &str = "data";
}
#[cfg(feature = "mqtt-backend")]
pub use mqtt::*;

pub const TIME_BETWEEN_UPDATES: Duration = Duration::from_secs(15 * 60);

pub const GNNS_AQUISITION_TIMEOUT_SEC: u16 = 360;

pub const LED_OFF_DURATION: Duration = Duration::from_secs(2);
pub const LED_ON_DURATION: Duration = Duration::from_millis(200);
