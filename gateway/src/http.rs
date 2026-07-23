use embassy_net::{
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use reqwless::{
    client::{HttpClient, TlsConfig, TlsVerify},
    headers::ContentType,
    request::{Method, RequestBuilder},
};

use ariel_os::{
    log::{debug, info},
    reexports::{
        embassy_net::{self, Stack},
        static_cell,
    },
};

use crate::config::BEARER_HEADER_VALUE;

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

static TCP_CLIENT_STATE: static_cell::ConstStaticCell<
    TcpClientState<MAX_CONCURRENT_CONNECTIONS, TCP_BUFFER_SIZE, TCP_BUFFER_SIZE>,
> = static_cell::ConstStaticCell::new(TcpClientState::new());
static TCP_CLIENT: static_cell::StaticCell<TcpClient<MAX_CONCURRENT_CONNECTIONS>> =
    static_cell::StaticCell::new();
static DNS_CLIENT: static_cell::StaticCell<DnsSocket> = static_cell::StaticCell::new();
static TLS_RX_BUFFER: static_cell::ConstStaticCell<[u8; TLS_READ_BUFFER_SIZE]> =
    static_cell::ConstStaticCell::new([0; TLS_READ_BUFFER_SIZE]);
static TLS_TX_BUFFER: static_cell::ConstStaticCell<[u8; TLS_WRITE_BUFFER_SIZE]> =
    static_cell::ConstStaticCell::new([0; TLS_WRITE_BUFFER_SIZE]);

pub async fn create_http_client<'a>(
    stack: Stack<'static>,
) -> HttpClient<'a, TcpClient<'a, MAX_CONCURRENT_CONNECTIONS>, DnsSocket<'a>> {
    let tcp_client_state = TCP_CLIENT_STATE.take();
    let tcp_client = TCP_CLIENT.init(TcpClient::new(stack, tcp_client_state));
    let dns_client = DNS_CLIENT.init(DnsSocket::new(stack));

    let tls_seed: u64 = rand_core::RngCore::next_u64(&mut ariel_os::random::crypto_rng());

    let tls_rx_buffer = TLS_RX_BUFFER.take();
    let tls_tx_buffer = TLS_TX_BUFFER.take();

    // We do not authenticate the server in this example, as that would require setting up a PSK
    // with the server.
    let tls_verify = TlsVerify::None;
    let tls_config = TlsConfig::new(tls_seed, tls_rx_buffer, tls_tx_buffer, tls_verify);

    HttpClient::new_with_tls(tcp_client, dns_client, tls_config)
}

pub async fn send_kuzzle_post_request(
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
