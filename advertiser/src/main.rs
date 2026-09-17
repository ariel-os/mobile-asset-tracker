//! Adapted from the example in `trouble_host`.
#![no_main]
#![no_std]

mod config;
mod pins;

use ariel_os::{log::{debug, info}, reexports::embassy_time, time::Timer};
use embassy_futures::join::join;
#[cfg(all(context = "nrf52840", feature = "nrf-qspi-optimisation"))]
use embassy_nrf::{
    bind_interrupts, peripherals,
    qspi::{self, Frequency},
};
use embassy_time::Duration;
use trouble_host::advertise::{
    AdStructure, AdvChannelMap, Advertisement, AdvertisementParameters, BR_EDR_NOT_SUPPORTED,
    LE_GENERAL_DISCOVERABLE,
};

use config::*;

#[cfg(all(context = "nrf52840", feature = "nrf-qspi-optimisation"))]
bind_interrupts!(struct Irqs {
    QSPI => qspi::InterruptHandler<peripherals::QSPI>;
});

#[derive(Debug, defmt::Format)]
struct AdvertisementPayload {
    pub sequence: u32,
    pub channel_number: u8,
}

impl AdvertisementPayload {
    pub fn to_bytes(&self) -> [u8; 5] {
        [
            self.sequence.to_be_bytes()[0],
            self.sequence.to_be_bytes()[1],
            self.sequence.to_be_bytes()[2],
            self.sequence.to_be_bytes()[3],
            self.channel_number,
        ]
    }
}

#[cfg(all(context = "nrf52840", feature = "nrf-qspi-optimisation"))]
#[ariel_os::task(autostart, peripherals)]
async fn disable_flash(peripherals: pins::Peripherals) {
    let mut config = qspi::Config::default();
    config.capacity = 2 * 1024 * 1024; // 2 MB
    config.frequency = Frequency::M32;
    config.deep_power_down = Some(qspi::DeepPowerDownConfig {
        // Arbitrary values, we don't use the flash.
        enter_time: 3,
        exit_time: 3,
    });

    let _q = qspi::Qspi::new(
        peripherals.instance,
        Irqs,
        peripherals.spi_sck,
        peripherals.spi_cs,
        peripherals.spi_io0,
        peripherals.spi_io1,
        peripherals.spi_io2,
        peripherals.spi_io3,
        config,
    );

    // Drop the instance to power down the peripherals.
}

#[ariel_os::task(autostart)]
async fn run_advertisement() {
    #[cfg(all(context = "nrf52840", feature = "nrf-power-optimisation"))]
    embassy_nrf::pac::POWER
        .dcdcen()
        .write(|w| w.set_dcdcen(true));

    info!("starting ble stack");
    let stack = ariel_os::ble::ble_stack().await;
    let mut host = stack.build();

    let mut adv_data = [0; 31];

    info!("Starting advertising");

    let mut sequence: u32 = 0;
    let mut channel_number: u8 = 37;
    let _ = join(host.runner.run(), async {
        loop {
            let payload = AdvertisementPayload {
                sequence,
                channel_number,
            };
            debug!("payload {:?}", payload);
            let len = AdStructure::encode_slice(
                &[
                    AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
                    AdStructure::CompleteLocalName(TAG_NAME.as_bytes()),
                    AdStructure::ManufacturerSpecificData {
                        company_identifier: 0xFFFF,
                        payload: &payload.to_bytes(),
                    },
                ],
                &mut adv_data[..],
            )
            .unwrap();
            // channels are disabled by default
            let channel_map = AdvChannelMap::new();
            let channel_map = match channel_number {
                37 => channel_map.enable_channel_37(true),
                38 => channel_map.enable_channel_38(true),
                39 => channel_map.enable_channel_39(true),
                _ => channel_map,
            };
            let channel_map = Some(channel_map);

            let params = AdvertisementParameters {
                interval_min: Duration::from_millis(ADVERTISEMENT_INTERVAL * 16),
                interval_max: Duration::from_millis(ADVERTISEMENT_INTERVAL * 16),
                channel_map,
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
            channel_number = if channel_number < 39 {
                channel_number + 1
            } else {
                37
            };
        }
    })
    .await;
}
