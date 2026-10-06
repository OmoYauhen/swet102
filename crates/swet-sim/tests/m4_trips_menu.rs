//! M4: trips with max/avg/Ah, odometer, battery-trip popup, the menu.

use swet_heart::store::Record;
use swet_heart::ui::Screen;
use swet_heart::ui::menu::Item;
use swet_heart::ui::popup::{Fault, Popup};
use swet_heart::ui::ride::View;
use swet_heart::{Buttons, config};
use swet_sim::Sim;

fn stored(s: &Sim) -> Record {
    Record::decode(&s.hal().store.expect("something was saved")).expect("current layout")
}

/// Ride at `kmh` drawing `amps` for `secs`, then return the sim.
fn ride(kmh: u32, amps: u8, secs: u32) -> Sim {
    let mut s = Sim::new();
    s.boot_to_ride();
    s.motor().set_speed_kmh(kmh);
    s.motor().current_x2 = amps * 2;
    s.run_ms(secs * 1000);
    s
}

fn show_view(s: &mut Sim, v: View) {
    for _ in 0..6 {
        if s.app().view() == v {
            return;
        }
        s.double_click(Buttons::M);
        s.run_ms(250); // let the pane push finish
    }
    panic!("view {v:?} not reached");
}

fn open_menu_at(s: &mut Sim, item: Item) {
    s.hold(Buttons::M, 1200);
    assert_eq!(s.app().screen(), Screen::Menu);
    for _ in 0..5 {
        if s.app().menu_item() == item {
            return;
        }
        s.click(Buttons::RIGHT);
    }
    panic!("menu item {item:?} not reached");
}

#[test]
fn trips_count_distance_moving_time_max_and_charge() {
    // 36 km/h ≈ 10 m/s at 10 A for 60 s
    let mut s = ride(36, 10, 60);
    let r = s.app().rides();
    for t in [r.trip, r.batt, r.ride] {
        assert!((595..=601).contains(&t.m), "{} m", t.m);
        assert!((355..=360).contains(&t.avg_x10()), "avg {}", t.avg_x10());
        assert!((355..=360).contains(&t.max_x10), "max {}", t.max_x10);
        assert!((164..=167).contains(&t.mah), "{} mAh", t.mah); // 10 A × 60 s = 166.7
    }
    assert_eq!(r.odo_m, r.trip.m);

    // a stop doesn't drag the average down
    s.motor().set_speed_kmh(0);
    s.motor().current_x2 = 0;
    s.run_ms(60_000);
    assert!((355..=360).contains(&s.app().rides().trip.avg_x10()));

    show_view(&mut s, View::Trip);
    s.assert_screen("m4_trip");
    show_view(&mut s, View::Odo);
    s.assert_screen("m4_odo");
}

#[test]
fn distances_are_saved_after_a_stop_and_survive_a_power_cycle() {
    let mut s = ride(36, 0, 30);
    s.motor().set_speed_kmh(0);
    s.run_ms(config::SAVE_STOPPED_MS + config::SAVE_DEBOUNCE_MS + 500);
    let saved = stored(&s);
    assert!((295..=301).contains(&saved.odo_m), "{} m", saved.odo_m);
    assert_eq!(saved.trip_m, saved.odo_m);

    let s2 = s.reboot();
    let r = s2.app().rides();
    assert_eq!((r.trip.m, r.odo_m), (saved.trip_m, saved.odo_m));
    assert_eq!(
        r.ride.m, 0,
        "the ride counter starts at 0 on every power-on"
    );
}

#[test]
fn reset_trip_from_the_menu_with_confirmation() {
    let mut s = ride(36, 0, 30);
    s.motor().set_speed_kmh(0);
    let odo = s.app().rides().odo_m;
    open_menu_at(&mut s, Item::ResetTrip);
    s.assert_screen("m4_menu_reset");
    s.click(Buttons::M);
    assert!(matches!(s.app().screen(), Screen::Confirm(_)));
    s.assert_screen("m4_confirm_reset");

    s.click(Buttons::PWR); // cancel
    assert_eq!(s.app().screen(), Screen::Menu);
    assert!(s.app().rides().trip.m > 0);

    s.click(Buttons::M);
    s.click(Buttons::M); // yes
    assert_eq!(s.app().screen(), Screen::Menu);
    let r = s.app().rides();
    assert_eq!(r.trip.m, 0);
    assert_eq!(
        (r.batt.m, r.odo_m),
        (odo, odo),
        "only the manual trip resets"
    );
    s.assert_screen("m4_menu_done");
    s.run_ms(200);
    assert_eq!(stored(&s).trip_m, 0, "saved at once");
}

#[test]
fn menu_wraps_and_pwr_backs_out() {
    let mut s = ride(0, 0, 1);
    s.hold(Buttons::M, 1200);
    assert_eq!(
        s.app().menu_item(),
        Item::ResetTrip,
        "opens at the first item"
    );
    s.click(Buttons::LEFT);
    assert_eq!(s.app().menu_item(), Item::Dfu);
    s.click(Buttons::RIGHT);
    s.click(Buttons::RIGHT);
    assert_eq!(s.app().menu_item(), Item::Ble);
    s.click(Buttons::PWR);
    assert_eq!(s.app().screen(), Screen::Ride);
    s.hold(Buttons::M, 1200);
    assert_eq!(
        s.app().menu_item(),
        Item::ResetTrip,
        "position is not remembered"
    );
}

#[test]
fn detail_screens_open_and_return_to_the_menu() {
    let mut s = ride(0, 0, 1);
    for (item, screen) in [
        (Item::Ble, Screen::Ble),
        (Item::Diagnostics, Screen::Diag),
        (Item::Firmware, Screen::Firmware),
    ] {
        open_menu_at(&mut s, item);
        s.click(Buttons::M);
        assert_eq!(s.app().screen(), screen);
        if screen == Screen::Ble {
            s.assert_screen("m4_ble");
        }
        s.click(Buttons::PWR);
        assert_eq!(s.app().screen(), Screen::Menu);
        s.click(Buttons::PWR);
        assert_eq!(s.app().screen(), Screen::Ride);
    }
}

#[test]
fn reboot_to_dfu_saves_first_and_needs_confirmation() {
    let mut s = ride(0, 0, 1);
    s.click(Buttons::RIGHT); // an unsaved PAS change
    open_menu_at(&mut s, Item::Dfu);
    s.click(Buttons::M);
    s.assert_screen("m4_confirm_dfu");
    assert!(!s.hal().dfu_requested);
    s.click(Buttons::M);
    s.run_ms(200);
    assert!(s.hal().dfu_requested);
    assert_eq!(stored(&s).pas, 1, "saved before rebooting");
}

/// Feed one SoC value long enough to count as stable (3 readings, 600 ms apart).
fn soc(s: &mut Sim, v: u8) {
    s.motor().soc = v;
    s.run_ms(2500);
}

#[test]
fn battery_trip_popup_after_a_charge() {
    let mut s = ride(36, 0, 1);
    soc(&mut s, 60); // first boot: soc_min set silently
    assert_eq!(s.app().popup(), Popup::None);
    s.run_ms(40_000); // ~400 m
    soc(&mut s, 40);
    s.motor().set_speed_kmh(0);
    s.run_ms(500); // let a 0 rpm reading arrive before taking the snapshot
    let km_x10 = (s.app().rides().batt.m / 100) as u16;
    let trip = s.app().rides().trip.m;
    soc(&mut s, 95); // charged
    assert_eq!(s.app().popup(), Popup::BatteryTrip(km_x10));
    s.assert_screen("m4_battery_trip");
    assert_eq!(s.app().rides().batt.m, 0);
    assert_eq!(s.app().rides().trip.m, trip, "the manual trip is untouched");
    s.run_ms(200);
    assert_eq!(stored(&s).soc_min, 95, "saved at once");

    s.click(Buttons::RIGHT); // any button dismisses…
    assert_eq!(s.app().popup(), Popup::None);
    assert_eq!(s.app().state().pas, 0, "…and does nothing else");
}

#[test]
fn a_motor_error_beats_the_battery_trip_message() {
    let mut s = ride(36, 0, 1);
    soc(&mut s, 50);
    soc(&mut s, 70);
    assert!(matches!(s.app().popup(), Popup::BatteryTrip(_)));
    s.motor().status = 0x21;
    s.run_ms(700);
    assert_eq!(s.app().popup(), Popup::Fault(Fault::Code(0x21)));
    s.click(Buttons::M); // acknowledge the error…
    assert!(
        matches!(s.app().popup(), Popup::BatteryTrip(_)),
        "…the message is still there"
    );
}
