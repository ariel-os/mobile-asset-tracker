#![no_main]
#![no_std]

mod pins;

use embassy_futures::join::join;
use heapless::{FnvIndexMap, Vec};
use postcard::{
    ser_flavors::{Cobs, Slice},
    serialize_with_flavor,
};
use trouble_host::{
    Host,
    connection::{PhySet, ScanConfig},
    prelude::{BdAddr, EventHandler, LeAdvReportsIter},
    scan::Scanner,
};

use ariel_os::{
    debug::log::{debug, error, info, trace, warn},
    hal,
    time::{Duration, Instant, Timer},
    uart::Baudrate,
};
use common_types::{AddressesSeen, MAX_SEEN};
use embedded_io_async::BufRead;

#[ariel_os::task(autostart, peripherals)]
async fn get_scan_data(peripherals: pins::Peripherals) {
    let mut config = hal::uart::Config::default();
    config.baudrate = Baudrate::_115200;
    info!("Selected configuration: {:?}", config);

    let mut rx_buf = [0u8; 32];
    let mut tx_buf = [0u8; 32];

    let mut uart = pins::ReceiverUart::new(
        peripherals.uart_rx,
        peripherals.uart_tx,
        &mut rx_buf,
        &mut tx_buf,
        config,
    )
    .expect("Invalid UART configuration");
    let mut packet_buffer: Vec<u8, 2048> = Vec::new();

    // let mut buf = [0; 8];
    // buf.copy_from_slice(b"Hello!\r\n");

    // uart.write(&buf).await.unwrap();
    // info!("wrote hello in uart!");

    // loop {
    //     info!("reading...");
    //     let res = uart.read(&mut buf).await;
    //     if let Err(e) = res {
    //         error!("UART read error: {:?}", e);
    //         continue;
    //     } else {
    //         info!("read ok");
    //         info!("read: {:?}", &buf);
    //     }
    //     info!("writing...");
    //     uart.write(&buf).await.unwrap();
    // }

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
        let len = read.len();
        debug!("Read {} bytes from UART", read.len());
        packet_buffer.extend_from_slice(read).unwrap();

        uart.consume(len);
        if let Some(separator) = packet_buffer.iter().position(|&b| b == 0x00) {
            let packet = &mut packet_buffer[..separator];
            debug!("Received packet, trying to decode...");

            match postcard::from_bytes_cobs::<AddressesSeen>(packet) {
                Ok(decoded) => {
                    debug!("Decoded packet");
                    info!("Packet: {:?}", decoded.addrs);
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
