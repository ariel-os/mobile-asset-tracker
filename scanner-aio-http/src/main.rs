#![no_main]
#![no_std]

mod config;

use core::str::FromStr;

use mobile_asset_tracker_scanner::TagScanner;
use reqwless::{
    client::{HttpClient, TlsConfig, TlsVerify},
    headers::ContentType,
    request::{Method, RequestBuilder},
};

use ariel_os::{
    identity::Eui48,
    log::{debug, info},
    reexports::embassy_net::{
        dns::DnsSocket,
        tcp::client::{TcpClient, TcpClientState},
    },
};

use common_types::TAG_NAME_MAX_LEN;

use config::*;

static TRACKER_SCANNER: TagScanner = TagScanner::new(TAG_PREFIX);

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

#[ariel_os::task(autostart)]
async fn receive_tags() {
    let stack = ariel_os::net::network_stack().await.unwrap();
    stack.wait_config_up().await;

    // initialisation du client HTTP
    let tcp_client_state =
        TcpClientState::<MAX_CONCURRENT_CONNECTIONS, TCP_BUFFER_SIZE, TCP_BUFFER_SIZE>::new();
    let tcp_client = TcpClient::new(stack, &tcp_client_state);
    let dns_client = DnsSocket::new(stack);

    let tls_seed: u64 = rand_core::RngCore::next_u64(&mut ariel_os::random::crypto_rng());

    let mut tls_rx_buffer = [0; TLS_READ_BUFFER_SIZE];
    let mut tls_tx_buffer = [0; TLS_WRITE_BUFFER_SIZE];
    let tls_verify = TlsVerify::None;
    let tls_config = TlsConfig::new(tls_seed, &mut tls_rx_buffer, &mut tls_tx_buffer, tls_verify);

    let mut client = HttpClient::new_with_tls(&tcp_client, &dns_client, tls_config);

    let device_id: heapless::String<TAG_NAME_MAX_LEN> = SNIFFER_ID
        .map(|id| heapless::String::from_str(id).ok())
        .unwrap_or(
            ariel_os::identity::interface_eui48(1)
                .map(|eui| heapless::format!("{}", eui).unwrap())
                .ok(),
        )
        .unwrap_or(
            heapless::String::try_from("unknown")
                .expect("heapless string conversion from constant"),
        );

    info!("Device ID: {}", device_id.as_str());

    let receiver = TRACKER_SCANNER.receiver();

    loop {
        let value = receiver.receive().await;

        let mut addr = value.addr.into_inner();
        // To big endian representation
        addr.reverse();
        let mac = heapless::format!("{}", Eui48(addr)).expect("Correct EUI48 value");

        let payload = common_types::AdvertisementPayload {
            adv_type: heapless::String::try_from("").unwrap(),
            channel: 0,
            mac,
            name: value.name,
            raw_rssi: value.rssi,
            rssi: value.rssi,
            sequence: value.sequence,
            sniffer_id: device_id.clone(),
            // TODO: proper time
            timestamp: value.timestamp.as_millis(),
        };

        let body = serde_json::to_vec(&payload).unwrap();

        send_data_to_backend(&mut client, BACKEND_ENDPOINT, &body)
            .await
            .unwrap();
    }
}

#[ariel_os::task(autostart)]
async fn run_scanner() {
    info!("starting ble stack");

    let host = ariel_os::ble::ble_stack().await.build();

    TRACKER_SCANNER.run(host, SCAN_INTERVAL, SCAN_WINDOW).await
}

async fn send_data_to_backend(
    client: &mut HttpClient<'_, TcpClient<'_, MAX_CONCURRENT_CONNECTIONS>, DnsSocket<'_>>,
    url: &str,
    body: &[u8],
) -> Result<(), reqwless::Error> {
    let mut http_rx_buf = [0; HTTP_BUFFER_SIZE];

    debug!("Creating handle, body len {}", body.len());

    let mut handle = client
        .request(Method::POST, url)
        .await?
        // .headers(&headers)
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
