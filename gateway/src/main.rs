#![no_main]
#![no_std]
mod board;
mod pins;
mod sensors;

use core::{cell::RefCell, str::FromStr as _};

use coap_handler::Attribute;
use coap_handler_implementations::{GetRenderable, TypeHandler, wkc::ConstantSingleRecordReport};
use coap_request::Stack;
use embassy_nrf::{Peri, PeripheralType, gpio::Pin};
use embassy_sync::blocking_mutex::{self, raw::CriticalSectionRawMutex};
use embedded_io_async::BufRead;
use heapless::{String, Vec};

use ariel_os::{
    config::str_from_env,
    gpio::{Input, Level, Output, Pull},
    hal::{self, ltem, uart::Uart},
    log::{Debug2Format, debug, error, info, warn},
    sensors::{Label, Reading, Sensor, sensor::ReadingError},
    time::{Duration, Instant, Timer},
    uart::Baudrate,
};
use ariel_os_sensors_gnss_time_ext::GnssTimeExt as _;

use common_types::{DetectedTag, GatewayUpdate, Location, TAG_NAME_MAX_LEN, TagsSeen};

use crate::pins::Peripherals;

// Test server : 65.108.193.50:4230
const COAP_ENDPOINT: &str = str_from_env!("COAP_ENDPOINT", "The CoAP endpoint to connect to.");
const TIME_BETWEEN_UPDATES: Duration = Duration::from_secs(360);

static LAST_UPDATE: blocking_mutex::Mutex<CriticalSectionRawMutex, RefCell<Option<GatewayUpdate>>> =
    blocking_mutex::Mutex::new(RefCell::new(None));

async fn uart_receive<'a, REQ, TX, RX>(
    request_pin: Peri<'a, REQ>,
    uart_rx: Peri<'a, RX>,
    uart_tx: Peri<'a, TX>,
) -> TagsSeen
where
    REQ: PeripheralType + Pin,
    TX: PeripheralType + Pin,
    RX: PeripheralType + Pin,
{
    let mut config = hal::uart::Config::default();
    config.baudrate = Baudrate::_115200;

    let mut request = Output::new(request_pin, Level::Low);

    let mut rx_buf = [0u8; 32];
    let mut tx_buf = [0u8; 1];

    let uart = pins::ReceiverUart::new(uart_rx, uart_tx, &mut rx_buf, &mut tx_buf, config)
        .expect("Invalid UART configuration");
    request.set_high();

    let tags = wait_for_decoded_message(uart).await;

    request.set_low();
    tags
}

async fn wait_for_decoded_message(mut uart: Uart<'_>) -> TagsSeen {
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
                    debug!("Decoded packet of {} bytes", separator);
                    return decoded;
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

#[ariel_os::task(autostart)]
async fn gnss_runner() {
    // Single shot, give it 6 minutes to get a fix

    let mut config = ariel_os_sensor_nrf91_gnss::config::Config::default();
    config.operation_mode = ariel_os_sensor_nrf91_gnss::config::GnssOperationMode::SingleShot(360);
    config.power_mode = ariel_os_sensor_nrf91_gnss::config::GnssPowerSaveMode::DutyCycling;

    sensors::NRF91_GNSS.init(config).await;
    sensors::nrf91_gnss_runner().await;
}

enum UpdateLocationError {
    ReadingError(ReadingError),
    InvalidFix(Location),
}
#[cfg(feature = "fake-position")]
async fn get_location() -> Result<Location, UpdateLocationError> {
    Ok(Location {
        latitude: 0f32,
        longitude: 0f32,
        altitude: 0f32,
        heading: 0f32,
        horizontal_speed: 0f32,
        vertical_speed: 0f32,
        time_of_fix: 1781008373u64,
    })
}

#[cfg(not(feature = "fake-position"))]
async fn get_location() -> Result<Location, UpdateLocationError> {
    if let Err(e) = sensors::NRF91_GNSS.trigger_measurement() {
        warn!("Failed to trigger GNSS measurement: {:?}", e);
    }
    let samples = sensors::NRF91_GNSS
        .wait_for_reading()
        .await
        .map_err(UpdateLocationError::ReadingError)?;

    debug!("Got GNSS reading: {:?}", defmt::Debug2Format(&samples));

    let mut location = Location {
        altitude: 0.0,
        latitude: 0.0,
        longitude: 0.0,
        time_of_fix: 0,

        // TODO: populate these values
        heading: 0.0,
        horizontal_speed: 0.0,
        vertical_speed: 0.0,
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
        Ok(location)
    } else {
        Err(UpdateLocationError::InvalidFix(location))
    }
}

#[ariel_os::task(autostart, peripherals)]
async fn updates(mut peripherals: Peripherals) {
    let stack = ariel_os::net::network_stack().await.unwrap();
    let mut last_update_timestamp = Instant::from_ticks(0);

    let device_id: String<TAG_NAME_MAX_LEN> = ariel_os::identity::interface_eui48(1)
        .map(|eui| heapless::format!("{}", eui).unwrap())
        .unwrap_or(String::from_str("unknown").unwrap());

    info!("Device ID: {}", device_id.as_str());

    let mut led_green = Output::new(peripherals.user_interaction.led_green, Level::Low);
    let mut btn1 = Input::builder(peripherals.user_interaction.btn1, Pull::Up)
        .build_with_interrupt()
        .unwrap();
    let mut led_blue = Output::new(peripherals.user_interaction.led_blue, Level::Low);
    let mut led_red = Output::new(peripherals.user_interaction.led_red, Level::Low);

    led_red.set_high();
    led_green.set_high();
    led_blue.set_high();
    Timer::after_millis(500).await;
    led_red.set_high();
    led_green.set_low();
    led_blue.set_high();
    Timer::after_millis(500).await;
    led_red.set_low();
    led_green.set_high();
    led_blue.set_high();
    Timer::after_millis(500).await;

    ltem::disable();

    loop {
        // Wait for the button being pressed or 60s, whichever comes first.
        info!("Waiting before sending next update...");

        // Showing blue: waiting
        led_red.set_low();
        led_green.set_low();
        led_blue.set_high();

        // Try to send an update every TIME_BETWEEN_UPDATES, waiting for a gnss fix may make the duration between updates longer.

        let duration_to_wait = TIME_BETWEEN_UPDATES
            .checked_sub(last_update_timestamp.elapsed())
            .unwrap_or(Duration::from_ticks(0));

        if duration_to_wait.as_ticks() > 0 {
            let _ =
                embassy_futures::select::select(btn1.wait_for_low(), Timer::after_secs(360)).await;
        }

        // Prevent sending updates too frequently
        if last_update_timestamp.elapsed() < Duration::from_secs(10) {
            warn!("Update skipped to avoid sending updates too frequently");
            continue;
        }

        // Make sure LTEM is disabled
        ltem::disable();

        // Now cyan/light blue, GNSS aquisition in progress
        led_red.set_low();
        led_green.set_high();
        led_blue.set_high();

        info!("Requesting GNSS location");

        let mut location;
        loop {
            location = match get_location().await {
                Ok(loc) => Some(loc),
                Err(UpdateLocationError::InvalidFix(loc)) => {
                    warn!("Invalid fix : {:?}", Debug2Format(&loc));
                    None
                }
                Err(UpdateLocationError::ReadingError(e)) => {
                    error!("Readin error: {:?}", e);
                    None
                }
            };
            if location.is_some() {
                break;
            } else {
                // Color is now yellow, GNSS fix has failed at least once.
                led_red.set_high();
                led_green.set_high();
                led_blue.set_low();
            }
        }

        // Purple, receiving BLE devices from nRF5340
        led_red.set_high();
        led_green.set_low();
        led_blue.set_high();

        let addresses_seen = uart_receive(
            peripherals.uart.request.reborrow(),
            peripherals.uart.uart_rx.reborrow(),
            peripherals.uart.uart_tx.reborrow(),
        )
        .await;
        let decode_instant = Instant::now();

        info!("Enabling cellular networking");

        // White, enabling LTE-M
        led_red.set_high();
        led_green.set_high();
        led_blue.set_high();

        ltem::enable();
        stack.wait_link_up().await;

        info!("Cellular networking up");

        // Green: CoAP communication in progress
        led_red.set_low();
        led_green.set_high();
        led_blue.set_low();

        debug!("Getting seen list");

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

        // ping the RD
        register_to_rd().await;

        last_update_timestamp = Instant::now();

        // Leave the connection up for a bit, then disable the LTE-M
        Timer::after_secs(10).await;
        ltem::disable();
        stack.wait_link_down().await;
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
