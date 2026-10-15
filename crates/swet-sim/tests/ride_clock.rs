//! Clock above the page tile (PRODUCT §3.6): HH:MM while a phone is
//! connected and has sent the time, nothing otherwise.

use swet_sim::Sim;

/// Lit pixels in the clock row: column 1 (the tile's 26 px), top 16 px.
fn clock_px(s: &Sim) -> usize {
    let f = s.screen();
    (0..16)
        .flat_map(|y| (0..26).map(move |x| (x, y)))
        .filter(|&(x, y)| f.get(x, y))
        .count()
}

fn riding() -> Sim {
    let mut s = Sim::new();
    s.boot_to_ride();
    s
}

#[test]
fn empty_without_a_phone() {
    let mut s = riding();
    s.run_ms(300);
    assert_eq!(clock_px(&s), 0);
    s.assert_screen("ride_clock_none");
}

#[test]
fn empty_while_connected_before_the_time_arrives() {
    let mut s = riding();
    s.phone_connect();
    s.run_ms(300);
    assert_eq!(clock_px(&s), 0);
}

#[test]
fn shows_the_phone_time_and_keeps_it_running() {
    let mut s = riding();
    s.phone_connect();
    s.phone_send_time(21 * 3600 + 47 * 60 + 30, 19 * 60, 7 * 60);
    s.run_ms(300);
    assert!(clock_px(&s) > 0);
    s.assert_screen("ride_clock_2147");
    // 30 s later the minute turns over
    s.run_ms(30_000);
    s.assert_screen("ride_clock_2148");
}

#[test]
fn wraps_at_midnight() {
    let mut s = riding();
    s.phone_connect();
    s.phone_send_time(23 * 3600 + 59 * 60 + 50, 19 * 60, 7 * 60);
    s.run_ms(15_000);
    s.assert_screen("ride_clock_0000");
}

#[test]
fn gone_when_the_phone_disconnects() {
    let mut s = riding();
    s.phone_connect();
    s.phone_send_time(8 * 3600 + 5 * 60, 19 * 60, 7 * 60);
    s.run_ms(300);
    assert!(clock_px(&s) > 0);
    s.phone_disconnect();
    s.run_ms(300);
    assert_eq!(clock_px(&s), 0);
    // back again on reconnect, without a new sync
    s.phone_connect();
    s.run_ms(300);
    assert!(clock_px(&s) > 0);
}
