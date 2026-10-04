//! M1: ride screen, PAS page, speed/power views, gestures, motor I/O.

use swet_heart::Buttons;
use swet_heart::ui::Screen;
use swet_heart::ui::ride::{Page, View};
use swet_sim::Sim;

fn riding(kmh: u32) -> Sim {
    let mut s = Sim::new();
    s.motor().set_speed_kmh(kmh);
    s.motor().current_x2 = 24; // 12 A → 624 W
    s.boot_to_ride();
    s
}

#[test]
fn speed_view_shows_motor_speed() {
    let s = riding(27);
    assert!(s.app().motor().link_up());
    assert_eq!(s.app().view(), View::Speed);
    s.assert_screen("m1_speed_27");
}

#[test]
fn no_motor_shows_dashes() {
    let mut s = Sim::new();
    s.motor().online = false;
    s.run_ms(1000);
    assert!(!s.app().motor().link_up());
    s.assert_screen("m1_no_motor");
}

#[test]
fn right_left_change_pas_and_reach_the_motor() {
    let mut s = riding(0);
    for _ in 0..3 {
        s.click(Buttons::RIGHT);
    }
    s.run_ms(300); // next write slot
    assert_eq!(s.app().state().pas, 3);
    assert_eq!(s.hal().motor.pas_code, Some(0x0C));
    s.assert_screen("m1_pas_3");

    s.click(Buttons::LEFT);
    s.run_ms(300);
    assert_eq!(s.app().state().pas, 2);
    assert_eq!(s.hal().motor.pas_code, Some(0x0B));
}

#[test]
fn pas_stays_within_0_to_9() {
    let mut s = riding(0);
    s.click(Buttons::LEFT);
    assert_eq!(s.app().state().pas, 0);
    for _ in 0..12 {
        s.click(Buttons::RIGHT);
    }
    assert_eq!(s.app().state().pas, 9);
}

#[test]
fn pas_is_instant_no_double_click_wait() {
    let mut s = riding(0);
    s.press(Buttons::RIGHT);
    s.run_ms(60);
    s.release(Buttons::RIGHT);
    s.run_ms(40); // debounce only
    assert_eq!(s.app().state().pas, 1);
}

#[test]
fn hold_left_at_pas_0_is_walk_assist() {
    let mut s = riding(0);
    s.press(Buttons::LEFT);
    s.run_ms(1300);
    assert!(s.app().state().walk);
    assert_eq!(s.hal().motor.pas_code, Some(0x06));
    s.assert_screen("m1_walk");
    s.release(Buttons::LEFT);
    s.run_ms(300);
    assert!(!s.app().state().walk);
    assert_eq!(s.hal().motor.pas_code, Some(0x00));
}

#[test]
fn hold_left_above_pas_0_does_nothing() {
    let mut s = riding(0);
    s.click(Buttons::RIGHT);
    s.hold(Buttons::LEFT, 1300);
    assert!(!s.app().state().walk);
    assert_eq!(s.app().state().pas, 1, "a hold is not a click");
}

#[test]
fn m_double_click_switches_view_not_page() {
    let mut s = riding(20);
    s.double_click(Buttons::M);
    s.run_ms(400);
    assert_eq!(s.app().view(), View::Power);
    assert_eq!(s.app().page(), Page::Pas);
    s.assert_screen("m1_power");
    s.double_click(Buttons::M);
    s.run_ms(400);
    assert_eq!(s.app().view(), View::Speed);
}

#[test]
fn m_single_click_waits_for_the_double_click_window() {
    let mut s = riding(20);
    s.click(Buttons::M);
    s.run_ms(400);
    assert_eq!(
        s.app().view(),
        View::Speed,
        "single click is next page, not next view"
    );
}

#[test]
fn link_up_pushes_speed_limit_pas_and_lights() {
    let s = riding(0);
    let m = &s.hal().motor;
    assert_eq!(m.speed_limit_wire, Some(192), "25 km/h as wheel rpm");
    assert_eq!(m.pas_code, Some(0x00));
    assert_eq!(m.lights, Some(false));
}

#[test]
fn link_loss_and_recovery_resends_everything() {
    let mut s = riding(10);
    s.motor().online = false;
    s.run_ms(2500);
    assert!(!s.app().motor().link_up());
    // the controller "rebooted": it forgot what it was told
    {
        let m = s.motor();
        m.pas_code = None;
        m.speed_limit_wire = None;
        m.lights = None;
        m.online = true;
    }
    s.run_ms(1500);
    assert!(s.app().motor().link_up());
    let m = &s.hal().motor;
    assert_eq!(
        (m.speed_limit_wire, m.pas_code, m.lights),
        (Some(192), Some(0), Some(false))
    );
}

#[test]
fn m_hold_opens_diagnostics_and_pwr_goes_back() {
    let mut s = riding(0);
    s.hold(Buttons::M, 1200);
    assert_eq!(s.app().screen(), Screen::Diag);
    s.click(Buttons::M);
    assert_eq!(s.hal().orient, 1, "M cycles the display orientation");
    s.click(Buttons::PWR);
    assert_eq!(s.app().screen(), Screen::Ride);
}

#[test]
fn pwr_hold_powers_off_but_not_the_press_that_switched_it_on() {
    let mut s = Sim::new();
    s.press(Buttons::PWR);
    s.run_ms(3000);
    assert!(!s.hal().powered_off);
    s.release(Buttons::PWR);
    s.run_ms(100);
    s.press(Buttons::PWR);
    s.run_ms(1100);
    assert!(s.hal().powered_off);
}

#[test]
fn motor_bus_keeps_one_request_per_100_ms_slot() {
    let mut s = riding(15);
    let before = s.app().motor().diag;
    s.run_ms(6000);
    let after = s.app().motor().diag;
    let frames = (after.requests - before.requests) + (after.writes - before.writes);
    assert_eq!(frames, 60);
    assert_eq!(
        after.timeouts, before.timeouts,
        "fake motor answers within the slot"
    );
    assert_eq!(after.bad_checksums, 0);
}
