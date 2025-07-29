use ariel_os::hal::peripherals;

// VCOM1
#[cfg(context = "nrf5340-net")]
ariel_os::hal::define_peripherals!(Peripherals {
    uart_tx: P1_08,
    uart_rx: P1_06,
    serial: SERIAL0,
});


#[cfg(context = "nrf52dk")]
ariel_os::hal::define_peripherals!(Peripherals {
    uart_tx: P0_00,
    uart_rx: P0_01,
    serial: UARTE0,
});


#[cfg(context = "nrf52840dk")]
ariel_os::hal::define_peripherals!(Peripherals {
    uart_tx: P0_06,
    uart_rx: P0_08,
    serial: UARTE0,
});
