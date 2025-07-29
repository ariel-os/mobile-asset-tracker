use ariel_os::hal::{peripherals, uart};

#[cfg(context = "nrf52840dk")]
pub type ReceiverUart<'a> = uart::UARTE0<'a>;
#[cfg(context = "nrf52840dk")]
ariel_os::hal::define_peripherals!(Peripherals {
    uart_tx: P0_08,
    uart_rx: P0_06,
});

#[cfg(context = "nrf52dk")]
pub type ReceiverUart<'a> = uart::UARTE0<'a>;
#[cfg(context = "nrf52dk")]
ariel_os::hal::define_peripherals!(Peripherals {
    uart_tx: P0_08,
    uart_rx: P0_06,
});


#[cfg(context = "nrf9151")]
pub type ReceiverUart<'a> = uart::SERIAL3<'a>;
#[cfg(context = "nrf9151")]
ariel_os::hal::define_peripherals!(Peripherals {
    uart_tx: P0_05,
    uart_rx: P0_04,
});
