//! M6: boot animation, page slide, PAS roll, info-pane push; input never
//! waits behind an animation.

use swet_heart::store::Record;
use swet_heart::ui::boot;
use swet_heart::ui::popup::{Fault, Popup};
use swet_heart::ui::ride::{Page, View};
use swet_heart::ui::{Screen, ride};
use swet_heart::{Buttons, STORE_LEN, config};
use swet_sim::Sim;

fn riding(kmh: u32) -> Sim {
    let mut s = Sim::new();
    s.motor().set_speed_kmh(kmh);
    s.boot_to_ride();
    s
}

/// Tick until `cond` holds (at most 2 s).
fn until(s: &mut Sim, cond: impl Fn(&Sim) -> bool) {
    for _ in 0..100 {
        if cond(s) {
            return;
        }
        s.tick();
    }
    panic!("condition not reached");
}

#[test]
fn boot_shows_sparkles_then_scrolls_the_version_then_rides() {
    let mut s = Sim::booting(None);
    s.run_ms(300);
    assert_eq!(s.app().screen(), Screen::Boot);
    s.assert_screen("m6_boot_sparkles");

    s.run_ms(boot::SPARKLES_MS + 500 - 300);
    s.assert_screen("m6_boot_scroll");

    let total = boot::duration_ms();
    assert!((1500..=3000).contains(&total), "boot takes {total} ms");
    s.run_ms(total - s.now_ms());
    assert_eq!(s.app().screen(), Screen::Ride);
}

#[test]
fn any_button_skips_the_boot_and_does_nothing_else() {
    let mut s = Sim::booting(None);
    s.run_ms(200);
    s.click(Buttons::M);
    assert_eq!(s.app().screen(), Screen::Ride);
    s.run_ms(config::DBL_MS + 200);
    assert_eq!(
        s.app().page(),
        Page::Pas,
        "the skipping click didn't switch the page"
    );

    let mut s = Sim::booting(None);
    s.run_ms(200);
    s.click(Buttons::RIGHT);
    assert_eq!(s.app().screen(), Screen::Ride);
    assert_eq!(s.app().state().pas, 0, "nor change PAS");
}

#[test]
fn a_locked_bike_boots_into_the_pin_screen() {
    let mut r = Record::defaults();
    r.locked = true;
    let buf: [u8; STORE_LEN] = r.encode();
    let mut s = Sim::booting(Some(buf));
    s.run_ms(500);
    assert_eq!(s.app().screen(), Screen::Boot);
    s.run_ms(boot::duration_ms());
    assert_eq!(s.app().screen(), Screen::Pin);
}

#[test]
fn the_link_lost_screen_waits_for_the_boot_to_end() {
    let mut s = Sim::booting(None);
    s.motor().online = false;
    s.run_ms(config::MOTOR_LINK_TIMEOUT_MS + 100);
    assert_eq!(s.app().screen(), Screen::Boot);
    assert_eq!(s.app().popup(), Popup::None);
    s.run_ms(boot::duration_ms());
    assert_eq!(s.app().popup(), Popup::Fault(Fault::LinkLost));
}

#[test]
fn page_switch_slides_inside_the_tile() {
    let mut s = riding(27);
    s.click(Buttons::M);
    until(&mut s, |s| s.app().page() == Page::Lights);
    assert!(s.app().animating());
    s.run_ms(60);
    s.assert_screen("m6_page_slide");
    s.run_ms(u32::from(ride::PAGE_SLIDE_MS));
    assert!(!s.app().animating());
}

#[test]
fn pas_change_rolls_the_digit() {
    let mut s = riding(27);
    s.press(Buttons::RIGHT);
    s.run_ms(60);
    s.release(Buttons::RIGHT);
    until(&mut s, |s| s.app().state().pas == 1);
    assert!(s.app().animating());
    s.run_ms(40);
    s.assert_screen("m6_pas_roll");
    s.run_ms(u32::from(ride::PAS_ROLL_MS));
    assert!(!s.app().animating());
}

#[test]
fn pane_push_slides_live_views() {
    let mut s = riding(27);
    s.click(Buttons::M);
    s.run_ms(100);
    s.press(Buttons::M); // the second press is the double-click
    until(&mut s, |s| s.app().view() == View::Power);
    s.run_ms(40);
    s.assert_screen("m6_pane_push"); // speed leaving left, power coming in
    s.release(Buttons::M);
    s.run_ms(u32::from(ride::PANE_PUSH_MS));
    assert!(!s.app().animating());
}

#[test]
fn a_press_snaps_a_running_animation_to_its_end() {
    let mut s = riding(27);
    s.double_click(Buttons::M);
    until(&mut s, |s| s.app().view() == View::Power);
    assert!(s.app().animating());
    s.press(Buttons::RIGHT);
    s.run_ms(60); // debounced: the press arrives
    assert!(
        !s.app().animating(),
        "snapped on the press, not after the push"
    );
    s.release(Buttons::RIGHT);
    s.run_ms(60);
    assert_eq!(s.app().state().pas, 1, "and the press was handled");
}

#[test]
fn fast_pas_taps_do_not_queue_behind_the_roll() {
    let mut s = riding(0);
    for _ in 0..5 {
        s.click(Buttons::RIGHT);
    }
    assert_eq!(s.app().state().pas, 5);
    s.run_ms(u32::from(ride::PAS_ROLL_MS));
    assert!(!s.app().animating());
}
