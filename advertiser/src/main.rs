//! Adapted from the example in `trouble_host`.
#![no_main]
#![no_std]

mod config;

use ariel_os::{log::info, reexports::embassy_time, time::Timer};
use embassy_futures::join::join;
use embassy_time::Duration;
use trouble_host::advertise::{
    AdStructure, Advertisement, AdvertisementParameters, BR_EDR_NOT_SUPPORTED,
    LE_GENERAL_DISCOVERABLE,
};

use config::*;

#[ariel_os::task(autostart)]
async fn run_advertisement() {
    info!("starting ble stack");
    let stack = ariel_os::ble::ble_stack().await;
    let mut host = stack.build();

    let mut adv_data = [0; 31];

    info!("Starting advertising");

    let mut sequence: u32 = 0;

    let _ = join(host.runner.run(), async {
        loop {
            let sequence_bytes = sequence.to_be_bytes();

            let len = AdStructure::encode_slice(
                &[
                    AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
                    AdStructure::CompleteLocalName(TAG_NAME.as_bytes()),
                    AdStructure::ManufacturerSpecificData {
                        company_identifier: 0xFFFF,
                        payload: &sequence_bytes,
                    },
                ],
                &mut adv_data[..],
            )
            .unwrap();
            let params = AdvertisementParameters {
                interval_min: Duration::from_millis(ADVERTISEMENT_INTERVAL * 16),
                interval_max: Duration::from_millis(ADVERTISEMENT_INTERVAL * 16),
                // max_events: Some(1),
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

            Timer::after_millis(ADVERTISEMENT_INTERVAL).await;
            sequence += 1;
        }
    })
    .await;
}
