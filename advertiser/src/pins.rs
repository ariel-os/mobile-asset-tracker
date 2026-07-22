use ariel_os::hal::peripherals;

#[cfg(context = "seeedstudio-xiao-nrf52840-plus")]
ariel_os::hal::define_peripherals!(Peripherals {
    spi_sck: P0_21,
    spi_cs: P0_25,
    spi_io0: P0_20,
    spi_io1: P0_24,
    spi_io2: P0_22,
    spi_io3: P0_23,
    instance: QSPI,
});


#[cfg(context = "adafruit-feather-nrf52840-express")]
ariel_os::hal::define_peripherals!(Peripherals {
    spi_sck: P0_19,
    spi_cs: P0_20,
    spi_io0: P0_17,
    spi_io1: P0_22,
    spi_io2: P0_23,
    spi_io3: P0_21,
    instance: QSPI,
});
