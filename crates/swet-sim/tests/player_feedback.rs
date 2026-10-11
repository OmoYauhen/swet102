//! Player page: the tile shows what was just sent for `ACTION_MS`, then the
//! note again.

use swet_heart::ui::ride::{ACTION_MS, Page};
use swet_heart::{Buttons, config};
use swet_sim::Sim;

fn on_player(phone: bool) -> Sim {
    let mut s = Sim::new();
    s.boot_to_ride();
    if phone {
        s.phone_connect();
    }
    while s.app().page() != Page::Player {
        s.click(Buttons::M);
        s.run_ms(config::DBL_MS + 200);
    }
    s
}

/// Do `gesture`, check the icon, then that the note is back after `ACTION_MS`.
fn shows(golden: &str, gesture: impl FnOnce(&mut Sim)) {
    let mut s = on_player(true);
    let sent = s.app().blep().commands;
    gesture(&mut s);
    assert_eq!(s.app().blep().commands, sent + 1, "{golden}: one command");
    s.assert_screen(golden);
    s.run_ms(ACTION_MS);
    s.assert_screen("m5_player"); // the note again
}

#[test]
fn volume_down_shows_a_speaker_with_minus() {
    shows("player_volume_down", |s| s.click(Buttons::LEFT));
}

#[test]
fn volume_up_shows_a_speaker_with_plus() {
    shows("player_volume_up", |s| {
        s.click(Buttons::RIGHT);
        s.run_ms(config::DBL_MS + 40); // RIGHT waits for a possible double-click
    });
}

#[test]
fn previous_track_shows_skip_back() {
    shows("player_prev", |s| s.hold(Buttons::LEFT, 1200));
}

#[test]
fn next_track_shows_skip_forward() {
    shows("player_next", |s| s.hold(Buttons::RIGHT, 1200));
}

#[test]
fn play_pause_shows_play_pause() {
    shows("player_play_pause", |s| s.double_click(Buttons::RIGHT));
}

#[test]
fn nothing_sent_nothing_shown() {
    let mut s = on_player(false);
    s.click(Buttons::LEFT);
    s.assert_screen("m5_player_unavailable");
}

#[test]
fn leaving_the_page_ends_the_icon() {
    let mut s = on_player(true);
    s.click(Buttons::LEFT);
    // round the ring and back within the second
    for _ in 0..4 {
        s.press(Buttons::M);
        s.run_ms(60);
        s.release(Buttons::M);
        s.run_ms(config::DBL_MS + 60);
    }
    assert_eq!(s.app().page(), Page::Player);
    s.run_ms(200);
    s.assert_screen("m5_player");
}
