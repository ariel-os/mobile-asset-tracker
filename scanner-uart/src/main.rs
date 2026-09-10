#![no_main]
#![no_std]

mod config;
mod pins;

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embedded_io_async::Write;
use heapless::{Vec, index_map::FnvIndexMap};
use mobile_asset_tracker_scanner::{TagReport, TagScanner};
use postcard::{
    ser_flavors::{Cobs, Slice},
    serialize_with_flavor,
};

use ariel_os::{
    gpio::{Input, Pull},
    identity::Eui48,
    log::{Debug2Format, error, info, warn},
    time::{Instant, Timer},
};

use common_types::{DetectedTag, MAX_SEEN, TagsSeen};

use config::*;

#[cfg(context = "nrf5340-net")]
use embassy_nrf::peripherals::SERIAL0;
#[cfg(context = "nrf52dk")]
use embassy_nrf::peripherals::UARTE0;
#[cfg(context = "nrf52840dk")]
use embassy_nrf::peripherals::UARTE0;
use embassy_nrf::{bind_interrupts, uarte};

type TagStorageMap = FnvIndexMap<[u8; 3], TagReport, MAX_SEEN>;

static SEEN: Mutex<CriticalSectionRawMutex, TagStorageMap> = Mutex::new(FnvIndexMap::new());

static TRACKER_SCANNER: TagScanner = TagScanner::new(TAG_PREFIX);

#[cfg(context = "nrf5340-net")]
bind_interrupts!(struct Irqs {
    SERIAL0 => uarte::InterruptHandler<SERIAL0>;
});

#[cfg(context = "nrf52dk")]
bind_interrupts!(struct Irqs {
    UARTE0 => uarte::InterruptHandler<UARTE0>;
});

#[cfg(context = "nrf52840dk")]
bind_interrupts!(struct Irqs {
    UARTE0 => uarte::InterruptHandler<UARTE0>;
});

#[ariel_os::task(autostart)]
async fn automatic_cleanup() {
    loop {
        Timer::after(SCAN_INTERVAL).await;
        // Remove entries older than 10 minutes
        {
            let mut seen = SEEN.lock().await;

            remove_old_entries(&mut seen);
        }
    }
}

#[ariel_os::task(autostart, peripherals)]
async fn send_scan_data(mut peripherals: pins::Peripherals) {
    let mut request = Input::builder(peripherals.request, Pull::Down)
        .build_with_interrupt()
        .unwrap();

    let mut config = uarte::Config::default();
    config.parity = uarte::Parity::EXCLUDED;
    config.baudrate = uarte::Baudrate::BAUD115200;

    // When the request pin is high, send an update evrey 10ms.
    loop {
        request.wait_for_high().await;

        let mut uart = uarte::Uarte::new(
            peripherals.serial.reborrow(),
            peripherals.uart_rx.reborrow(),
            peripherals.uart_tx.reborrow(),
            Irqs,
            config.clone(),
        );
        info!("Sending scan data...");
        let seen = { SEEN.lock().await.clone() };

        let now = Instant::now();

        let tags: Vec<DetectedTag, MAX_SEEN> = seen
            .iter()
            .map(|(_id, report)| {
                // BdAddr stores it in reverse order of what Eui48 expects.
                let mut addr = report.addr.into_inner();
                addr.reverse();
                DetectedTag {
                    age: u16::try_from(now.duration_since(report.timestamp).as_secs())
                        .unwrap_or(u16::MAX),
                    rssi: report.rssi,
                    addr: heapless::format!("{}", Eui48(addr))
                        .unwrap_or(heapless::String::try_from("encode error").unwrap()),
                    name: report.name.clone(),
                    sequence: report.sequence,
                }
            })
            .collect();

        let buffer = &mut [0u8; 4096];
        let data = serialize_with_flavor::<TagsSeen, Cobs<Slice>, &mut [u8]>(
            &TagsSeen { tags },
            Cobs::try_new(Slice::new(buffer)).unwrap(),
        );

        // let buffer = &mut [0u8; 16];
        // let data: Result<&'static [u8; 5], &'static str> = Ok(b"Hello");

        match data {
            Ok(slice) => match uart.write_all(slice).await {
                Ok(_) => {
                    info!("Sent {} bytes", slice.len());
                }
                Err(e) => {
                    warn!("Failed to send data over UART: {:?}", e);
                }
            },
            Err(e) => {
                warn!("Failed to serialize data: {}", e);
            }
        }
        Timer::after_millis(10).await;
    }
}

/// Remove entries older than 10 minutes
fn remove_old_entries(seen: &mut TagStorageMap) {
    let now = Instant::now();
    seen.retain(|_, tag| now.duration_since(tag.timestamp) < SCAN_INTERVAL * 10);
}

fn remove_oldest_entry(seen: &mut TagStorageMap) {
    if let Some((oldest_key, _)) = seen.iter().min_by_key(|&(_, v)| v.timestamp) {
        seen.remove(&oldest_key.clone());
    }
}

#[ariel_os::task(autostart)]
async fn receive_tags() {
    let receiver = TRACKER_SCANNER.receiver();

    loop {
        let value = receiver.receive().await;

        let mut seen = SEEN.lock().await;
        let mut key = [0u8; 3];
        key.copy_from_slice(&value.addr.into_inner()[0..3]);

        if seen.is_full() && !seen.contains_key(&key) {
            remove_oldest_entry(&mut seen);
        }

        info!(
            "inserting {:?}, sequence: {:06}, name: {}",
            key,
            value.sequence,
            value.name.as_str()
        );

        if let Err(e) = seen.insert(key, value) {
            error!("Cannot insert tag: {:?}", Debug2Format(&e));
        }
    }
}

#[ariel_os::task(autostart)]
async fn run_scanner() {
    info!("starting ble stack");

    let host = ariel_os::ble::ble_stack().await.build();

    TRACKER_SCANNER.run(host, SCAN_INTERVAL, SCAN_WINDOW).await
}
