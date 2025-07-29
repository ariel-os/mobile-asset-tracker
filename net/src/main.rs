#![no_main]
#![no_std]

mod pins;

use core::{cell::Cell, str::FromStr};

use embassy_futures::join::join;
use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use heapless::{String, Vec, index_map::FnvIndexMap};
use postcard::{
    ser_flavors::{Cobs, Slice},
    serialize_with_flavor,
};
use trouble_host::{
    Host,
    connection::{PhySet, ScanConfig},
    prelude::{AdStructure, EventHandler, LeAdvReportsIter},
    scan::Scanner,
};

use ariel_os::{
    config::str_from_env_or,
    debug::log::{Debug2Format, info, trace, warn},
    time::{Duration, Instant, Timer},
};

use embedded_io_async::Write;

use common_types::{DetectedTag, MAX_SEEN, TAG_NAME_MAX_LEN, TagsSeen};

#[cfg(context = "nrf5340-net")]
use embassy_nrf::peripherals::SERIAL0;
#[cfg(context = "nrf52dk")]
use embassy_nrf::peripherals::UARTE0;
#[cfg(context = "nrf52840dk")]
use embassy_nrf::peripherals::UARTE0;
use embassy_nrf::{bind_interrupts, uarte};

type TagStorageMap = FnvIndexMap<String<TAG_NAME_MAX_LEN>, (Instant, i8), MAX_SEEN>;

static SEEN: Mutex<CriticalSectionRawMutex, Cell<TagStorageMap>> =
    Mutex::new(Cell::new(FnvIndexMap::new()));

const PREFIX: &str = str_from_env_or!(
    "TAG_PREFIX",
    "Ariel",
    "Filter out all BLE devices that don't have this prefix in their name"
);

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
        Timer::after_secs(30).await;
        // Remove entries older than 10 minutes
        {
            SEEN.lock(|cell| {
                let mut seen = cell.take();
                remove_old_entries(&mut seen);
                cell.set(seen);
            });
        }
    }
}

#[ariel_os::task(autostart, peripherals)]
async fn send_scan_data(peripherals: pins::Peripherals) {
    let mut config = uarte::Config::default();
    config.parity = uarte::Parity::EXCLUDED;
    config.baudrate = uarte::Baudrate::BAUD115200;

    let mut uart = uarte::Uarte::new(
        peripherals.serial,
        peripherals.uart_rx,
        peripherals.uart_tx,
        Irqs,
        config,
    );

    loop {
        Timer::after_secs(2).await;
        info!("Sending scan data...");
        let seen = {
            SEEN.lock(|cell| {
                let t = cell.take();
                cell.set(t.clone());
                t
            })
        };

        let now = Instant::now();

        let tags: Vec<DetectedTag, MAX_SEEN> = seen
            .iter()
            .map(|(id, (instant, rssi))| DetectedTag {
                age: u16::try_from(now.duration_since(*instant).as_secs()).unwrap_or(u16::MAX),
                rssi: *rssi,
                id: id.clone(),
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
    }
}

/// Remove entries older than 10 minutes
fn remove_old_entries(seen: &mut TagStorageMap) {
    let now = Instant::now();
    seen.retain(|_, &mut (instant, _)| now.duration_since(instant) < Duration::from_secs(600));
}

fn remove_oldest_entry(seen: &mut TagStorageMap) {
    if let Some((oldest_key, _)) = seen.iter().min_by_key(|&(_, &v)| v) {
        seen.remove(&oldest_key.clone());
    }
}

#[ariel_os::task(autostart)]
async fn run_scanner() {
    info!("starting ble stack");

    let Host {
        central,
        mut runner,
        ..
    } = ariel_os::ble::ble_stack().await.build();

    let printer = DiscorveryHandler {};
    let mut scanner = Scanner::new(central);
    let _ = join(runner.run_with_handler(&printer), async {
        let config = ScanConfig::<'_> {
            active: true,
            phys: PhySet::M1,
            interval: Duration::from_secs(1),
            window: Duration::from_secs(1),
            ..Default::default()
        };
        let mut _session = scanner.scan(&config).await.unwrap();
        // Scan forever
        loop {
            info!("scanning...");
            Timer::after_secs(1).await;
        }
    })
    .await;
}

struct DiscorveryHandler {}

impl EventHandler for DiscorveryHandler {
    fn on_adv_reports(&self, mut it: LeAdvReportsIter<'_>) {
        SEEN.lock(|cell| {
            let mut seen = cell.take();
            while let Some(Ok(report)) = it.next() {
                let adv_data = AdStructure::decode(report.data);

                let name = {
                    let mut decoded = None;
                    for adv in adv_data {
                        match adv {
                            Ok(AdStructure::CompleteLocalName(data)) => {
                                decoded = str::from_utf8(data).ok();

                                if decoded.is_none() {
                                    warn!("failed to decode name");
                                }

                                break;
                            }

                            Ok(adv) => {
                                trace!("unknown advertisement {:?}", adv);
                            }
                            Err(e) => {
                                trace!("error decoding advertisement: {:?}", e);
                            }
                        }
                    }
                    decoded
                };

                if let Some(str_name) = name
                    && str_name.starts_with(PREFIX)
                {
                    match String::from_str(str_name) {
                        Ok(name) => {
                            if !seen.contains_key(&name) {
                                trace!("discovered: {}", name.as_str());
                                // force cleanup if we have too many entries
                                if seen.len() >= MAX_SEEN {
                                    remove_old_entries(&mut seen);
                                    // if we still have too many entries, remove the oldest one
                                    if seen.len() >= MAX_SEEN {
                                        warn!("too many seen entries, removing oldest");
                                        remove_oldest_entry(&mut seen);
                                    }
                                }
                            }
                            // Update / insert the address with the current time
                            let _ = seen.insert(name, (Instant::now(), report.rssi));
                        }
                        Err(e) => {
                            warn!("BLE name too long: {:?} ", Debug2Format(&e));
                        }
                    }
                }
            }

            cell.set(seen);
        });
    }
}
