//! M3: persistence, lock + PIN, modes, power-off sequence, auto-off.
//!
//! Tests build with the dev PINs: city 1111, sport 2222 (swet-heart/build.rs).

use swet_heart::store::Record;
use swet_heart::ui::Screen;
use swet_heart::{Buttons, Power, config};
use swet_sim::Sim;

fn riding() -> Sim {
    let mut s = Sim::new();
    s.motor().set_speed_kmh(15);
    s.boot_to_ride();
    s
}

fn stored(s: &Sim) -> Record {
    let bytes = s.hal().store.expect("something was saved");
    Record::decode(&bytes).expect("current layout")
}

/// Lock with a PWR double-click and wait until the display is off.
fn lock(s: &mut Sim) {
    s.double_click(Buttons::PWR);
    s.run_ms(2000);
    assert!(s.hal().powered_off);
}

/// Enter a PIN on the picker: each digit from 0 with RIGHT, then M.
fn enter_pin(s: &mut Sim, pin: [u8; 4]) {
    for d in pin {
        for _ in 0..d {
            s.click(Buttons::RIGHT);
        }
        s.click(Buttons::M);
    }
}

#[test]
fn first_boot_uses_defaults() {
    let s = riding();
    let st = s.app().state();
    assert_eq!((st.pas, st.speed_limit, st.locked), (0, 25, false));
    assert_eq!(s.app().screen(), Screen::Ride);
}

#[test]
fn pas_is_saved_3_s_after_the_last_change_and_survives_a_power_cycle() {
    let mut s = riding();
    for _ in 0..4 {
        s.click(Buttons::RIGHT);
    }
    s.run_ms(config::SAVE_DEBOUNCE_MS - 500);
    assert_eq!(s.hal().store_writes, 0, "still inside the debounce window");
    s.run_ms(700);
    assert_eq!(s.hal().store_writes, 1, "four clicks, one write");
    assert_eq!(stored(&s).pas, 4);

    let mut s2 = s.reboot();
    s2.boot_to_ride();
    assert_eq!(s2.app().state().pas, 4);
    assert_eq!(
        s2.hal().motor.pas_code,
        Some(0x0D),
        "PAS 4 goes to the motor at link-up"
    );
}

#[test]
fn pwr_hold_saves_before_cutting_power() {
    let mut s = riding();
    s.click(Buttons::RIGHT);
    s.click(Buttons::RIGHT);
    // power off well inside the 3 s debounce: the change must still land
    s.press(Buttons::PWR);
    s.run_ms(1300);
    assert!(s.hal().powered_off);
    assert_eq!(stored(&s).pas, 2);
}

#[test]
fn lock_shows_padlock_saves_and_powers_off() {
    let mut s = riding();
    s.double_click(Buttons::PWR);
    s.run_ms(100);
    assert!(matches!(s.app().power(), Power::Locking { .. }));
    s.assert_screen("m3_padlock");
    s.run_ms(config::LOCK_FLASH_MS + 200);
    assert!(s.hal().powered_off);
    assert!(stored(&s).locked);
}

#[test]
fn locked_boot_asks_for_pin_and_gives_no_assist() {
    let mut s = riding();
    for _ in 0..5 {
        s.click(Buttons::RIGHT);
    }
    lock(&mut s);

    let mut s = s.reboot();
    s.boot_to_ride();
    assert_eq!(s.app().screen(), Screen::Pin);
    s.assert_screen("m3_pin_entry");
    assert_eq!(s.app().state().pas, 5, "level is remembered…");
    assert_eq!(
        s.hal().motor.pas_code,
        Some(0x00),
        "…but not sent while locked"
    );
    s.click(Buttons::RIGHT);
    s.click(Buttons::LEFT);
    s.hold(Buttons::LEFT, 1300); // walk assist gesture does nothing either
    assert_eq!(s.hal().motor.pas_code, Some(0x00));
}

#[test]
fn city_pin_unlocks_with_25_and_restores_pas() {
    let mut s = riding();
    for _ in 0..3 {
        s.click(Buttons::RIGHT);
    }
    lock(&mut s);
    let mut s = s.reboot();
    s.boot_to_ride();
    enter_pin(&mut s, config::PIN_CITY);
    s.run_ms(300);
    assert_eq!(s.app().screen(), Screen::Ride);
    let st = *s.app().state();
    assert_eq!((st.locked, st.speed_limit, st.pas), (false, 25, 3));
    assert_eq!(s.hal().motor.pas_code, Some(0x0C));
    assert_eq!(s.hal().motor.speed_limit_wire, Some(192));
    s.run_ms(100);
    assert!(!stored(&s).locked, "unlock is saved at once");
}

#[test]
fn sport_pin_unlocks_with_99_and_shows_the_bolt() {
    let mut s = riding();
    lock(&mut s);
    let mut s = s.reboot();
    s.motor().soc = 60;
    s.boot_to_ride();
    enter_pin(&mut s, config::PIN_SPORT);
    s.run_ms(300);
    assert_eq!(s.app().state().speed_limit, 99);
    assert!(s.app().state().is_sport());
    assert_eq!(
        s.hal().motor.speed_limit_wire,
        Some(762),
        "99 km/h as wheel rpm"
    );
    s.assert_screen("m3_sport_battery");

    // the mode survives a normal power cycle
    s.run_ms(500);
    let s2 = s.reboot();
    assert_eq!(s2.app().state().speed_limit, 99);
    assert_eq!(s2.app().screen(), Screen::Ride);
}

#[test]
fn wrong_pin_stays_locked_and_says_so() {
    let mut s = riding();
    lock(&mut s);
    let mut s = s.reboot();
    s.boot_to_ride();
    enter_pin(&mut s, [1, 2, 3, 4]);
    assert_eq!(s.app().screen(), Screen::Pin);
    assert!(s.app().state().locked);
    s.assert_screen("m3_pin_wrong");
    // and a correct PIN right after still works
    enter_pin(&mut s, config::PIN_CITY);
    assert_eq!(s.app().screen(), Screen::Ride);
}

#[test]
fn pwr_click_is_backspace_on_the_pin_screen() {
    let mut s = riding();
    lock(&mut s);
    let mut s = s.reboot();
    s.boot_to_ride();
    // type 1, 1, 9 by mistake, back up, fix the third digit to 1
    s.click(Buttons::RIGHT);
    s.click(Buttons::M); // 1
    s.click(Buttons::RIGHT);
    s.click(Buttons::M); // 1
    s.click(Buttons::LEFT);
    s.click(Buttons::M); // 9 — oops
    s.click(Buttons::PWR); // back to the third digit (still 9)
    s.click(Buttons::RIGHT); // 9 → 0
    s.click(Buttons::RIGHT); // 0 → 1
    s.click(Buttons::M);
    s.click(Buttons::RIGHT);
    s.click(Buttons::M); // fourth digit: 1
    assert_eq!(s.app().screen(), Screen::Ride);
}

#[test]
fn pwr_hold_on_the_pin_screen_powers_off() {
    let mut s = riding();
    lock(&mut s);
    let mut s = s.reboot();
    s.boot_to_ride();
    s.press(Buttons::PWR);
    s.run_ms(1300);
    assert!(s.hal().powered_off);
    assert!(stored(&s).locked, "still locked");
}

#[test]
fn auto_off_after_five_idle_minutes() {
    let mut s = Sim::new();
    s.boot_to_ride(); // wheel still, no current
    s.run_ms(config::AUTO_OFF_MS - 5_000);
    assert!(!s.hal().powered_off);
    s.run_ms(6_000);
    assert!(s.hal().powered_off);
}

#[test]
fn riding_or_a_button_keeps_it_on() {
    let mut s = riding(); // 15 km/h
    s.run_ms(config::AUTO_OFF_MS + 10_000);
    assert!(!s.hal().powered_off, "moving");

    s.motor().set_speed_kmh(0);
    s.run_ms(config::AUTO_OFF_MS - 10_000);
    s.click(Buttons::RIGHT);
    s.run_ms(config::AUTO_OFF_MS - 10_000);
    assert!(!s.hal().powered_off, "the click restarted the idle timer");
    s.run_ms(15_000);
    assert!(s.hal().powered_off);
}

#[test]
fn power_off_does_not_wait_forever_for_flash() {
    let mut s = riding();
    s.click(Buttons::RIGHT);
    s.press(Buttons::PWR);
    s.run_ms(1000 + config::SAVE_BEFORE_OFF_MS + 200);
    assert!(s.hal().powered_off);
}
