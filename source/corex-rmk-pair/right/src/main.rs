#![no_std]
#![no_main]
#[cfg(not(feature = "raytac-r-a14"))]
mod gesture_config;
#[cfg(feature = "raytac-r-a14")]
#[path = "gesture_config_raytac.rs"]
mod gesture_config;
mod paw3222;
mod paw3222_schedule;
mod paw_wire;
mod pointing_mode;
mod pairing_keys;
#[path = "../../shared/status_led.rs"]
mod status_led;
mod tuning;
mod tuning_values;
use rmk::macros::rmk_central;
// Raytac uses independent PAW3222 modules on J4 (device 0) and J3 (device 1).
#[rmk_central]
mod keyboard {
    #[register_processor(poll)]
    fn pairing_keys() -> crate::pairing_keys::PairingKeys {
        crate::pairing_keys::PairingKeys::new()
    }
    #[register_processor(poll)]
    fn status_leds() -> crate::status_led::StatusLeds {
        use embassy_nrf::gpio::{Level, Output, OutputDrive};
        #[cfg(not(feature = "raytac-r-a14"))]
        let lights = crate::status_led::Lights::Right {
                first: Output::new(p.P0_07, Level::Low, OutputDrive::Standard),
                second: Output::new(p.P1_09, Level::Low, OutputDrive::Standard),
            };
        #[cfg(feature = "raytac-r-a14")]
        let lights = crate::status_led::Lights::Raytac {
            first: [
                Output::new(p.P0_16, Level::High, OutputDrive::Standard),
                Output::new(p.P0_19, Level::High, OutputDrive::Standard),
                Output::new(p.P0_25, Level::High, OutputDrive::Standard),
            ],
            second: [
                Output::new(p.P0_20, Level::High, OutputDrive::Standard),
                Output::new(p.P0_11, Level::High, OutputDrive::Standard),
                Output::new(p.P0_23, Level::High, OutputDrive::Standard),
            ],
        };
        crate::status_led::StatusLeds::new(lights, true)
    }
    #[register_processor(poll)]
    async fn paw() -> crate::paw3222::PawSensors<'static> {
        use embassy_nrf::gpio::{Flex, Input, Level, Output, OutputDrive, Pull};
        // Q3 drives Q2 to enable VSENSE. R7/R8 and C10 need 350 ms to settle
        // before the generated battery ADC task takes its first sample.
        #[cfg(not(feature = "raytac-r-a14"))]
        let battery_enable = Output::new(p.P0_31, Level::High, OutputDrive::Standard);
        #[cfg(feature = "raytac-r-a14")]
        let battery_enable = Output::new(p.P1_10, Level::High, OutputDrive::Standard);
        embassy_time::Timer::after_millis(350).await;
        #[cfg(not(feature = "raytac-r-a14"))]
        let (wire, motion, chip_select_or_aux, spare_reset) = (
            crate::paw3222::GpioWire {
                sck: Output::new(p.P1_02, Level::High, OutputDrive::Standard),
                sdio: Flex::new(p.P1_06),
            },
            Input::new(p.P1_04, Pull::Up),
            Output::new(p.P1_01, Level::Low, OutputDrive::Standard),
            Some(Input::new(p.P0_21, Pull::Up)),
        );
        #[cfg(feature = "raytac-r-a14")]
        let (wire, motion, chip_select_or_aux, spare_reset) = (
            crate::paw3222::GpioWire {
                sck: Output::new(p.P0_05, Level::High, OutputDrive::Standard),
                sdio: Flex::new(p.P1_08),
            },
            Input::new(p.P0_07, Pull::Up),
            // Preserve the prototype's PORT 2 / J4 connector. NCS stays low.
            Output::new(p.P0_26, Level::Low, OutputDrive::Standard),
            None,
        );
        let primary = crate::paw3222::Paw3222::new(
            &keymap, 0, wire, motion, chip_select_or_aux, Some(battery_enable), spare_reset,
        );
        #[cfg(feature = "raytac-r-a14")]
        let secondary = Some(crate::paw3222::Paw3222::new(
            &keymap, 1,
            crate::paw3222::GpioWire {
                sck: Output::new(p.P0_08, Level::High, OutputDrive::Standard),
                sdio: Flex::new(p.P0_04),
            },
            Input::new(p.P0_06, Pull::Up),
            Output::new(p.P1_09, Level::Low, OutputDrive::Standard),
            None, None,
        ));
        #[cfg(not(feature = "raytac-r-a14"))]
        let secondary = None;
        crate::paw3222::PawSensors::new(primary, secondary)
    }
    #[register_processor(poll)]
    fn pointing_mode() -> crate::pointing_mode::PointingModeController<'static, { crate::paw3222::SENSOR_COUNT }> {
        crate::pointing_mode::PointingModeController::<{ crate::paw3222::SENSOR_COUNT }>::new(&keymap)
    }
}
