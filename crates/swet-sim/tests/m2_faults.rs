//! M2: error screen, link-loss screen, walk keep-alive, flush only on change.

use swet_heart::Buttons;
use swet_heart::config;
use swet_heart::ui::popup::{Fault, Popup};
use swet_sim::Sim;

fn riding() -> Sim {
    let mut s = Sim::new();
    s.motor().set_speed_kmh(18);
    s.boot_to_ride();
    s
}

/// STATUS is polled every 600 ms (one of six 100 ms slots).
const STATUS_PERIOD: u32 = 600;

#[test]
fn motor_error_code_shows_full_screen() {
    let mut s = riding();
    assert_eq!(s.app().popup(), Popup::None);
    s.motor().status = 0x21;
    s.run_ms(STATUS_PERIOD + 100);
    assert_eq!(s.app().popup(), Popup::Fault(Fault::Code(0x21)));
    s.assert_screen("m2_error_21");
}

#[test]
fn hex_codes_use_letters() {
    let mut s = riding();
    s.motor().status = 0x2B;
    s.run_ms(STATUS_PERIOD + 100);
    assert_eq!(s.app().popup(), Popup::Fault(Fault::Code(0x2B)));
    s.assert_screen("m2_error_2b");
}

#[test]
fn braking_is_not_an_error() {
    let mut s = riding();
    s.motor().status = 0x03;
    s.run_ms(3 * STATUS_PERIOD);
    assert_eq!(s.app().popup(), Popup::None);
}

#[test]
fn error_clears_after_three_normal_replies() {
    let mut s = riding();
    s.motor().status = 0x21;
    s.run_ms(STATUS_PERIOD + 100);
    s.motor().status = 0x01;
    s.run_ms(STATUS_PERIOD + 100);
    assert_ne!(
        s.app().popup(),
        Popup::None,
        "one normal reply is not enough"
    );
    s.run_ms(2 * STATUS_PERIOD + 100);
    assert_eq!(s.app().popup(), Popup::None);
}

#[test]
fn m_dismisses_and_it_returns_after_ten_seconds() {
    let mut s = riding();
    s.motor().status = 0x21;
    s.run_ms(STATUS_PERIOD + 100);
    s.click(Buttons::M);
    assert_eq!(s.app().popup(), Popup::None);
    s.run_ms(config::FAULT_REPEAT_MS - 1000);
    assert_eq!(s.app().popup(), Popup::None);
    s.run_ms(1100);
    assert_eq!(s.app().popup(), Popup::Fault(Fault::Code(0x21)));
}

#[test]
fn a_new_code_shows_at_once_after_a_dismiss() {
    let mut s = riding();
    s.motor().status = 0x21;
    s.run_ms(STATUS_PERIOD + 100);
    s.click(Buttons::M);
    s.motor().status = 0x08;
    s.run_ms(STATUS_PERIOD + 100);
    assert_eq!(s.app().popup(), Popup::Fault(Fault::Code(0x08)));
}

#[test]
fn error_screen_swallows_buttons_except_power_off() {
    let mut s = riding();
    s.motor().status = 0x21;
    s.run_ms(STATUS_PERIOD + 100);
    s.click(Buttons::RIGHT);
    s.hold(Buttons::LEFT, 1300);
    assert_eq!(s.app().state().pas, 0);
    assert!(!s.app().state().walk);
    s.press(Buttons::PWR);
    s.run_ms(1100);
    assert!(s.hal().powered_off);
}

#[test]
fn link_loss_shows_dashes_and_recovery_clears_it() {
    let mut s = riding();
    s.motor().online = false;
    s.run_ms(config::MOTOR_LINK_TIMEOUT_MS + 200);
    assert_eq!(s.app().popup(), Popup::Fault(Fault::LinkLost));
    s.assert_screen("m2_link_lost");
    s.motor().online = true;
    s.run_ms(300);
    assert_eq!(s.app().popup(), Popup::None);
}

#[test]
fn power_on_without_motor_gets_a_grace_period() {
    let mut s = Sim::new();
    s.motor().online = false;
    s.run_ms(config::MOTOR_LINK_TIMEOUT_MS - 200);
    assert_eq!(s.app().popup(), Popup::None);
    s.run_ms(400);
    assert_eq!(s.app().popup(), Popup::Fault(Fault::LinkLost));
}

#[test]
fn display_is_flushed_only_when_the_frame_changes() {
    let mut s = riding();
    let before = s.hal().flushes;
    s.run_ms(2000); // speed, current and battery all steady
    assert_eq!(s.hal().flushes, before, "a still screen costs no SPI time");
    s.click(Buttons::RIGHT);
    assert_eq!(s.hal().flushes, before + 1);
}

#[test]
fn walk_keepalive_resends_pas_06_while_held() {
    fn walk_writes(log: &[u8]) -> usize {
        log.windows(4)
            .filter(|w| *w == [0x16, 0x0B, 0x06, 0x27])
            .count()
    }
    // default (Swang Stodva behaviour): sent once
    let mut s = riding();
    s.press(Buttons::LEFT);
    s.run_ms(4000);
    assert_eq!(walk_writes(&s.hal().uart_log), 1);

    let mut s = riding();
    s.app_mut().motor_mut().walk_keepalive_ms = 500;
    s.press(Buttons::LEFT);
    s.run_ms(4000); // walk starts after the 1 s hold, then every 500 ms
    let n = walk_writes(&s.hal().uart_log);
    assert!((6..=7).contains(&n), "got {n} walk writes");
}
