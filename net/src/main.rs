#![no_main]
#![no_std]

mod pins;

use core::{cell::Cell, str::FromStr};

use bt_hci::{cmd::info, param::LeAdvReport};
use embassy_futures::join::join;
use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use embedded_io_async::Write;
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
    gpio::{Input, Pull},
    log::{Debug2Format, debug, info, trace, warn},
    time::{Duration, Instant, Timer},
};

use common_types::{DetectedTag, MAX_SEEN, TAG_NAME_MAX_LEN, TagsSeen};

#[cfg(context = "nrf5340-net")]
use embassy_nrf::peripherals::SERIAL0;
#[cfg(context = "nrf52dk")]
use embassy_nrf::peripherals::UARTE0;
#[cfg(context = "nrf52840dk")]
use embassy_nrf::peripherals::UARTE0;
use embassy_nrf::{bind_interrupts, uarte};
use uuid::Uuid;

type TagStorageMap = FnvIndexMap<Uuid, (Instant, i8), MAX_SEEN>;

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
        Timer::after_millis(10).await;
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

            // There's an issue with the Duration https://github.com/embassy-rs/bt-hci/pull/74
            // Workaround is to multiply the value by 16.

            // Max scan interval in the BLE spec is 10s.
            interval: Duration::from_secs(10 * 16),
            // Beacon advertising frequency is between 1Hz and 10Hz, staying up makes sure we can catch at least one advertisement.
            window: Duration::from_secs(2 * 16),
            ..Default::default()
        };
        let mut _session = scanner.scan(&config).await.unwrap();
        // Scan forever
        loop {
            debug!("scanning...");
            Timer::after_secs(1000).await;
        }
    })
    .await;
}

struct DiscorveryHandler {}

fn return_valid_uuid(report: LeAdvReport) -> Option<uuid::Uuid> {
    let adv_data = AdStructure::decode(report.data);

    for adv in adv_data {
        match adv {
            Ok(AdStructure::ManufacturerSpecificData {
                company_identifier,
                payload,
            }) => {
                // If not Apple (for iBeacon)
                if company_identifier != 0x004c {
                    trace!("dropping for incorrect identifier");
                    return None;
                }

                if payload.len() <= 22 {
                    debug!("dropping for incorrect length {}", payload.len());

                    return None;
                }

                debug!("getting uuid");
                let mut uuid_data: [u8; 16] = [0; 16];
                uuid_data.copy_from_slice(payload.get(2..=17)?);
                debug!("getting sequence");

                let mut sequence_data: [u8; 4] = [0; 4];
                sequence_data.copy_from_slice(payload.get(18..=21)?);

                let uuid = uuid::Uuid::from_bytes(uuid_data);
                let sequence = u32::from_be_bytes(sequence_data);

                info!(
                    "uuid: {:?}, sequence:{}",
                    Debug2Format(&uuid.hyphenated()),
                    sequence
                );

                return Some(uuid);
            }

            Ok(adv) => {
                trace!("unknown advertisement {:?}", adv);
            }
            Err(e) => {
                trace!("error decoding advertisement: {:?}", e);
            }
        }
    }
    None
}

impl EventHandler for DiscorveryHandler {
    fn on_adv_reports(&self, mut it: LeAdvReportsIter<'_>) {
        let instant = Instant::now();

        SEEN.lock(|cell| {
            let mut seen = cell.take();
            while let Some(Ok(report)) = it.next() {
                let uuid = return_valid_uuid(report);
                if let Some(uuid) = uuid {
                    info!("uuid: {:?}, rssi {},chan {}", Debug2Format(&uuid.hyphenated()),report.rssi,report.event_kind)

                    let _ = seen.insert(uuid, (instant, report.rssi));
                }
            }

            cell.set(seen);
        });
    }
}
