//! Adapted from the example in `trouble_host`.
#![no_main]
#![no_std]

use ariel_os::{log::info, reexports::embassy_time, time::Timer};
use embassy_futures::join::join;
use embassy_time::Duration;
use heapless::Vec;
use trouble_host::advertise::{
    AdStructure, Advertisement, AdvertisementParameters, BR_EDR_NOT_SUPPORTED,
    LE_GENERAL_DISCOVERABLE,
};

#[ariel_os::task(autostart)]
async fn run_advertisement() {
    info!("starting ble stack");
    let stack = ariel_os::ble::ble_stack().await;
    let mut host = stack.build();

    let mut adv_data = [0; 31];

    const BEACON_TYPE: [u8; 2] = [0x02, 0x15];
    const BEACON_UUID_PREFIX: [u8; 8] = [0x01, 0x12, 0x23, 0x34, 0x45, 0x56, 0x67, 0x78];
    const BEACON_UUID_SUFFIX: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    const BEACON_MEASURED_POWER: [u8; 1] = [0];

    let mut beacon_uuid: Vec<_, 16> = Vec::from_array(BEACON_UUID_PREFIX);
    beacon_uuid.extend_from_slice(&BEACON_UUID_SUFFIX).unwrap();

    info!("Starting advertising");

    let mut sequence: u32 = 0;

    let _ = join(host.runner.run(), async {
        loop {
            let sequence_bytes = sequence.to_be_bytes();
            let mut manufacturer_payload: Vec<_, 27> = Vec::new();
            manufacturer_payload
                .extend_from_slice(&BEACON_TYPE)
                .unwrap();
            manufacturer_payload
                .extend_from_slice(&beacon_uuid)
                .unwrap();

            // We replace MAJOR and MINOR with the sequence bytes
            manufacturer_payload
                .extend_from_slice(&sequence_bytes)
                .unwrap();
            manufacturer_payload
                .extend_from_slice(&BEACON_MEASURED_POWER)
                .unwrap();

            let len = AdStructure::encode_slice(
                &[
                    AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
                    AdStructure::ManufacturerSpecificData {
                        // Apple company identifier [0x4c, 0x00]
                        company_identifier: 0x004c,
                        payload: &manufacturer_payload,
                    },
                ],
                &mut adv_data[..],
            )
            .unwrap();
            let params = AdvertisementParameters {
                interval_min: Duration::from_millis(100),
                interval_max: Duration::from_millis(100),
                max_events: Some(1),
                ..Default::default()
            };

            let _advertiser = host
                .peripheral
                .advertise(
                    &params,
                    Advertisement::NonconnectableScannableUndirected {
                        adv_data: adv_data.get(..len).unwrap(),
                        scan_data: &[],
                    },
                )
                .await;

            Timer::after_secs(1).await;
            sequence += 1;
        }
    })
    .await;
}
