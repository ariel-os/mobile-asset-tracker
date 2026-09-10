#[cfg(context = "nordic-thingy-91-x-nrf9151")]
use ariel_os::{hal::peripherals, log::info, time::Timer};
#[cfg(context = "nordic-thingy-91-x-nrf9151")]
pub type SensorI2c = ariel_os::hal::i2c::controller::SERIAL1;
#[cfg(context = "nordic-thingy-91-x-nrf9151")]
ariel_os::hal::define_peripherals!(Peripherals {
    i2c_sda: P0_09,
    i2c_scl: P0_08,

    pmic_interrupt: P0_02,
});

#[cfg(not(context = "nordic-thingy-91-x-nrf9151"))]
ariel_os::hal::define_peripherals!(Peripherals {});

#[cfg(context = "nordic-thingy-91-x-nrf9151")]
#[ariel_os::task(autostart, peripherals)]
async fn board_init(peripherals: Peripherals) {
    use ariel_os::gpio::{Input, Pull};
    use ariel_os::i2c::controller::{Kilohertz, highest_freq_in};
    use embedded_hal_async::i2c::I2c;

    let mut pmic_interrupt = Input::builder(peripherals.pmic_interrupt, Pull::Down)
        .build_with_interrupt()
        .unwrap();

    let mut i2c_config = ariel_os::hal::i2c::controller::Config::default();
    i2c_config.frequency = const { highest_freq_in(Kilohertz::kHz(100)..=Kilohertz::kHz(400)) };

    let mut i2c_bus = SensorI2c::new(peripherals.i2c_sda, peripherals.i2c_scl, i2c_config);

    ariel_os::hal::boards::init_thingy91x_board(&mut i2c_bus, true, true)
        .await
        .unwrap();


    i2c_bus.write(0x6b, &[0x06, 0x03, 0x05]).await.unwrap();

    // Enable EVENTADCVBATRDY interrupt in INTENEVENTSADCSET.
    i2c_bus.write(0x6b, &[0x00, 0x04, 0x01]).await.unwrap();

    loop {
        debug!("Requesting measurement");

        // trigger TASKVBATMEASURE
        i2c_bus.write(0x6b, &[0x05, 0x00, 0x01]).await.unwrap();

        loop {
            debug!("Waiting for interrupt");

            pmic_interrupt.wait_for_high().await;

            let mut buffer = [0u8; 1];

            i2c_bus
                .write_read(0x6b, &[0x00, 0x02], &mut buffer)
                .await
                .unwrap();

            debug!("EVENTSADCSET {}", buffer[0]);

            // If EVENTADCVBATRDY is set
            if buffer[0] & 0x01 == 1 {
                // Clear EVENTSADCCLR
                i2c_bus.write(0x6b, &[0x00, 0x03, 0x01]).await.unwrap();
                break;
            }
        }

        // Read ADCVBATRESULTMSB

        let mut buffer_msb = [0u8, 1];
        i2c_bus
            .write_read(0x6b, &[0x05, 0x11], &mut buffer_msb)
            .await
            .unwrap();

        // Read ADCGP0RESULTLSBS
        let mut buffer_lsbs = [0u8, 1];
        i2c_bus
            .write_read(0x6b, &[0x05, 0x15], &mut buffer_lsbs)
            .await
            .unwrap();

        // 8 bits from buffer_msb and 2 bits from buffer_lsbs (the 2 least significant ones).
        let vbat_raw: u16 = (buffer_msb[0] as u16) << 2 | (buffer_lsbs[0] & 0x3) as u16;

        let vbat = (vbat_raw as f32 / 1023.0) * 5.0;

        info!("VBAT: {}", vbat);

        Timer::after_secs(2).await;
    }
}
