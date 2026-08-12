pub const TAG_NAME: &str =
    ariel_os::config::str_from_env!("TAG_NAME", "Name of the BLE tag to advertise");

// Advertisement interval in ms.
pub const ADVERTISEMENT_INTERVAL: u64 = 500;
