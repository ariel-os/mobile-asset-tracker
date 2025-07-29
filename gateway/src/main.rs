#![no_main]
#![no_std]
mod pins;
mod sensors;

use core::{cell::RefCell, str::FromStr as _};

use coap_handler::Attribute;
use coap_handler_implementations::{GetRenderable, TypeHandler, wkc::ConstantSingleRecordReport};
use coap_request::Stack;
use embassy_sync::{
    blocking_mutex::{self, raw::CriticalSectionRawMutex},
    mutex::Mutex,
};
use embedded_io_async::BufRead;
use heapless::{String, Vec};

use ariel_os::{
    asynch::Spawner,
    config::str_from_env,
    debug::log::{Debug2Format, Hex, debug, error, info, warn},
    gpio::{Input, Level, Output, Pull},
    hal,
    sensors::{Label, Reading, Sensor},
    time::{Duration, Instant, Timer},
    uart::Baudrate,
};
use ariel_os_sensors_gnss_time_ext::GnssTimeExt as _;

use common_types::{DetectedTag, GatewayUpdate, Location, TAG_NAME_MAX_LEN, TagsSeen};

use crate::pins::{GnssStatusPeripherals, UartPeripherals, UpdatePeripherals};

// Test server : 65.108.193.50:4230
const COAP_ENDPOINT: &str = str_from_env!("COAP_ENDPOINT", "The CoAP endpoint to connect to.");

static SEEN: Mutex<CriticalSectionRawMutex, (TagsSeen, Instant)> =
    Mutex::new((TagsSeen { tags: Vec::new() }, Instant::from_ticks(0)));
static CURRENT_LOCATION: Mutex<CriticalSectionRawMutex, Option<Location>> = Mutex::new(None);

static LAST_UPDATE: blocking_mutex::Mutex<CriticalSectionRawMutex, RefCell<Option<GatewayUpdate>>> =
    blocking_mutex::Mutex::new(RefCell::new(None));

#[ariel_os::task(autostart, peripherals)]
async fn uart_receive(peripherals: UartPeripherals) {
    let mut config = hal::uart::Config::default();
    config.baudrate = Baudrate::_115200;

    let mut rx_buf = [0u8; 32];
    let mut tx_buf = [0u8; 1];

    let mut uart = pins::ReceiverUart::new(
        peripherals.uart_rx,
        peripherals.uart_tx,
        &mut rx_buf,
        &mut tx_buf,
        config,
    )
    .expect("Invalid UART configuration");
    let mut packet_buffer: Vec<u8, 8192> = Vec::new();

    loop {
        debug!("Waiting for UART data...");
        let result = uart.fill_buf().await;
        let read = match result {
            Err(e) => {
                error!("UART read error: {:?}", e);
                continue;
            }
            Ok(n) => n,
        };
        let size_read = read.len();
        debug!("Read {} bytes from UART", size_read);
        let err = packet_buffer.extend_from_slice(read);
        if let Err(e) = err {
            warn!("Packet buffer full, dropping data: {:?}", Debug2Format(&e));
            packet_buffer.clear();
            continue;
        }
        uart.consume(size_read);

        if let Some(separator) = packet_buffer.iter().position(|&b| b == 0x00) {
            let packet = &mut packet_buffer[..separator];
            debug!("Received packet, trying to decode...");

            match postcard::from_bytes_cobs::<TagsSeen>(packet) {
                Ok(decoded) => {
                    debug!("Decoded packet");
                    let mut seen = SEEN.lock().await;
                    *seen = (decoded, Instant::now())
                }
                Err(e) => {
                    warn!("Failed to decode packet: {:?}", e);
                }
            }

            // Remove the read buffer
            // should not panic as the size is smaller than the capacity of the vec.
            packet_buffer = Vec::from_slice(packet_buffer.split_at(separator + 1).1).unwrap();
        }
    }
}

#[ariel_os::task(autostart, peripherals)]
async fn update_location(peripherals: GnssStatusPeripherals) {
    let mut led_blue = Output::new(peripherals.led_blue, Level::Low);
    let mut led_red = Output::new(peripherals.led_red, Level::Low);
    led_red.set_high();

    let spawner = unsafe { Spawner::for_current_executor().await };
    unsafe {
        nrfxlib_sys::nrf_modem_gnss_prio_mode_enable();
    }

    sensors::NRF91_GNSS
        .init(ariel_os_nrf91_gnss::config::Config::default())
        .await;
    spawner.spawn(sensors::nrf91_gnss_runner()).unwrap();

    loop {
        if let Err(e) = sensors::NRF91_GNSS.trigger_measurement() {
            warn!("Failed to trigger GNSS measurement: {:?}", e);
        }
        let reading = sensors::NRF91_GNSS.wait_for_reading().await;

        debug!("Got GNSS reading: {:?}", defmt::Debug2Format(&reading));

        if let Ok(samples) = reading {
            let mut location = Location {
                altitude: 0.0,
                latitude: 0.0,
                longitude: 0.0,
                time_of_fix: 0,

                // TODO: populate these values
                heading: 0.0,
                horizontal_speed: 0.0,
                vertical_spedd: 0.0,
            };
            let mut found_altitude = false;
            let mut found_latitude = false;
            let mut found_longitude = false;

            let found_timestamp = match samples.time_of_fix_timestamp() {
                Ok(t) => {
                    location.time_of_fix = t as u64;
                    true
                }
                Err(e) => {
                    warn!("Failed to get time of fix: {:?}", e);
                    false
                }
            };

            for (channel, sample) in samples.samples() {
                match channel.label() {
                    Label::Altitude => {
                        if let Ok(value) = sample.value() {
                            debug!("altitude: {}", value);
                            location.altitude =
                                value as f32 / 10i32.pow((-channel.scaling()) as u32) as f32;
                            found_altitude = true;
                        }
                    }
                    Label::Latitude => {
                        if let Ok(value) = sample.value() {
                            location.latitude =
                                value as f32 / 10i32.pow((-channel.scaling()) as u32) as f32;
                            found_latitude = true;
                        }
                    }
                    Label::Longitude => {
                        if let Ok(value) = sample.value() {
                            location.longitude =
                                value as f32 / 10i32.pow((-channel.scaling()) as u32) as f32;
                            found_longitude = true;
                        }
                    }
                    _ => {}
                }
            }

            if found_altitude && found_latitude && found_longitude && found_timestamp {
                led_red.set_low();
                led_blue.set_high();
                debug!("updating location");
                let mut loc_lock = CURRENT_LOCATION.lock().await;
                *loc_lock = Some(location);
            } else {
                led_blue.set_low();
            }
        }
    }
}

#[ariel_os::task(autostart, peripherals)]
async fn updates(peripherals: UpdatePeripherals) {
    let device_id: String<TAG_NAME_MAX_LEN> = ariel_os::identity::device_id_bytes()
        .map(|slice| heapless::format!("{}", Hex(slice)).unwrap())
        .unwrap_or(String::from_str("unknown").unwrap());

    let mut led = Output::new(peripherals.led_green, Level::Low);
    let mut btn1 = Input::builder(peripherals.btn1, Pull::Up)
        .build_with_interrupt()
        .unwrap();
    let mut last_update_timestamp = Instant::now();

    loop {
        // Wait for the button being pressed or 60s, whichever comes first.
        info!("Waiting 60s before sending next update...");

        led.set_low();
        let _ = embassy_futures::select::select(btn1.wait_for_low(), Timer::after_secs(360)).await;
        led.set_high();
        // Prevent sending updates too frequently
        if last_update_timestamp.elapsed() < Duration::from_secs(10) {
            warn!("Update skipped to avoid sending updates too frequently");
            continue;
        }

        info!("Updating status...");
        let location = { *CURRENT_LOCATION.lock().await };
        debug!("Getting seen list");

        let (addresses_seen, decode_instant) = { SEEN.lock().await.clone() };

        let decode_age_secs =
            u16::try_from(Instant::now().duration_since(decode_instant).as_secs())
                .unwrap_or(u16::MAX);

        let detected_tags = addresses_seen
            .tags
            .iter()
            .map(|tag| DetectedTag {
                age: tag.age + decode_age_secs,
                ..tag.clone()
            })
            .collect();

        // Backend forces to have values instead of undefined, so we send possibly wrong data.
        let update = GatewayUpdate {
            location,
            detected_tags,
            // FIXME: get battery level
            battery_level: Some(100),
            // You may want to use another form of ID
            gateway_id: device_id.clone(),
            // FIXME: track time instead of relying on the last GPS update
            timestamp: location.map(|l| l.time_of_fix).unwrap_or(0),
        };

        // replace the last update
        let _ = LAST_UPDATE.lock(|s| s.borrow_mut().replace(update));
        last_update_timestamp = Instant::now();

        // ping the RD
        register_to_rd().await;
    }
}

async fn register_to_rd() {
    let client = ariel_os::coap::coap_client().await;

    let demoserver = COAP_ENDPOINT.parse().unwrap();

    info!("Sending POST to {}...", demoserver);
    let request = coap_request_implementations::Code::post()
        .with_path("/rd")
        .with_request_payload_slice(b"This is Ariel OS")
        .processing_response_payload_through(|p| {
            info!(
                "RD response is {:?}",
                core::str::from_utf8(p).map_err(|_| "not Unicode?")
            );
        });
    let response = client.to(demoserver).request(request).await;
    info!("Response {:?}", response.map_err(|_| "TransportError"));
}

#[ariel_os::task(autostart)]
async fn coap_run() {
    use coap_handler_implementations::{HandlerBuilder, SimpleRendered, new_dispatcher};

    let handler = new_dispatcher()
        // test route
        .at(&["hello"], SimpleRendered("Hello from Ariel OS"))
        // the route that returns the status
        .at(
            &["status"],
            ConstantSingleRecordReport::new(
                TypeHandler::new_minicbor_2(coap_handler_implementations::with_get(
                    StatusRenderer::new(),
                )),
                &[Attribute::Title("Gateway Status")],
            ),
        );

    ariel_os::coap::coap_run(handler).await;
}

struct StatusRenderer {}

impl StatusRenderer {
    pub fn new() -> StatusRenderer {
        StatusRenderer {}
    }
}

impl GetRenderable for StatusRenderer {
    type Get = GatewayUpdate;
    fn get(&mut self) -> Result<Self::Get, coap_message_utils::Error> {
        info!("GET /status");

        LAST_UPDATE
            .lock(|s| s.clone())
            .into_inner()
            .ok_or(coap_message_utils::Error::service_unavailable())
    }
}
