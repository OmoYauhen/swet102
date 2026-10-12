//! Menu → Special thanks (PRODUCT §5): the last item; LEFT/RIGHT step through
//! the names (wrapping) with the boot animation's slides, PWR goes back.

use swet_heart::Buttons;
use swet_heart::ui::Screen;
use swet_heart::ui::boot::{SLIDE_IN_MS, SLIDE_OUT_MS};
use swet_heart::ui::menu::Item;
use swet_sim::Sim;

/// Long enough for any step to finish.
const SETTLE_MS: u32 = SLIDE_OUT_MS + SLIDE_IN_MS + 200;

/// Riding, then M hold → LEFT (the ring wraps to the last item).
fn on_the_item() -> Sim {
    let mut s = Sim::new();
    s.boot_to_ride();
    s.hold(Buttons::M, 1200);
    s.click(Buttons::LEFT);
    assert_eq!(s.app().menu_item(), Item::Thanks, "last in the ring");
    s
}

/// … → M: the screen with the first name, slid in.
fn opened() -> Sim {
    let mut s = on_the_item();
    s.click(Buttons::M);
    assert_eq!(s.app().screen(), Screen::Thanks);
    s.run_ms(SETTLE_MS);
    s
}

#[test]
fn the_last_menu_item_has_a_heart() {
    let s = on_the_item();
    s.assert_screen("thanks_menu_item");
}

#[test]
fn opens_on_the_first_name_sliding_in_like_the_boot_text() {
    let mut s = on_the_item();
    s.click(Buttons::M);
    assert_eq!(s.app().screen(), Screen::Thanks);
    assert_eq!(s.app().thanks_index(), 0);
    assert!(s.app().animating(), "sliding in");
    s.run_ms(SLIDE_IN_MS / 4);
    s.assert_screen("thanks_opening"); // coming in from the right
    s.run_ms(SLIDE_IN_MS);
    assert!(!s.app().animating());
    s.assert_screen("thanks_1");
}

#[test]
fn right_steps_through_every_name_and_wraps() {
    let mut s = opened();
    s.click(Buttons::RIGHT);
    assert_eq!(s.app().thanks_index(), 1);
    s.run_ms(SLIDE_OUT_MS / 2);
    s.assert_screen("thanks_step_out"); // @entropia_pl leaving to the left
    s.run_ms(SLIDE_OUT_MS / 2 + SLIDE_IN_MS / 4);
    s.assert_screen("thanks_step_in"); // @yahor_k coming in from the right
    s.run_ms(SETTLE_MS);
    s.assert_screen("thanks_2");

    s.click(Buttons::RIGHT);
    s.run_ms(SETTLE_MS);
    assert_eq!(s.app().thanks_index(), 2);
    s.assert_screen("thanks_3");

    s.click(Buttons::RIGHT);
    s.run_ms(SETTLE_MS);
    assert_eq!(s.app().thanks_index(), 0, "wraps to the first");
    s.assert_screen("thanks_1");
}

#[test]
fn left_goes_back_and_wraps_to_the_last_name() {
    let mut s = opened();
    s.click(Buttons::LEFT);
    assert_eq!(s.app().thanks_index(), 2, "wraps to the last");
    s.run_ms(SETTLE_MS);
    s.assert_screen("thanks_3");
    s.click(Buttons::LEFT);
    s.run_ms(SETTLE_MS);
    assert_eq!(s.app().thanks_index(), 1);
    s.assert_screen("thanks_2");
}

#[test]
fn presses_during_a_slide_snap_it_and_never_queue() {
    let mut s = opened();
    s.click(Buttons::RIGHT);
    s.click(Buttons::RIGHT); // while the first step is still sliding
    assert_eq!(s.app().thanks_index(), 2);
    s.run_ms(SETTLE_MS);
    assert!(!s.app().animating());
    s.assert_screen("thanks_3");
}

#[test]
fn pwr_goes_back_to_the_menu_item_and_reopening_starts_over() {
    let mut s = opened();
    s.click(Buttons::RIGHT);
    s.run_ms(SETTLE_MS);
    s.click(Buttons::PWR);
    assert_eq!(s.app().screen(), Screen::Menu);
    assert_eq!(s.app().menu_item(), Item::Thanks);
    s.click(Buttons::M);
    s.run_ms(SETTLE_MS);
    assert_eq!(s.app().thanks_index(), 0, "the first name again");
    s.assert_screen("thanks_1");
    s.click(Buttons::M); // M does nothing here
    assert_eq!(s.app().screen(), Screen::Thanks);
    s.click(Buttons::PWR);
    s.click(Buttons::PWR);
    assert_eq!(s.app().screen(), Screen::Ride);
}
