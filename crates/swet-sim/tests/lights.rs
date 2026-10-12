//! Lights (PRODUCT §3.5): OFF / ON / AUTO from the menu, AUTO from the
//! phone's time sync, the motor light, screen brightness and telemetry
//! following the effective state.

use swet_heart::blep::{self, TimeSync};
use swet_heart::lights::LightsMode;
use swet_heart::ui::Screen;
use swet_heart::ui::menu::Item;
use swet_heart::{BleChannel, Buttons, config};
use swet_sim::Sim;

const fn hms(h: u32, m: u32, s: u32) -> u32 {
    h * 3600 + m * 60 + s
}

const SUNSET: u16 = 19 * 60;
const SUNRISE: u16 = 7 * 60;

fn riding() -> Sim {
    let mut s = Sim::new();
    s.boot_to_ride();
    s
}

/// The effective state everywhere it shows: state, motor (after the next
/// write slot), screen brightness.
fn assert_lights(s: &mut Sim, on: bool) {
    s.run_ms(300);
    assert_eq!(s.app().state().lights, on, "state");
    assert_eq!(s.hal().motor.lights, Some(on), "motor light");
    let contrast = if on {
        config::CONTRAST_NIGHT
    } else {
        config::CONTRAST_DAY
    };
    assert_eq!(s.hal().contrast, contrast, "screen brightness");
}

/// Menu → Lights → `mode` → M, back on the ride screen.
fn pick(s: &mut Sim, mode: LightsMode) {
    s.hold(Buttons::M, 1200);
    assert_eq!(s.app().menu_item(), Item::Lights, "the first item");
    s.click(Buttons::M);
    assert_eq!(s.app().screen(), Screen::Lights);
    while s.app().lights_choice() != mode {
        s.click(Buttons::RIGHT);
    }
    s.click(Buttons::M);
    assert_eq!(s.app().screen(), Screen::Menu);
    s.click(Buttons::PWR);
    assert_eq!(s.app().screen(), Screen::Ride);
    assert_eq!(s.app().lights().mode, mode);
}

#[test]
fn off_at_power_on() {
    let mut s = riding();
    assert_eq!(s.app().lights().mode, LightsMode::Off);
    assert_eq!(s.app().lights().clock.secs_of_day(s.now_ms()), None);
    assert_lights(&mut s, false);
}

#[test]
fn the_first_time_sync_switches_to_auto() {
    let mut s = riding();
    s.phone_connect();
    s.phone_send_time(hms(21, 0, 0), SUNSET, SUNRISE);
    assert_eq!(s.app().lights().mode, LightsMode::Auto);
    assert_lights(&mut s, true);
    s.assert_screen("lights_on_ride");
    s.run_ms(1000);
    let tel = s.notified(BleChannel::Telemetry);
    assert_eq!(tel.last().expect("telemetry")[8] & 1, 1, "flags bit0");

    // AUTO in daylight: off, telemetry says so
    let mut s = riding();
    s.phone_connect();
    s.phone_send_time(hms(12, 0, 0), SUNSET, SUNRISE);
    assert_eq!(s.app().lights().mode, LightsMode::Auto);
    assert_lights(&mut s, false);
    s.run_ms(1000);
    let tel = s.notified(BleChannel::Telemetry);
    assert_eq!(tel.last().expect("telemetry")[8] & 1, 0);
}

#[test]
fn auto_turns_on_twenty_minutes_before_sunset() {
    let mut s = riding();
    // 18:39:50, sunset 19:00: on at 18:40
    s.phone_send_time(hms(18, 39, 50), SUNSET, SUNRISE);
    assert_lights(&mut s, false);
    s.run_ms(9_000); // 18:40:00 minus the 300 ms in assert_lights
    assert!(!s.app().state().lights, "18:39:59");
    s.run_ms(1_000);
    assert_lights(&mut s, true);
}

#[test]
fn auto_turns_off_at_sunrise() {
    let mut s = riding();
    s.phone_send_time(hms(6, 59, 55), SUNSET, SUNRISE);
    assert_lights(&mut s, true);
    s.run_ms(5_000);
    assert_lights(&mut s, false);
}

#[test]
fn the_clock_wraps_at_midnight() {
    let mut s = riding();
    // sunset 23:50 (on from 23:30), sunrise 00:00: dark until midnight only
    s.phone_send_time(hms(23, 59, 57), 23 * 60 + 50, 0);
    assert_lights(&mut s, true);
    s.run_ms(3_000);
    assert_lights(&mut s, false);
    assert!(
        s.app()
            .lights()
            .clock
            .secs_of_day(s.now_ms())
            .expect("synced")
            < 10
    );
}

#[test]
fn a_menu_choice_after_the_sync_is_not_overridden() {
    let mut s = riding();
    s.phone_connect();
    s.phone_send_time(hms(21, 0, 0), SUNSET, SUNRISE);
    assert_lights(&mut s, true);
    pick(&mut s, LightsMode::Off);
    assert_lights(&mut s, false);

    // the phone syncs again ten minutes later, and after a reconnect
    s.run_ms(600);
    s.phone_send_time(hms(21, 10, 0), SUNSET, SUNRISE);
    s.phone_disconnect();
    s.run_ms(1000);
    s.phone_connect();
    s.phone_send_time(hms(21, 11, 0), SUNSET, SUNRISE);
    assert_eq!(s.app().lights().mode, LightsMode::Off);
    assert_lights(&mut s, false);

    // the rider can go back to AUTO, which uses the latest time
    pick(&mut s, LightsMode::Auto);
    assert_lights(&mut s, true);
}

#[test]
fn without_a_phone_the_rider_picks_on_and_off() {
    let mut s = riding();
    s.hold(Buttons::M, 1200);
    s.assert_screen("lights_menu_item");
    s.click(Buttons::M);
    s.assert_screen("lights_choice_no_time");
    s.click(Buttons::PWR); // back: nothing changes
    assert_eq!(s.app().screen(), Screen::Menu);
    s.click(Buttons::PWR);
    assert_eq!(s.app().lights().mode, LightsMode::Off);

    pick(&mut s, LightsMode::On);
    assert_lights(&mut s, true);
    pick(&mut s, LightsMode::Auto);
    assert_lights(&mut s, false); // no time: AUTO is off
    pick(&mut s, LightsMode::Off);
    assert_lights(&mut s, false);
}

#[test]
fn a_choice_made_before_the_first_sync_is_kept() {
    let mut s = riding();
    pick(&mut s, LightsMode::On);
    s.phone_connect();
    s.phone_send_time(hms(12, 0, 0), SUNSET, SUNRISE);
    assert_eq!(s.app().lights().mode, LightsMode::On);
    assert_lights(&mut s, true);
}

#[test]
fn the_choice_screen_shows_the_time_and_when_auto_switches() {
    let mut s = riding();
    s.phone_send_time(hms(21, 3, 0), SUNSET, SUNRISE);
    s.hold(Buttons::M, 1200);
    s.assert_screen("lights_menu_item_auto");
    s.click(Buttons::M);
    s.assert_screen("lights_choice_auto");
}

#[test]
fn bad_time_syncs_are_ignored() {
    let mut s = riding();
    let good = blep::encode_time_sync(&TimeSync {
        secs_of_day: hms(21, 0, 0),
        sunset_min: SUNSET,
        sunrise_min: SUNRISE,
    });
    let mut bad: Vec<Vec<u8>> = vec![
        good[..8].to_vec(),
        [&good[..], &[0]].concat(),
        b"T".to_vec(),
        Vec::new(),
    ];
    for (i, v) in [
        (0, &b"t"[..]),
        (1, &86_400u32.to_le_bytes()[..]),
        (5, &1440u16.to_le_bytes()[..]),
        (7, &u16::MAX.to_le_bytes()[..]),
    ] {
        let mut b = good;
        b[i..i + v.len()].copy_from_slice(v);
        bad.push(b.to_vec());
    }
    for b in &bad {
        s.phone_write_control(b);
    }
    s.run_ms(100);
    assert_eq!(s.app().lights().mode, LightsMode::Off);
    assert_eq!(s.app().lights().clock.secs_of_day(s.now_ms()), None);
    assert_lights(&mut s, false);
    assert_eq!(s.app().screen(), Screen::Ride);

    // a bad one doesn't count as "the first sync" either
    s.phone_write_control(&good);
    assert_eq!(s.app().lights().mode, LightsMode::Auto);
}

#[test]
fn lights_are_off_after_a_power_cycle() {
    let mut s = riding();
    pick(&mut s, LightsMode::On);
    s.phone_send_time(hms(21, 0, 0), SUNSET, SUNRISE);
    assert_lights(&mut s, true);
    s.hold(Buttons::PWR, 1200);
    assert!(s.hal().powered_off);
    let mut s = s.reboot();
    s.boot_to_ride();
    assert_eq!(s.app().lights().mode, LightsMode::Off);
    assert_eq!(s.app().lights().clock.secs_of_day(s.now_ms()), None);
    assert_lights(&mut s, false);
}

#[test]
fn lights_do_not_touch_pas_or_the_store() {
    let mut s = riding();
    s.click(Buttons::RIGHT);
    s.click(Buttons::RIGHT);
    s.run_ms(config::SAVE_DEBOUNCE_MS + 500);
    let stored = s.hal().store;
    let saves = s.app().saves();
    pick(&mut s, LightsMode::On);
    s.run_ms(config::SAVE_DEBOUNCE_MS + 500);
    assert_eq!(s.app().state().pas, 2);
    assert_eq!(s.app().saves(), saves, "the mode is RAM only");
    assert_eq!(s.hal().store, stored);
}
