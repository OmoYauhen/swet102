//! M0: the test pattern runs on the virtual clock, reacts to buttons, pings
//! the motor and can power off.

use swet_heart::Buttons;
use swet_sim::Sim;

#[test]
fn boots_and_draws_the_test_pattern() {
    let mut s = Sim::new();
    s.run_ms(100);
    assert_eq!(s.hal().flushes, 5);
    let f = s.screen();
    assert!(f.get(0, 0) && f.get(127, 63), "border is drawn");
    s.assert_screen("m0_testpattern");
}

#[test]
fn m_click_cycles_display_orientation() {
    let mut s = Sim::new();
    s.tick();
    assert_eq!(s.hal().orient, 0);
    s.click(Buttons::M);
    assert_eq!(s.hal().orient, 1);
    s.click(Buttons::M);
    s.click(Buttons::M);
    s.click(Buttons::M);
    assert_eq!(s.hal().orient, 0);
}

#[test]
fn power_button_still_held_from_boot_is_ignored() {
    let mut s = Sim::new();
    s.press(Buttons::PWR); // the press that switched the display on
    s.run_ms(3000);
    assert!(!s.hal().powered_off);
}

#[test]
fn pwr_hold_powers_off() {
    let mut s = Sim::new();
    s.tick();
    s.press(Buttons::PWR);
    s.run_ms(900);
    assert!(!s.hal().powered_off);
    s.run_ms(200);
    assert!(s.hal().powered_off);
}

#[test]
fn motor_status_is_pinged_every_500_ms() {
    let mut s = Sim::new();
    s.tick(); // first ping goes out at t = 20 ms
    assert_eq!(s.hal().uart_log, [0x11, 0x08]);
    s.run_ms(480);
    assert_eq!(s.hal().uart_log.len(), 2, "next ping only after 500 ms");
    s.tick();
    assert_eq!(s.hal().uart_log.len(), 4);
}
