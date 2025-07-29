#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
use heapless::Vec;
#[cfg(feature = "minicbor")]
use minicbor::{Decode, Encode};
use serde::{Deserialize, Serialize};
#[cfg(feature = "std")]
use std::vec::Vec;

pub const MAX_SEEN: usize = 32;
pub const TAG_NAME_MAX_LEN: usize = 64;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TagsSeen {
    #[cfg(feature = "std")]
    pub tags: Vec<DetectedTag>,
    #[cfg(not(feature = "std"))]
    pub tags: Vec<DetectedTag, MAX_SEEN>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "minicbor", derive(Decode, Encode))]
pub struct Location {
    #[cfg_attr(feature = "minicbor", n(0))]
    pub latitude: f32,
    #[cfg_attr(feature = "minicbor", n(1))]
    pub longitude: f32,
    #[cfg_attr(feature = "minicbor", n(2))]
    pub altitude: f32,
    #[cfg_attr(feature = "minicbor", n(3))]
    pub heading: f32,
    #[serde(rename = "horizontalSpeed")]
    #[cfg_attr(feature = "minicbor", n(4))]
    pub horizontal_speed: f32,
    #[serde(rename = "verticalSpeed")]
    #[cfg_attr(feature = "minicbor", n(5))]
    pub vertical_spedd: f32,
    #[serde(rename = "timeOfFix")]
    #[cfg_attr(feature = "minicbor", n(6))]
    pub time_of_fix: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "minicbor", derive(Decode, Encode))]
pub struct DetectedTag {
    #[cfg_attr(feature = "minicbor", cbor(n(0), with = "minicbor_adapters"))]
    pub id: heapless::String<TAG_NAME_MAX_LEN>,
    #[cfg_attr(feature = "minicbor", n(1))]
    pub age: u16,
    #[cfg_attr(feature = "minicbor", n(2))]
    pub rssi: i8,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "minicbor", derive(Decode, Encode))]
pub struct GatewayUpdate {
    #[cfg(feature = "std")]
    #[serde(rename = "gatewayId")]
    #[cfg_attr(feature = "minicbor", n(0))]
    pub gateway_id: String,
    #[cfg(not(feature = "std"))]
    #[serde(rename = "gatewayId")]
    #[cfg_attr(feature = "minicbor", cbor(n(0), with = "minicbor_adapters"))]
    pub gateway_id: heapless::String<TAG_NAME_MAX_LEN>,
    #[cfg_attr(feature = "minicbor", n(1))]
    pub timestamp: u64,

    #[cfg(feature = "std")]
    #[serde(rename = "detectedTags")]
    #[cfg_attr(feature = "minicbor", n(2))]
    pub detected_tags: Vec<DetectedTag>,
    #[cfg(not(feature = "std"))]
    #[serde(rename = "detectedTags")]
    #[cfg_attr(feature = "minicbor", cbor(n(2), with = "minicbor_adapters"))]
    pub detected_tags: Vec<DetectedTag, MAX_SEEN>,

    #[serde(rename = "batteryLevel")]
    #[cfg_attr(feature = "minicbor", n(3))]
    pub battery_level: Option<u8>,
    #[cfg_attr(feature = "minicbor", n(4))]
    pub location: Option<Location>,
}
