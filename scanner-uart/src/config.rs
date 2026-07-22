use ariel_os::{config::str_from_env_or, time::Duration};

// Set using env variable.
pub const TAG_PREFIX: [u8; 3] = {
    const PREFIX_STR: &str = str_from_env_or!(
        "TAG_PREFIX",
        "CC:DD:EE",
        "Filter out all BLE devices that don't have this prefix in their name"
    );
    let mut eui48: [u8; 3] = const_str::hex!(const_str::replace!(PREFIX_STR, ":", ""));
    eui48.reverse();
    eui48
};

pub const SCAN_INTERVAL: Duration = Duration::from_secs(10);
pub const SCAN_WINDOW: Duration = Duration::from_secs(2);
