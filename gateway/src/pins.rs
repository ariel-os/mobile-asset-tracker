use ariel_os::hal::{peripherals, uart};

// VCOM1
#[cfg(context = "nordic-thingy-91-x-nrf9151")]
pub type ReceiverUart<'a> = uart::SERIAL3<'a>;
#[cfg(context = "nordic-thingy-91-x-nrf9151")]
ariel_os::hal::define_peripherals!(UartPeripherals {
    uart_tx: P0_05,
    uart_rx: P0_04,
});

#[cfg(context = "nordic-thingy-91-x-nrf9151")]
ariel_os::hal::define_peripherals!(UpdatePeripherals {
    btn1: P0_26,
    led_green: P0_31,
});

#[cfg(context = "nordic-thingy-91-x-nrf9151")]
ariel_os::hal::define_peripherals!(GnssStatusPeripherals {
    led_blue: P0_30,
    led_red: P0_29
});
