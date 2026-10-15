use alloc::format;
use ariel_os::{
    log::{Debug2Format, info, warn},
    reexports::embassy_net::{self, Stack},
    time::Duration,
    time::Timer,
};
use embedded_tls::{Aes128GcmSha256, TlsConfig, TlsConnection, TlsContext};
use rust_mqtt::client::Client;
use rust_mqtt::{
    buffer::AllocBuffer,
    client::{
        event::Event,
        options::{ConnectOptions, DisconnectOptions, PublicationOptions, TopicReference},
    },
    config::SessionExpiryInterval,
    types::{MqttBinary, MqttString, TopicName},
};

use crate::config::*;

// RFC8449: TLS 1.3 encrypted records are limited to 16 KiB + 256 bytes.
const MAX_ENCRYPTED_TLS_13_RECORD_SIZE: usize = 16640;
// Required by `embedded_tls::TlsConnection::new()`.
const TLS_READ_BUFFER_SIZE: usize = MAX_ENCRYPTED_TLS_13_RECORD_SIZE;
// Can be smaller than the read buffer (could be adjusted: trade-off between memory usage and not
// splitting large writes into multiple records).
const TLS_WRITE_BUFFER_SIZE: usize = 4096;

const TCP_BUFFER_SIZE: usize = 1024;

pub async fn mqtt_connect_and_publish(client_id: &str, stack: Stack<'static>, data: &[u8]) {
    // Increase the buffer size if you want to send bigger packets.
    let mut rx_buffer = [0; TCP_BUFFER_SIZE];
    let mut tx_buffer = [0; TCP_BUFFER_SIZE];

    info!("waiting for interface to come up...");
    stack.wait_config_up().await;

    let mut socket = embassy_net::tcp::TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

    socket.set_timeout(Some(Duration::from_secs(30)));
    info!("Connecting...");
    let host_addr = MQTT_HOST_IP;
    if let Err(e) = socket.connect((host_addr, MQTT_HOST_PORT)).await {
        warn!("connect error: {:?}", e);
        Timer::after_secs(10).await;
        return;
    }
    info!("Connected to {:?}", socket.remote_endpoint());

    let mut tls_read_buf = [0u8; TLS_READ_BUFFER_SIZE];
    let mut tls_write_buf = [0u8; TLS_WRITE_BUFFER_SIZE];

    // host name is sent by SNI (used in SSL certificate)
    let tls_config = TlsConfig::new()
        .with_server_name(MQTT_HOST_NAME)
        .enable_rsa_signatures();

    //TODO We do not currently verify server certificate
    // We use Aes128GcmSha256 cipher
    let crypto_provider =
        embedded_tls::UnsecureProvider::new::<Aes128GcmSha256>(ariel_os::random::crypto_rng());

    let mut tls_connection = TlsConnection::new(socket, &mut tls_read_buf, &mut tls_write_buf);

    info!("TLS handshake...");
    if let Err(e) = tls_connection
        .open(TlsContext::new(&tls_config, crypto_provider))
        .await
    {
        warn!("TLS handshake error: {:?}", e);
        Timer::after_secs(10).await;
        return;
    }
    info!("TLS handshake OK");

    let transport = tls_connection;
    let mut buffer = AllocBuffer;

    const SUBS_MAX: usize = 1;
    const REC_MAX: usize = 1;
    const SEND_MAX: usize = 1;
    const MAX_SUBS_IDS: usize = 1;
    const MAX_USER_PROPS: usize = 16;
    const MAX_IN_TOPIC_ALIASES: usize = 1;
    const MAX_OUT_TOPIC_ALIASES: usize = 1;
    let mut client: Client<
        '_,
        '_,
        _, // transport type inferred from `client.connect(transport, ..)`
        AllocBuffer,
        SUBS_MAX,
        REC_MAX,
        SEND_MAX,
        MAX_SUBS_IDS,
        MAX_USER_PROPS,
        MAX_IN_TOPIC_ALIASES,
        MAX_OUT_TOPIC_ALIASES,
    > = Client::new(&mut buffer);

    let connect_options = ConnectOptions::new()
        .clean_start()
        .session_expiry_interval(SessionExpiryInterval::NeverEnd)
        .user_name(MqttString::from_str(MQTT_USER).unwrap())
        .password(MqttBinary::from_slice(MQTT_PWD.as_bytes()).unwrap());

    client
        .connect(
            transport,
            &connect_options,
            Some(MqttString::from_str(client_id).unwrap()),
        )
        .await
        .unwrap();

    // build the topinc string
    let topic_str = format!("{}/{}/{}", MQTT_TOPIC_ROOT, client_id, MQTT_TOPIC_DATA);
    let topic = MqttString::from_str(&topic_str).unwrap();

    let topic_reference = TopicReference::Name(TopicName::new(topic).unwrap());

    let _packet_identifier = client
        .publish(
            &PublicationOptions::new(topic_reference.as_borrowed()).exactly_once(),
            data.into(),
        )
        .await
        .unwrap()
        .unwrap();
    info!("After publish, {:?}", Debug2Format(&_packet_identifier));

    while let Ok(event) = client.poll().await {
        if let Event::PublishComplete(_) = event {
            // Publish succeeded, we can disconnect
            client.disconnect(&DisconnectOptions::new()).await.unwrap();
            return;
        }
    }
}
