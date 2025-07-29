#![no_main]
#![no_std]

use ariel_os::{debug::log::info, time::Timer};
use embassy_nrf::pac::gpio::vals::Mcusel;

#[ariel_os::task(autostart)]
async fn main() {
    embassy_nrf::reset::hold_network_core();
    let gpio0 = embassy_nrf::pac::P0;
    let gpio1 = embassy_nrf::pac::P1;

    gpio0
        .pin_cnf(29)
        .write(|w| w.set_mcusel(Mcusel::NETWORK_MCU));
    gpio1
        .pin_cnf(4)
        .write(|w| w.set_mcusel(Mcusel::NETWORK_MCU));
    gpio1
        .pin_cnf(6)
        .write(|w| w.set_mcusel(Mcusel::NETWORK_MCU));
    gpio1
        .pin_cnf(8)
        .write(|w| w.set_mcusel(Mcusel::NETWORK_MCU));

    let spu = embassy_nrf::pac::SPU;

    spu.periphid(8).perm().write(|w| w.set_secattr(false)); // UARTE0
    spu.periphid(9).perm().write(|w| w.set_secattr(false)); // UARTE1
    // spu.periphid(10).perm().write(|w| w.set_secattr(false));
    spu.periphid(11).perm().write(|w| w.set_secattr(false)); // UARTE2
    spu.periphid(12).perm().write(|w| w.set_secattr(false)); // UARTE3

    info!("Starting BLE Scan Reporter Demo...");
    // start the network core
    embassy_nrf::reset::release_network_core();

    loop {
        info!("FIXME: Implement proper core sleep");
        Timer::after_secs(100).await;
    }
}
