use ariel_os::hal::peripherals;

#[cfg(context = "nrf52840")]
ariel_os::hal::define_peripherals!(Peripherals {
    spi_sck: P0_21,
    spi_miso: P0_24,
    spi_mosi: P0_20,
    spi_cs: P0_25,
    instance: SPI3,
});
