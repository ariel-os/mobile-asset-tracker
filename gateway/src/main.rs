#![no_main]
#![no_std]
mod board;
mod pins;
mod sensors;

extern crate alloc;

use core::str::FromStr as _;

use embassy_net::{
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use embedded_io_async::BufRead;
use heapless::{String, Vec};
use reqwless::{
    client::{HttpClient, TlsConfig, TlsVerify},
    headers::ContentType,
    request::{Method, RequestBuilder},
};

use ariel_os::{
    config::str_from_env,
    gpio::{Input, Level, Output, Pull},
    hal::{self, ltem, uart::Uart},
    log::{Debug2Format, debug, error, info, warn},
    reexports::embassy_net,
    sensors::{Label, Reading, Sensor, sensor::ReadingError},
    time::{Duration, Instant, Timer},
    uart::Baudrate,
};
use ariel_os_sensors_gnss_time_ext::GnssTimeExt as _;

use common_types::{DetectedTag, GatewayUpdate, Location, TAG_NAME_MAX_LEN, TagsSeen};

use crate::pins::Peripherals;

// RFC8449: TLS 1.3 encrypted records are limited to 16 KiB + 256 bytes.
const MAX_ENCRYPTED_TLS_13_RECORD_SIZE: usize = 16640;
// Required by `embedded_tls::TlsConnection::new()`.
const TLS_READ_BUFFER_SIZE: usize = MAX_ENCRYPTED_TLS_13_RECORD_SIZE;
// Can be smaller than the read buffer (could be adjusted: trade-off between memory usage and not
// splitting large writes into multiple records).
const TLS_WRITE_BUFFER_SIZE: usize = 4096;

const TCP_BUFFER_SIZE: usize = 1024;
const HTTP_BUFFER_SIZE: usize = 1024;

const MAX_CONCURRENT_CONNECTIONS: usize = 2;

const KUZZLE_ENDPOINT: &str = str_from_env!("KUZZLE_ENDPOINT", "Kuzzle endpoint to connect to.");
const KUZZLE_TOKEN: &str = str_from_env!("KUZZLE_TOKEN", "Kuzzle token.");

const BEARER_HEADER_VALUE: &str = const_str::format!("Bearer {}", KUZZLE_TOKEN);
const TIME_BETWEEN_UPDATES: Duration = Duration::from_secs(360);

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

    // initialisation du client HTTP
    let tcp_client_state =
        TcpClientState::<MAX_CONCURRENT_CONNECTIONS, TCP_BUFFER_SIZE, TCP_BUFFER_SIZE>::new();
    let tcp_client = TcpClient::new(stack, &tcp_client_state);
    let dns_client = DnsSocket::new(stack);

    let tls_seed: u64 = rand_core::RngCore::next_u64(&mut ariel_os::random::crypto_rng());

    let mut tls_rx_buffer = [0; TLS_READ_BUFFER_SIZE];
    let mut tls_tx_buffer = [0; TLS_WRITE_BUFFER_SIZE];

    // We do not authenticate the server in this example, as that would require setting up a PSK
    // with the server.
    let tls_verify = TlsVerify::None;
    let tls_config = TlsConfig::new(tls_seed, &mut tls_rx_buffer, &mut tls_tx_buffer, tls_verify);

    let mut client = HttpClient::new_with_tls(&tcp_client, &dns_client, tls_config);

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

    let mut uart_request = Output::new(peripherals.uart.request, Level::Low);

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

    let mut rx_buf = [0u8; 32];
    let mut tx_buf = [0u8; 1];

    let mut uart_config = hal::uart::Config::default();
    uart_config.baudrate = Baudrate::_115200;

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

        let detected_tags = {
            let uart = pins::ReceiverUart::new(
                peripherals.uart.uart_rx.reborrow(),
                peripherals.uart.uart_tx.reborrow(),
                &mut rx_buf,
                &mut tx_buf,
                uart_config,
            )
            .expect("Invalid UART configuration");
            uart_request.set_high();

            let detected_tags = wait_for_decoded_message(uart).await;

            uart_request.set_low();
            detected_tags
        };

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

        debug!("Updating detected tags age");

        let decode_age_secs =
            u16::try_from(Instant::now().duration_since(decode_instant).as_secs())
                .unwrap_or(u16::MAX);

        let detected_tags = detected_tags
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

        debug!("Serializing response");

        let body = match serde_json::to_vec(&update) {
            Ok(b) => b,
            Err(e) => {
                error!("Failed to serialize update to JSON: {:?}", Debug2Format(&e));
                continue;
            }
        };

        info!("json : {:?}", Debug2Format(&serde_json::to_string(&update).unwrap()));

        debug!("Sending request");
        if let Err(err) =
            send_kuzzle_post_request(&mut client, KUZZLE_ENDPOINT, body.as_slice()).await
        {
            error!(
                "Error while sending an HTTP request: {:?}",
                Debug2Format(&err)
            );
        }

        debug!("Update sent to backend");

        last_update_timestamp = Instant::now();

        ltem::disable();
        // stack.wait_link_down().await;
    }
}

async fn send_kuzzle_post_request(
    client: &mut HttpClient<'_, TcpClient<'_, MAX_CONCURRENT_CONNECTIONS>, DnsSocket<'_>>,
    url: &str,
    body: &[u8],
) -> Result<(), reqwless::Error> {
    let mut http_rx_buf = [0; HTTP_BUFFER_SIZE];

    let headers = [("Authorization", BEARER_HEADER_VALUE)];

    debug!("Creating handle, body len {}", body.len());

    let mut handle = client
        .request(Method::POST, url)
        .await?
        .headers(&headers)
        .body(body)
        .content_type(ContentType::ApplicationJson);

    debug!("Executing request");

    let response = handle.send(&mut http_rx_buf).await?;

    info!("Response status: {}", response.status.0);

    if let Some(ref content_type) = response.content_type {
        info!("Response Content-Type: {}", content_type.as_str());
    }

    if let Ok(body) = response.body().read_to_end().await {
        if let Ok(body) = core::str::from_utf8(&body) {
            info!("Response body:\n{}", body);
        } else {
            info!("Received a response body, but it is not valid UTF-8");
        }
    } else {
        info!("No response body");
    }

    Ok(())
}
