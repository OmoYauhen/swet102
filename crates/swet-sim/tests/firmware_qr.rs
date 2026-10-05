//! The Firmware screen's QR code must scan to the repo (decoded with rqrr from
//! the rendered frame, not just compared to a bitmap).

use swet_heart::Buttons;
use swet_heart::ui::Screen;
use swet_heart::ui::menu::Item;
use swet_sim::Sim;

#[test]
fn firmware_screen_qr_scans_to_the_repo() {
    let mut s = Sim::new();
    s.boot_to_ride();
    s.hold(Buttons::M, 1200);
    while s.app().menu_item() != Item::Firmware {
        s.click(Buttons::RIGHT);
    }
    s.click(Buttons::M);
    assert_eq!(s.app().screen(), Screen::Firmware);

    // 4× up, lit = white: what a phone camera sees, minus the blur.
    let frame = s.screen();
    let scale = 4;
    let (w, h) = (128 * scale, 64 * scale);
    let mut img = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| {
        if frame.get((x / scale) as i32, (y / scale) as i32) {
            255
        } else {
            0
        }
    });
    let grids = img.detect_grids();
    assert_eq!(grids.len(), 1, "exactly one QR code on screen");
    let (_meta, content) = grids[0].decode().expect("the QR code decodes");
    assert_eq!(content, "HTTPS://GITHUB.COM/OMOYAUHEN/SWET102");
}
