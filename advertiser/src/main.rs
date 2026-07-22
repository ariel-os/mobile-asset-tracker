//! Adapted from the example in `trouble_host`.
#![no_main]
#![no_std]

mod config;
mod pins;

use ariel_os::{log::info, reexports::embassy_time, time::Timer};
use embassy_futures::join::join;
#[cfg(all(context = "nrf52840", feature = "nrf-qspi-optimisation"))]
use embassy_nrf::{
    bind_interrupts, peripherals,
    qspi::{self, Frequency},
};
use embassy_time::Duration;
use trouble_host::advertise::{
    AdStructure, Advertisement, AdvertisementParameters, BR_EDR_NOT_SUPPORTED,
    LE_GENERAL_DISCOVERABLE,
};

use config::*;

#[cfg(all(context = "nrf52840", feature = "nrf-qspi-optimisation"))]
bind_interrupts!(struct Irqs {
    QSPI => qspi::InterruptHandler<peripherals::QSPI>;
});

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
