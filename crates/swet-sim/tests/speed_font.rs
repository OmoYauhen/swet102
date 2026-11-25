//! Menu → Speed font: pick the speed's font, saved across power-off.

use swet_heart::ui::Screen;
use swet_heart::ui::menu::Item;
use swet_heart::ui::ride::SpeedFont;
use swet_heart::{Buttons, config};
use swet_sim::Sim;

fn riding(kmh: u32) -> Sim {
    let mut s = Sim::new();
    s.motor().set_speed_kmh(kmh);
    s.boot_to_ride();
    s
}

/// Menu → Speed font, from the ride screen.
fn open_fonts(s: &mut Sim) {
    s.hold(Buttons::M, 1200);
    while s.app().menu_item() != Item::SpeedFont {
        s.click(Buttons::RIGHT);
    }
    s.click(Buttons::M);
    assert_eq!(s.app().screen(), Screen::SpeedFont);
}

#[test]
fn every_font_previews_and_shows_on_the_ride_screen() {
    let mut s = riding(27);
    assert_eq!(s.app().state().speed_font, SpeedFont::Karma, "the default");
    s.assert_screen("speed_font_karma");
    for sf in SpeedFont::ALL.iter().skip(1) {
        open_fonts(&mut s); // opens on the font in use…
        s.click(Buttons::RIGHT); // …so one step is the next one
        let name = format!("{sf:?}").to_lowercase();
        s.assert_screen(&format!("speed_font_choice_{name}"));
        s.click(Buttons::M); // set: back to the menu
        assert_eq!(s.app().screen(), Screen::Menu);
        assert_eq!(s.app().state().speed_font, *sf);
        s.click(Buttons::PWR);
        s.assert_screen(&format!("speed_font_{name}"));
    }
}

#[test]
fn pwr_leaves_the_font_as_it_was_and_the_choice_wraps() {
    let mut s = riding(27);
    open_fonts(&mut s);
    s.click(Buttons::LEFT); // wraps to the last
    s.click(Buttons::PWR);
    assert_eq!(s.app().screen(), Screen::Menu);
    assert_eq!(s.app().state().speed_font, SpeedFont::Karma);
}

#[test]
fn the_font_survives_a_power_cycle() {
    let mut s = riding(0);
    open_fonts(&mut s);
    s.click(Buttons::RIGHT);
    s.click(Buttons::RIGHT);
    s.click(Buttons::M);
    s.run_ms(config::SAVE_DEBOUNCE_MS + 500);
    let s2 = s.reboot();
    assert_eq!(s2.app().state().speed_font, SpeedFont::NexaRust);
}
