//! M5: Lights page, Player and Gate pages, BLE telemetry/trips/commands,
//! DFU from the phone.

use swet_heart::blep::{self, Command, TELEMETRY_LEN, TRIP_LEN};
use swet_heart::ui::Screen;
use swet_heart::ui::menu::Item;
use swet_heart::ui::ride::Page;
use swet_heart::{BleChannel, BleState, Buttons, config};
use swet_sim::Sim;

fn riding(kmh: u32) -> Sim {
    let mut s = Sim::new();
    s.motor().set_speed_kmh(kmh);
    s.motor().current_x2 = 24; // 12 A → 624 W
    s.boot_to_ride();
    s
}

/// M short until `page` is showing (each click waits out the double-click
/// window and the 150 ms page slide).
fn goto(s: &mut Sim, page: Page) {
    for _ in 0..4 {
        if s.app().page() == page {
            return;
        }
        s.click(Buttons::M);
        s.run_ms(config::DBL_MS + 200);
    }
    panic!("page {page:?} not reached");
}

fn commands(s: &Sim) -> Vec<[u8; 2]> {
    s.notified(BleChannel::Command)
        .into_iter()
        .map(|d| [d[0], d[1]])
        .collect()
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

#[test]
fn m_cycles_pas_lights_player_gate_and_pwr_jumps_back_to_pas() {
    let mut s = riding(0);
    let mut seen = vec![s.app().page()];
    for _ in 0..4 {
        s.click(Buttons::M);
        s.run_ms(config::DBL_MS + 200);
        seen.push(s.app().page());
    }
    assert_eq!(
        seen,
        [Page::Pas, Page::Lights, Page::Player, Page::Gate, Page::Pas]
    );

    goto(&mut s, Page::Gate);
    s.click(Buttons::PWR);
    s.run_ms(config::DBL_MS + 200);
    assert_eq!(s.app().page(), Page::Pas);
}

#[test]
fn lights_page_switches_the_motor_light_and_dims_the_screen() {
    let mut s = riding(0);
    assert_eq!(s.hal().contrast, config::CONTRAST_DAY);
    goto(&mut s, Page::Lights);
    s.assert_screen("m5_lights_off");

    s.click(Buttons::RIGHT);
    s.run_ms(300); // next write slot
    assert!(s.app().state().lights);
    assert_eq!(s.hal().motor.lights, Some(true));
    assert_eq!(s.hal().contrast, config::CONTRAST_NIGHT);
    s.assert_screen("m5_lights_on");

    s.click(Buttons::LEFT);
    s.run_ms(300);
    assert!(!s.app().state().lights);
    assert_eq!(s.hal().motor.lights, Some(false));
    assert_eq!(s.hal().contrast, config::CONTRAST_DAY);
}

#[test]
fn lights_are_off_after_a_power_cycle() {
    let mut s = riding(0);
    goto(&mut s, Page::Lights);
    s.click(Buttons::RIGHT);
    s.hold(Buttons::PWR, 1200);
    assert!(s.hal().powered_off);
    let mut s = s.reboot();
    s.boot_to_ride();
    assert!(!s.app().state().lights);
    assert_eq!(s.hal().motor.lights, Some(false));
}

#[test]
fn lights_do_not_touch_pas() {
    let mut s = riding(0);
    s.click(Buttons::RIGHT);
    s.click(Buttons::RIGHT);
    goto(&mut s, Page::Lights);
    s.click(Buttons::LEFT);
    s.click(Buttons::RIGHT);
    assert_eq!(s.app().state().pas, 2);
}

#[test]
fn player_and_gate_without_a_phone_are_dithered_and_send_nothing() {
    let mut s = riding(0);
    goto(&mut s, Page::Player);
    s.assert_screen("m5_player_unavailable");
    s.click(Buttons::LEFT);
    s.click(Buttons::RIGHT);
    s.run_ms(500);
    goto(&mut s, Page::Gate);
    s.assert_screen("m5_gate_unavailable");
    s.click(Buttons::LEFT);
    s.click(Buttons::RIGHT);
    assert!(s.hal().ble_notifies.is_empty());
    assert_eq!(s.app().blep().commands, 0);
}

#[test]
fn player_sends_volume_tracks_and_play_pause() {
    let mut s = riding(0);
    s.phone_connect();
    goto(&mut s, Page::Player);
    s.assert_screen("m5_player");

    s.click(Buttons::LEFT); // volume −
    s.hold(Buttons::LEFT, 1200); // previous track
    s.click(Buttons::RIGHT); // volume +, after the double-click window
    s.run_ms(config::DBL_MS + 200);
    s.hold(Buttons::RIGHT, 1200); // next track
    s.double_click(Buttons::RIGHT); // play / pause
    s.run_ms(config::DBL_MS + 200);

    assert_eq!(
        commands(&s),
        [
            [1, Command::VolumeDown as u8],
            [2, Command::PrevTrack as u8],
            [3, Command::VolumeUp as u8],
            [4, Command::NextTrack as u8],
            [5, Command::PlayPause as u8],
        ]
    );
}

#[test]
fn volume_down_is_instant_volume_up_waits_for_the_double_click_window() {
    let mut s = riding(0);
    s.phone_connect();
    goto(&mut s, Page::Player);
    s.click(Buttons::LEFT);
    assert_eq!(commands(&s).len(), 1, "LEFT has no double-click: instant");

    s.click(Buttons::RIGHT);
    s.run_ms(200);
    assert_eq!(commands(&s).len(), 1, "RIGHT waits for a possible double");
    s.run_ms(200);
    assert_eq!(commands(&s)[1][1], Command::VolumeUp as u8);
}

#[test]
fn gate_a_and_b_are_instant() {
    let mut s = riding(0);
    s.phone_connect();
    goto(&mut s, Page::Gate);
    s.assert_screen("m5_gate");
    s.click(Buttons::LEFT);
    s.click(Buttons::RIGHT);
    assert_eq!(
        commands(&s),
        [[1, Command::GateA as u8], [2, Command::GateB as u8]]
    );
}

#[test]
fn commands_need_the_command_subscription() {
    let mut s = riding(0);
    s.hal_mut().ble = BleState(BleState::CONNECTED | BleState::TELEMETRY_SUB);
    goto(&mut s, Page::Gate);
    s.click(Buttons::LEFT);
    assert!(commands(&s).is_empty());
    s.assert_screen("m5_gate_unavailable");
}

#[test]
fn telemetry_once_a_second_while_connected() {
    let mut s = riding(27);
    s.click(Buttons::RIGHT);
    s.click(Buttons::RIGHT);
    s.click(Buttons::RIGHT);
    s.run_ms(2000);
    assert!(s.hal().ble_values[0].is_empty(), "nothing without a phone");

    s.phone_connect();
    s.tick();
    let tel = s.notified(BleChannel::Telemetry);
    assert_eq!(tel.len(), 1, "a new phone gets telemetry at once");
    let t = &tel[0];
    assert_eq!(t.len(), TELEMETRY_LEN);
    assert_eq!(t[0], blep::TELEMETRY_VERSION);
    assert!(
        (268..=272).contains(&u16_at(t, 1)),
        "speed {}",
        u16_at(t, 1)
    );
    assert_eq!(u16_at(t, 3), 624); // 12 A × 52 V
    assert_eq!(t[5], 78); // soc
    assert_eq!(t[6], 3); // pas
    assert_eq!(t[7], config::CITY_LIMIT_KMH);
    assert_eq!(t[8], 0b100); // lights off, no walk, link up
    assert_eq!(t[9], 0); // no error
    let odo_hm = u32_at(t, 10);
    assert_eq!(odo_hm, s.app().rides().odo_m / 100);

    s.run_ms(3000);
    assert_eq!(s.notified(BleChannel::Telemetry).len(), 4);

    s.phone_disconnect();
    s.run_ms(3000);
    assert_eq!(s.notified(BleChannel::Telemetry).len(), 4);
}

#[test]
fn telemetry_reports_errors_and_link_loss() {
    let mut s = riding(0);
    s.phone_connect();
    s.motor().status = 0x21;
    s.run_ms(1500);
    let last = |s: &Sim| {
        s.notified(BleChannel::Telemetry)
            .last()
            .cloned()
            .expect("telemetry")
    };
    assert_eq!(last(&s)[9], 0x21);

    s.motor().online = false;
    s.run_ms(config::MOTOR_LINK_TIMEOUT_MS + 1500);
    let t = last(&s);
    assert_eq!(t[9], blep::ERROR_LINK_LOST);
    assert_eq!(t[8] & 0b100, 0, "link down");
    assert_eq!(t[5], blep::SOC_UNKNOWN);
}

#[test]
fn trips_four_records_on_connect_then_every_five_seconds() {
    let mut s = riding(36);
    s.run_ms(10_000);
    s.motor().set_speed_kmh(0); // hold the distances still for the comparison
    s.run_ms(1000);
    s.phone_connect();
    s.run_ms(200);
    let trips = s.notified(BleChannel::Trips);
    let ids: Vec<u8> = trips.iter().map(|r| r[1]).collect();
    assert_eq!(ids, [0, 1, 2, 3]);
    for r in &trips {
        assert_eq!(r.len(), TRIP_LEN);
        assert_eq!(r[0], blep::TRIPS_VERSION);
    }
    let rides = s.app().rides();
    assert_eq!(u32_at(&trips[0], 2), rides.trip.m);
    assert!(rides.trip.m > 90);
    let odo = &trips[3];
    assert_eq!(u32_at(odo, 2), rides.odo_m);
    assert_eq!(u16_at(odo, 6), rides.odo_max_x10);
    assert_eq!(u16_at(odo, 8), 0, "no average for the odometer");

    s.run_ms(5000);
    assert_eq!(s.notified(BleChannel::Trips).len(), 8);
}

#[test]
fn trip_reset_sends_the_trip_record_at_once() {
    let mut s = riding(36);
    s.run_ms(10_000);
    s.motor().set_speed_kmh(0);
    s.phone_connect();
    s.run_ms(500);
    let before = s.notified(BleChannel::Trips).len();

    s.hold(Buttons::M, 1200);
    assert_eq!(s.app().menu_item(), Item::ResetTrip);
    s.click(Buttons::M); // open
    s.click(Buttons::M); // yes
    s.tick();
    let trips = s.notified(BleChannel::Trips);
    assert_eq!(trips.len(), before + 1);
    let r = trips.last().expect("a trip record");
    assert_eq!(r[1], 0, "the manual trip");
    assert_eq!(u32_at(r, 2), 0);
}

#[test]
fn dfu_from_the_phone_opens_the_update_screen_only_when_stopped_for_five_seconds() {
    let mut s = riding(20);
    s.phone_connect();
    s.phone_write_control(b"DFU!");
    s.run_ms(1000);
    assert_eq!(s.app().screen(), Screen::Ride, "refused while riding");

    s.motor().set_speed_kmh(0);
    s.run_ms(3000);
    s.phone_write_control(b"DFU!");
    s.run_ms(1000);
    assert_eq!(s.app().screen(), Screen::Ride, "stopped for less than 5 s");

    s.run_ms(2000);
    s.phone_write_control(b"dfu?");
    s.run_ms(100);
    assert_eq!(s.app().screen(), Screen::Ride, "anything else is ignored");

    s.phone_write_control(b"DFU!");
    s.tick();
    assert_eq!(
        s.app().screen(),
        Screen::Update,
        "the rider still has to hold PWR"
    );
    assert!(!s.hal().dfu_requested);
    let saves = s.app().saves();
    s.press(Buttons::PWR);
    s.run_ms(1000);
    assert!(s.hal().dfu_requested);
    assert!(s.app().saves() > saves, "saved before rebooting");
}

#[test]
fn walk_assist_ends_on_release_even_after_a_page_switch() {
    let mut s = riding(0);
    s.press(Buttons::LEFT);
    s.run_ms(1200);
    assert!(s.app().state().walk);
    s.click(Buttons::M);
    s.run_ms(config::DBL_MS + 200);
    assert_eq!(s.app().page(), Page::Lights);
    s.release(Buttons::LEFT);
    s.run_ms(100);
    assert!(!s.app().state().walk);
}

#[test]
fn ble_screen_shows_the_phone() {
    let mut s = riding(0);
    s.phone_connect();
    s.hold(Buttons::M, 1200);
    s.click(Buttons::RIGHT);
    assert_eq!(s.app().menu_item(), Item::Ble);
    s.click(Buttons::M);
    assert_eq!(s.app().screen(), Screen::Ble);
    s.assert_screen("m5_ble_connected");
}
