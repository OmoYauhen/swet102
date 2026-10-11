//! M6: boot animation, page, PAS and info-pane slides; input never
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
fn boot_shows_sparkles_then_name_and_version_then_rides() {
    let mut s = Sim::booting(None);
    s.run_ms(300);
    assert_eq!(s.app().screen(), Screen::Boot);
    s.assert_screen("m6_boot_sparkles");

    s.run_ms(boot::SPARKLES_MS + 100 - 300);
    s.assert_screen("m6_boot_slide_in"); // both rows coming in from the right
    s.run_ms(boot::SLIDE_IN_MS + 300 - 100);
    s.assert_screen("m6_boot_rows"); // still, readable
    let before = s.screen();
    s.run_ms(boot::HOLD_MS / 2);
    assert!(s.screen() == before, "the rows hold still");

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
fn page_switch_slides_sideways_inside_the_tile() {
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
fn pas_change_slides_the_number() {
    let mut s = riding(27);
    s.press(Buttons::RIGHT);
    s.run_ms(60);
    s.release(Buttons::RIGHT);
    until(&mut s, |s| s.app().state().pas == 1);
    assert!(s.app().animating());
    s.run_ms(40);
    s.assert_screen("m6_pas_slide");
    s.run_ms(u32::from(ride::PAS_SLIDE_MS));
    assert!(!s.app().animating());
}

#[test]
fn pane_slides_up_with_live_views() {
    let mut s = riding(27);
    s.click(Buttons::M);
    s.run_ms(100);
    s.press(Buttons::M); // the second press is the double-click
    until(&mut s, |s| s.app().view() == View::Power);
    s.run_ms(40);
    s.assert_screen("m6_pane_slide"); // speed leaving at the top, power coming up
    s.release(Buttons::M);
    s.run_ms(u32::from(ride::PANE_SLIDE_MS));
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
        "snapped on the press, not after the slide"
    );
    s.release(Buttons::RIGHT);
    s.run_ms(60);
    assert_eq!(s.app().state().pas, 1, "and the press was handled");
}

#[test]
fn fast_pas_taps_do_not_queue_behind_the_slide() {
    let mut s = riding(0);
    for _ in 0..5 {
        s.click(Buttons::RIGHT);
    }
    assert_eq!(s.app().state().pas, 5);
    s.run_ms(u32::from(ride::PAS_SLIDE_MS));
    assert!(!s.app().animating());
}

#[test]
fn update_screens_ask_for_pwr_then_stay_on_the_display() {
    let mut s = riding(0);
    s.hold(Buttons::M, 1200); // menu
    for _ in 0..4 {
        if s.app().menu_item() == swet_heart::ui::menu::Item::Dfu {
            break;
        }
        s.click(Buttons::LEFT);
    }
    s.click(Buttons::M);
    s.assert_screen("m6_update_arm"); // press and hold PWR
    s.press(Buttons::PWR);
    until(&mut s, |s| s.app().power() != swet_heart::Power::On);
    assert!(!s.hal().dfu_requested, "not before the screen is up");
    s.run_ms(config::SAVE_BEFORE_OFF_MS + 100);
    assert!(s.hal().dfu_requested);
    s.assert_screen("m6_update_hold"); // the last frame sent: what the OLED keeps
}
