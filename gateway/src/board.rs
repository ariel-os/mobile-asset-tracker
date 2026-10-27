#[cfg(context = "nordic-thingy-91-x-nrf9151")]
use ariel_os::{log::info, log::trace, time::Timer};
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

use core::sync::atomic::{AtomicU16, Ordering};

#[cfg(context = "nordic-thingy-91-x-nrf9151")]
use crate::config::TIME_BETWEEN_VBAT_MEASURES;

static VBAT_MV: AtomicU16 = AtomicU16::new(0);

pub fn vbat_mv() -> Option<u16> {
    match VBAT_MV.load(Ordering::Relaxed) {
        0 => None,
        mv => Some(mv),
    }
}

//FIXME replace linear approximation with a real discharge table 
pub fn battery_percent(mv: u16) -> u8 {
    ((mv.clamp(3300, 4200) - 3300) as u32 * 100 / 900) as u8
}

#[cfg(context = "nordic-thingy-91-x-nrf9151")]
#[ariel_os::task(autostart, peripherals)]
async fn board_init(peripherals: Peripherals) {
    use ariel_os::{
        gpio::{Input, Pull},
        i2c::controller::{Kilohertz, highest_freq_in},
        log::debug,
    };
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
        debug!("Requesting battery measurement");

        // trigger TASKVBATMEASURE
        i2c_bus.write(0x6b, &[0x05, 0x00, 0x01]).await.unwrap();

        loop {
            debug!("Waiting for PMIC interrupt");

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

        let mut buffer_msb = [0u8; 1];
        i2c_bus
            .write_read(0x6b, &[0x05, 0x11], &mut buffer_msb)
            .await
            .unwrap();

        // Read ADCGP0RESULTLSBS
        let mut buffer_lsbs = [0u8; 1];
        i2c_bus
            .write_read(0x6b, &[0x05, 0x15], &mut buffer_lsbs)
            .await
            .unwrap();

        // 8 bits from buffer_msb and 2 bits from buffer_lsbs (the 2 least significant ones).
        let vbat_raw: u16 = (buffer_msb[0] as u16) << 2 | (buffer_lsbs[0] & 0x3) as u16;

        let vbat: u16 = (vbat_raw as u32 * 5000 / 1023) as u16;
        VBAT_MV.store(vbat, Ordering::Relaxed);

        trace!("VBAT: {}", vbat);

        // Timer::after_secs(2).await;
        Timer::after(TIME_BETWEEN_VBAT_MEASURES).await;
    }
}
