//! Adapted from the example in `trouble_host`.
#![no_main]
#![no_std]

mod pins;

use ariel_os::{
    gpio,
    log::{Debug2Format, debug, info},
    reexports::embassy_time,
    time::Timer,
};
use embassy_futures::join::join;
use embassy_nrf::{bind_interrupts, peripherals, spim};
use embassy_time::Duration;
use embedded_hal_async::spi::SpiDevice as _;
use embedded_hal_bus::spi::ExclusiveDevice;
use heapless::Vec;
use trouble_host::advertise::{
    AdStructure, Advertisement, AdvertisementParameters, BR_EDR_NOT_SUPPORTED,
    LE_GENERAL_DISCOVERABLE,
};

const TAG_NAME: &str =
    ariel_os::config::str_from_env!("TAG_NAME", "Name of the BLE tag to advertise");

// Advertisement interval in ms.
const ADVERTISEMENT_INTERVAL: u64 = 500;
bind_interrupts!(struct Irqs {
    SPIM3 => spim::InterruptHandler<peripherals::SPI3>;
});

#[ariel_os::task(autostart, peripherals)]
async fn disable_flash(peripherals: pins::Peripherals) {
    let mut spi_config = embassy_nrf::spim::Config::default();
    debug!(
        "Selected frequency: {:?}",
        Debug2Format(&spi_config.frequency)
    );

    let mut id = [0; 3];
    {
        let spi_bus = embassy_nrf::spim::Spim::new(
            peripherals.instance,
            Irqs,
            peripherals.spi_sck,
            peripherals.spi_miso,
            peripherals.spi_mosi,
            spi_config,
        );
        let spi_bus = spi_bus;

        let cs_output = gpio::Output::new(peripherals.spi_cs, gpio::Level::High);
        let mut spi_device = ExclusiveDevice::new_no_delay(spi_bus, cs_output).unwrap();

        // Deep sleep.
        spi_device.write(&[0xb9]).await.unwrap();
    }

    info!("id: {:?}", id);
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
