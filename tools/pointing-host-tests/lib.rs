//! Host checks of the production CoreX controller and RMK report path.
//! USB uses an in-memory transport; RMK's host-test flash task acknowledges writes.
#![allow(dead_code)]

#[path = "../../source/corex-rmk-pair/right/src/pairing_keys.rs"]
mod pairing_keys;

#[path = "../../source/corex-rmk-pair/right/src/gesture_config_raytac.rs"]
mod gesture_config;
#[path = "../../source/corex-rmk-pair/right/src/paw_wire.rs"]
mod paw_wire;
#[path = "../../source/corex-rmk-pair/right/src/pointing_mode.rs"]
mod pointing_mode;
#[path = "../../source/corex-rmk-pair/right/src/tuning.rs"]
mod tuning;
#[path = "../../source/corex-rmk-pair/right/src/tuning_values.rs"]
mod tuning_values;

#[cfg(test)]
mod ports {
    use super::pointing_mode::{PointingModeController, PointingModeControllerProcessorEventEnum};
    use core::convert::Infallible;
    use embassy_futures::select::select;
    use embassy_time::{Duration, Timer};
    use embedded_io_async::{ErrorType, Read, Write};
    use rmk::channel::USB_REPORT_CHANNEL;
    use rmk::config::{BehaviorConfig, PositionalConfig, RmkConfig};
    use rmk::core_traits::Runnable;
    use rmk::event::{Axis, AxisEvent, AxisValType, KeyboardEvent, PointingEvent};
    use rmk::hid::Report;
    use rmk::host::HostService;
    use rmk::keyboard::Keyboard;
    use rmk::keymap::{KeyMap, KeymapData};
    use rmk::processor::Processor;
    use rmk::test_support::test_block_on as block_on;
    use rmk::types::action::{Action, KeyAction};
    use rmk::types::connection::UsbState;
    use rmk::types::keycode::{HidKeyCode, KeyCode};

    struct Fixture {
        data: KeymapData<4, 7, 7>,
        behavior: BehaviorConfig,
        positions: PositionalConfig<4, 7>,
    }

    impl Fixture {
        fn new() -> Self {
            rmk::test_support::reset_connection_status();
            rmk::state::set_usb_state(UsbState::Configured);
            USB_REPORT_CHANNEL.clear();
            let mut keys = [[[KeyAction::No; 7]; 4]; 7];
            keys[0][0][0] = KeyAction::Single(Action::LayerOn(3));
            keys[0][0][1] = KeyAction::Single(Action::User(28));
            keys[0][0][6] = KeyAction::Single(Action::User(8)); // cursor 1/5
            keys[0][3][6] = KeyAction::Single(Action::User(15)); // scroll 1/20
            keys[0][1][6] = KeyAction::Single(Action::User(27)); // AML off
            keys[5][3][6] = KeyAction::Single(Action::Key(KeyCode::Hid(HidKeyCode::A)));
            keys[6][1][6] = KeyAction::Single(Action::User(30)); // gesture threshold 80
            Self {
                data: KeymapData::new(keys),
                behavior: BehaviorConfig::default(),
                positions: PositionalConfig::default(),
            }
        }

        fn keymap(&mut self) -> KeyMap<'_> {
            block_on(KeyMap::new(
                &mut self.data,
                &mut self.behavior,
                &self.positions,
            ))
        }
    }

    async fn motion<const N: usize>(
        controller: &mut PointingModeController<'_, N>,
        id: u8,
        x: i16,
        y: i16,
    ) {
        controller
            .process(PointingModeControllerProcessorEventEnum::Pointing(
                PointingEvent {
                    device_id: id,
                    axes: [
                        AxisEvent {
                            typ: AxisValType::Rel,
                            axis: Axis::X,
                            value: x,
                        },
                        AxisEvent {
                            typ: AxisValType::Rel,
                            axis: Axis::Y,
                            value: y,
                        },
                        AxisEvent {
                            typ: AxisValType::Rel,
                            axis: Axis::Z,
                            value: 0,
                        },
                    ],
                },
            ))
            .await;
    }

    fn mouse() -> (i16, i16, i16, i16) {
        let Report::MouseReport(report) = USB_REPORT_CHANNEL.try_receive().expect("mouse report")
        else {
            panic!("unexpected non-mouse report");
        };
        (report.x, report.y, report.wheel, report.pan)
    }

    fn no_report() {
        assert!(
            USB_REPORT_CHANNEL.try_receive().is_err(),
            "unexpected HID output"
        );
    }

    // Exercise a real Vial setting change; no production tuning logic is copied.
    struct Request {
        bytes: [u8; 32],
        offset: usize,
    }
    impl ErrorType for Request {
        type Error = Infallible;
    }
    impl Read for Request {
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Infallible> {
            let count = buf.len().min(self.bytes.len() - self.offset);
            buf[..count].copy_from_slice(&self.bytes[self.offset..self.offset + count]);
            self.offset += count;
            Ok(count)
        }
    }
    #[derive(Default)]
    struct Reply(Vec<u8>);
    impl ErrorType for Reply {
        type Error = Infallible;
    }
    impl Write for Reply {
        async fn write(&mut self, bytes: &[u8]) -> Result<usize, Infallible> {
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        async fn flush(&mut self) -> Result<(), Infallible> {
            Ok(())
        }
    }
    async fn cursor_setting<'a>(keymap: &'a KeyMap<'a>, user: u8) {
        setting(keymap, 0, user).await;
    }
    async fn setting<'a>(keymap: &'a KeyMap<'a>, layer: u8, user: u8) {
        let service = HostService::new(keymap, &RmkConfig::default());
        let mut request = Request {
            bytes: [0; 32],
            offset: 0,
        };
        request.bytes[..6].copy_from_slice(&[5, layer, 0, 6, 0x7e, user]);
        let mut reply = Reply::default();
        select(
            service.run_session(&mut request, &mut reply),
            rmk::test_support::drain_flash_channel(),
        )
        .await;
        assert_eq!(&reply.0[..6], &request.bytes[..6]);
        assert_eq!(
            keymap.action_at_pos(layer as usize, 0, 6),
            KeyAction::Single(Action::User(user))
        );
    }

    #[test]
    fn expanded_combo_slots_are_reported_and_all_round_trip_over_vial() {
        assert_eq!(rmk::types::constants::COMBO_MAX_NUM, 32);
        let mut fixture = Fixture::new();
        let keymap = fixture.keymap();
        block_on(async {
            let service = HostService::new(&keymap, &RmkConfig::default());
            async fn command(service: &HostService<'_>, bytes: &[u8]) -> Vec<u8> {
                let mut request = Request { bytes: [0;32], offset:0 };
                request.bytes[..bytes.len()].copy_from_slice(bytes);
                let mut reply = Reply::default();
                select(service.run_session(&mut request, &mut reply), rmk::test_support::drain_flash_channel()).await;
                reply.0
            }
            let counts = command(&service, &[0xfe,0x0d,0]).await;
            assert_eq!(counts[1],32);
            for slot in 0..32u8 {
                let result = command(&service, &[0xfe,0x0d,4,slot,4,0,5,0,0,0,0,0,40+slot,0]).await;
                assert_eq!(result[0],0);
            }
            for slot in 0..32u8 {
                let result = command(&service, &[0xfe,0x0d,3,slot]).await;
                assert_eq!(&result[..11], &[0,4,0,5,0,0,0,0,0,40+slot,0]);
            }
        });
    }

    #[test]
    fn both_ports_output_once_and_unknown_ids_are_ignored() {
        let mut fixture = Fixture::new();
        let keymap = fixture.keymap();
        let mut controller = PointingModeController::<2>::new(&keymap);
        block_on(async {
            for id in [0, 1] {
                motion(&mut controller, id, 10, -15).await;
                assert_eq!(mouse(), (-2, 3, 0, 0));
                no_report();
            }
            for id in [2, 255] {
                motion(&mut controller, id, 100, 100).await;
            }
            no_report();
        });
    }

    #[test]
    fn cursor_fractions_do_not_transfer_between_ports() {
        let mut fixture = Fixture::new();
        let keymap = fixture.keymap();
        let mut controller = PointingModeController::<2>::new(&keymap);
        block_on(async {
            motion(&mut controller, 0, 4, 0).await;
            motion(&mut controller, 1, 1, 0).await;
            no_report();
            motion(&mut controller, 0, 1, 0).await;
            assert_eq!(mouse(), (-1, 0, 0, 0));
            motion(&mut controller, 1, 4, 0).await;
            assert_eq!(mouse(), (-1, 0, 0, 0));
            no_report();
        });
    }

    #[test]
    fn scroll_fractions_are_independent_and_mode_changes_reach_both_ports() {
        let mut fixture = Fixture::new();
        let keymap = fixture.keymap();
        let mut keyboard = Keyboard::new(&keymap);
        let mut controller = PointingModeController::<2>::new(&keymap);
        block_on(async {
            keyboard.process_inner(KeyboardEvent::key(0, 0, true)).await;
            assert_eq!(keymap.active_layer(), 3);
            USB_REPORT_CHANNEL.clear();
            motion(&mut controller, 0, 0, 19).await;
            motion(&mut controller, 1, 0, 1).await;
            no_report();
            motion(&mut controller, 0, 0, 1).await;
            assert_eq!(mouse(), (0, 0, -1, 0));
            motion(&mut controller, 1, 0, 19).await;
            assert_eq!(mouse(), (0, 0, -1, 0));
            keyboard
                .process_inner(KeyboardEvent::key(0, 0, false))
                .await;
            assert_eq!(keymap.active_layer(), 0);
            USB_REPORT_CHANNEL.clear();
            for id in [0, 1] {
                motion(&mut controller, id, 5, 0).await;
                assert_eq!(mouse(), (-1, 0, 0, 0));
            }
            no_report();
        });
    }

    #[test]
    fn vial_scroll_direction_flips_each_axis_on_both_ports_only_in_scroll_mode() {
        let mut fixture = Fixture::new();
        let keymap = fixture.keymap();
        let mut keyboard = Keyboard::new(&keymap);
        let mut controller = PointingModeController::<2>::new(&keymap);
        block_on(async {
            keyboard.process_inner(KeyboardEvent::key(0, 0, true)).await;
            USB_REPORT_CHANNEL.clear();
            for (user, wheel, pan) in [(28,-1,1),(29,-1,-1),(30,1,1),(31,1,-1)] {
                setting(&keymap, 4, user).await;
                for port in [0,1] {
                    motion(&mut controller, port, 20, 20).await;
                    assert_eq!(mouse(), (0,0,wheel,pan));
                }
            }
            keyboard.process_inner(KeyboardEvent::key(0,0,false)).await;
            USB_REPORT_CHANNEL.clear();
            motion(&mut controller, 0, 5, 0).await;
            assert_eq!(mouse(), (-1,0,0,0));
        });
    }

    #[test]
    fn a_vial_gain_change_discards_old_fractions_on_both_ports() {
        let mut fixture = Fixture::new();
        let keymap = fixture.keymap();
        let mut controller = PointingModeController::<2>::new(&keymap);
        block_on(async {
            for id in [0, 1] {
                motion(&mut controller, id, 4, 0).await;
            }
            no_report();
            cursor_setting(&keymap, 9).await; // cursor 2/5
            for id in [0, 1] {
                motion(&mut controller, id, 1, 0).await;
            }
            no_report();
            for id in [0, 1] {
                motion(&mut controller, id, 2, 0).await;
                assert_eq!(mouse(), (-1, 0, 0, 0));
            }
            no_report();
        });
    }

    #[test]
    fn single_sensor_configuration_preserves_port_zero_and_rejects_port_one() {
        let mut fixture = Fixture::new();
        let keymap = fixture.keymap();
        let mut controller = PointingModeController::<1>::new(&keymap);
        block_on(async {
            motion(&mut controller, 1, 100, 100).await;
            no_report();
            motion(&mut controller, 0, 5, -10).await;
            assert_eq!(mouse(), (-1, 2, 0, 0));
            no_report();
        });
    }

    #[test]
    fn either_port_can_fire_the_shared_gesture_but_only_once_per_hold() {
        let mut fixture = Fixture::new();
        let keymap = fixture.keymap();
        let mut keyboard = Keyboard::new(&keymap);
        let mut controller = PointingModeController::<2>::new(&keymap);
        block_on(async {
            for first in [1, 0] {
                keyboard.process_inner(KeyboardEvent::key(0, 1, true)).await;
                USB_REPORT_CHANNEL.clear();
                motion(&mut controller, first, 80, 0).await;
                motion(&mut controller, 1 - first, 80, 0).await;
                // The real keyboard task consumes the shortcut queue and emits HID.
                select(keyboard.run(), Timer::after(Duration::from_millis(50))).await;
                let mut presses = 0;
                let mut releases = 0;
                while let Ok(report) = USB_REPORT_CHANNEL.try_receive() {
                    let Report::KeyboardReport(report) = report else {
                        panic!("gesture leaked cursor output");
                    };
                    if report.keycodes.contains(&(HidKeyCode::A as u8)) {
                        presses += 1;
                    }
                    if report.keycodes.iter().all(|key| *key == 0) {
                        releases += 1;
                    }
                }
                assert_eq!((presses, releases), (1, 1));
                keyboard
                    .process_inner(KeyboardEvent::key(0, 1, false))
                    .await;
            }
        });
    }
}
