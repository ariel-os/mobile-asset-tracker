#[cfg(context = "nordic-thingy-91-x-nrf9151")]
use ariel_os::hal::peripherals;
#[cfg(context = "nordic-thingy-91-x-nrf9151")]
pub type SensorI2c = ariel_os::hal::i2c::controller::SERIAL1;
#[cfg(context = "nordic-thingy-91-x-nrf9151")]
ariel_os::hal::define_peripherals!(Peripherals {
    i2c_sda: P0_09,
    i2c_scl: P0_08,
});

#[cfg(not(context = "nordic-thingy-91-x-nrf9151"))]
ariel_os::hal::define_peripherals!(Peripherals {});

#[cfg(context = "nordic-thingy-91-x-nrf9151")]
#[ariel_os::task(autostart, peripherals)]
async fn board_init(peripherals: Peripherals) {
    use ariel_os::i2c::controller::{Kilohertz, highest_freq_in};
    let mut i2c_config = ariel_os::hal::i2c::controller::Config::default();
    i2c_config.frequency = const { highest_freq_in(Kilohertz::kHz(100)..=Kilohertz::kHz(400)) };

    let mut i2c_bus = SensorI2c::new(peripherals.i2c_sda, peripherals.i2c_scl, i2c_config);

    ariel_os::hal::boards::init_thingy91x_board(&mut i2c_bus, true, true)
        .await
        .unwrap();
}
