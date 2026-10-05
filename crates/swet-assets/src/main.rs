//! Asset generator: XBM sources in `assets/` → `crates/swet-heart/src/gfx/assets.rs`.
//!
//!     cargo run -p swet-assets            # regenerate (output is committed)
//!     cargo run -p swet-assets -- preview # also write target/assets-preview/*.png
//!
//! Swang Stodva's assets (`assets/ss/`) are stored transposed (its blitter wanted
//! columns) with 0 = lit. Ours are upright, row-major, LSB = leftmost, 1 = lit:
//! the same layout as `swet_heart::Frame`, so blits are plain byte rows.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// An upright 1-bpp bitmap, 1 = lit.
struct Bitmap {
    w: usize,
    h: usize,
    px: Vec<bool>,
}

impl Bitmap {
    fn get(&self, x: usize, y: usize) -> bool {
        self.px[y * self.w + x]
    }

    /// Row-major, LSB-first byte rows.
    fn pack(&self) -> (usize, Vec<u8>) {
        let stride = self.w.div_ceil(8);
        let mut out = vec![0u8; stride * self.h];
        for y in 0..self.h {
            for x in 0..self.w {
                if self.get(x, y) {
                    out[y * stride + x / 8] |= 1 << (x % 8);
                }
            }
        }
        (stride, out)
    }
}

/// Parse an XBM file as-is (row-major, LSB = leftmost, 1 = set bit).
fn read_xbm(path: &Path) -> (usize, usize, Vec<bool>) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let define = |suffix: &str| -> usize {
        text.lines()
            .find_map(|l| {
                let mut it = l.split_whitespace();
                (it.next() == Some("#define") && it.next()?.ends_with(suffix))
                    .then(|| it.next()?.parse().ok())
                    .flatten()
            })
            .unwrap_or_else(|| panic!("{}: no {suffix}", path.display()))
    };
    let (w, h) = (define("_width"), define("_height"));
    let body = &text[text.find('{').expect("xbm body")..];
    let bytes: Vec<u8> = body
        .split(|c: char| c == ',' || c.is_whitespace() || c == '{' || c == '}' || c == ';')
        .filter_map(|t| t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")))
        .map(|t| u8::from_str_radix(t, 16).expect("hex byte"))
        .collect();
    let stride = w.div_ceil(8);
    assert_eq!(bytes.len(), stride * h, "{}: byte count", path.display());
    let mut px = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            px[y * w + x] = bytes[y * stride + x / 8] & (1 << (x % 8)) != 0;
        }
    }
    (w, h, px)
}

/// Swang Stodva format: stored transposed (`mogrify -flip -rotate 90`, which is
/// a plain transpose) and inverted (0 = lit).
fn read_ss(path: &Path) -> Bitmap {
    let (fw, fh, fpx) = read_xbm(path);
    let (w, h) = (fh, fw);
    let mut px = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            px[y * w + x] = !fpx[x * fw + y];
        }
    }
    Bitmap { w, h, px }
}

struct FontSrc {
    name: &'static str,
    file: &'static str,
    chars: &'static str,
    /// Glyph start columns after the first (SS `DEFINE_FONT` offsets).
    offsets: &'static [u16],
}

const FONTS: &[FontSrc] = &[
    FontSrc {
        name: "SPEED",
        file: "ss/font_speed.xbm",
        chars: ".0123456789",
        offsets: &[7, 23, 34, 50, 66, 82, 98, 114, 130, 146],
    },
    FontSrc {
        name: "SMALL",
        file: "ss/font_2nd.xbm",
        chars: " ./0123456789Whkm",
        offsets: &[
            2, 3, 7, 16, 23, 31, 40, 49, 58, 67, 76, 85, 94, 103, 111, 118,
        ],
    },
    FontSrc {
        name: "BATTERY",
        file: "ss/font_battery.xbm",
        chars: "%.0123456789V",
        offsets: &[5, 7, 12, 17, 22, 27, 32, 37, 42, 47, 52, 57],
    },
    FontSrc {
        name: "TEXT",
        file: "ss/font_full.xbm",
        chars: " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~",
        offsets: &[
            5, 7, 11, 20, 26, 37, 46, 48, 52, 56, 62, 70, 72, 76, 78, 83, 90, 96, 103, 110, 117,
            124, 131, 138, 145, 152, 154, 156, 165, 174, 183, 189, 201, 210, 217, 224, 232, 239,
            245, 253, 261, 263, 267, 274, 280, 289, 297, 305, 312, 320, 328, 335, 343, 351, 360,
            372, 380, 388, 396, 399, 404, 407, 414, 421, 424, 431, 438, 444, 451, 458, 463, 470,
            477, 479, 482, 488, 490, 500, 507, 514, 521, 528, 533, 539, 544, 551, 558, 568, 575,
            582, 588, 594, 596, 602,
        ],
    },
];

/// A pixel font drawn as outlines (e.g. W95FA), rendered exactly on its own
/// pixel grid and then scaled up by an integer factor.
struct PixelFontSrc {
    name: &'static str,
    file: &'static str,
    chars: &'static str,
    /// Font units per design pixel, and the grid's x offset in font units.
    unit: f32,
    x_origin: f32,
    scale: usize,
}

const PIXEL_FONTS: &[PixelFontSrc] = &[PixelFontSrc {
    // W95FA (SIL OFL 1.1, assets/fonts/W95FA-OFL.txt): PAS number in the page
    // tile, "!" and the hex code on the error screen.
    name: "W95",
    file: "fonts/W95FA.otf",
    chars: "!0123456789ABCDEF",
    unit: 80.0,
    x_origin: 50.0,
    scale: 4,
}];

/// Rasterise each char on the font's pixel grid, crop each glyph to its ink
/// columns and all glyphs to their common ink rows, then scale up.
fn read_pixel_font(path: &Path, src: &PixelFontSrc) -> (Bitmap, Vec<u16>) {
    use ab_glyph::{Font as _, FontRef, PxScale, ScaleFont as _, point};
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let font = FontRef::try_from_slice(&data).expect("font");
    // ab_glyph's PxScale is the pixel height of ascent − descent (not the em),
    // so this makes one design pixel exactly one output pixel.
    let px = font.height_unscaled() / src.unit;
    let scaled = font.as_scaled(PxScale::from(px));
    const CANVAS: usize = 64;
    let baseline = 48.0;
    let mut glyphs: Vec<Vec<bool>> = Vec::new();
    for c in src.chars.chars() {
        let mut g = scaled.scaled_glyph(c);
        g.position = point(-src.x_origin / src.unit, baseline);
        let mut canvas = vec![false; CANVAS * CANVAS];
        if let Some(o) = font.outline_glyph(g) {
            let b = o.px_bounds();
            o.draw(|x, y, v| {
                let (cx, cy) = (b.min.x as i32 + x as i32, b.min.y as i32 + y as i32);
                if v >= 0.5 && (0..CANVAS as i32).contains(&cx) && (0..CANVAS as i32).contains(&cy)
                {
                    canvas[cy as usize * CANVAS + cx as usize] = true;
                }
            });
        }
        glyphs.push(canvas);
    }
    let rows: Vec<usize> = (0..CANVAS)
        .filter(|&y| {
            glyphs
                .iter()
                .any(|g| (0..CANVAS).any(|x| g[y * CANVAS + x]))
        })
        .collect();
    let (top, bottom) = (rows[0], rows[rows.len() - 1] + 1);
    let h = (bottom - top) * src.scale;
    let mut cols: Vec<(usize, usize, usize)> = Vec::new(); // (glyph, x0, x1)
    for (i, g) in glyphs.iter().enumerate() {
        let ink: Vec<usize> = (0..CANVAS)
            .filter(|&x| (top..bottom).any(|y| g[y * CANVAS + x]))
            .collect();
        cols.push((i, ink[0], ink[ink.len() - 1] + 1));
    }
    let w: usize = cols.iter().map(|&(_, a, b)| (b - a) * src.scale).sum();
    let mut px_out = vec![false; w * h];
    let mut offsets = vec![0u16];
    let mut ox = 0;
    for &(i, x0, x1) in &cols {
        for y in 0..h {
            for x in 0..(x1 - x0) * src.scale {
                let (sx, sy) = (x0 + x / src.scale, top + y / src.scale);
                px_out[y * w + ox + x] = glyphs[i][sy * CANVAS + sx];
            }
        }
        ox += (x1 - x0) * src.scale;
        offsets.push(ox as u16);
    }
    (Bitmap { w, h, px: px_out }, offsets)
}

/// The repo URL for the Firmware screen's QR code. Uppercase so the QR uses
/// alphanumeric mode and fits version 2 (25×25); scheme, host and GitHub paths
/// are case-insensitive.
const REPO_QR: &str = "HTTPS://GITHUB.COM/OMOYAUHEN/SWET102";

/// QR code as a bitmap, 1 = dark module, no quiet zone.
fn qr_bitmap(data: &str) -> Bitmap {
    use qrcode::{EcLevel, QrCode};
    let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M).expect("QR fits");
    let w = code.width();
    assert_eq!(
        w, 25,
        "{data:?} must fit QR version 2 to stay 50 px at 2 px/module"
    );
    let px = code
        .to_colors()
        .iter()
        .map(|c| *c == qrcode::Color::Dark)
        .collect();
    Bitmap { w, h: w, px }
}

const IMAGES: &[(&str, &str)] = &[("SPARKLES", "ss/sparkles.xbm")];

fn bytes_lit(out: &mut String, bits: &[u8]) {
    for (i, chunk) in bits.chunks(16).enumerate() {
        out.push_str(if i == 0 { "\n        " } else { "        " });
        let row: Vec<String> = chunk.iter().map(|b| format!("0x{b:02X},")).collect();
        out.push_str(&row.join(" "));
        out.push('\n');
    }
    out.push_str("    ");
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let assets = root.join("assets");
    let preview = std::env::args().any(|a| a == "preview");
    let preview_dir = root.join("target/assets-preview");
    if preview {
        std::fs::create_dir_all(&preview_dir).expect("preview dir");
    }

    let mut out = String::from(
        "// @generated by `cargo run -p swet-assets` from assets/. Do not edit.\n\
         //! Fonts and images: upright, row-major, LSB = leftmost, 1 = lit.\n\n\
         use super::{Font, Image};\n",
    );

    for f in FONTS {
        let bmp = read_ss(&assets.join(f.file));
        assert_eq!(
            f.offsets.len() + 1,
            f.chars.len(),
            "{}: offsets vs chars",
            f.name
        );
        let (stride, bits) = bmp.pack();
        let mut offs = vec![0u16];
        offs.extend_from_slice(f.offsets);
        offs.push(bmp.w as u16);
        let _ = write!(
            out,
            "\n/// From `{}` ({} px tall).\npub static {}: Font = Font {{\n    height: {},\n    \
             stride: {stride},\n    chars: b\"{}\",\n    offsets: &{offs:?},\n    bits: &[",
            f.file,
            bmp.h,
            f.name,
            bmp.h,
            f.chars.escape_default()
        );
        bytes_lit(&mut out, &bits);
        out.push_str("],\n};\n");
        if preview {
            write_png(
                &bmp,
                &preview_dir.join(format!("{}.png", f.name.to_lowercase())),
            );
        }
    }

    for f in PIXEL_FONTS {
        let (bmp, offs) = read_pixel_font(&assets.join(f.file), f);
        let (stride, bits) = bmp.pack();
        let _ = write!(
            out,
            "\n/// From `{}` (pixel grid × {}, {} px tall).\npub static {}: Font = Font {{\n    height: {},\n    \
             stride: {stride},\n    chars: b\"{}\",\n    offsets: &{offs:?},\n    bits: &[",
            f.file,
            f.scale,
            bmp.h,
            f.name,
            bmp.h,
            f.chars.escape_default()
        );
        bytes_lit(&mut out, &bits);
        out.push_str("],\n};\n");
        if preview {
            write_png(
                &bmp,
                &preview_dir.join(format!("{}.png", f.name.to_lowercase())),
            );
        }
    }

    for (name, file) in IMAGES {
        let bmp = read_ss(&assets.join(file));
        let (stride, bits) = bmp.pack();
        let _ = write!(
            out,
            "\n/// From `{file}`.\npub static {name}: Image = Image {{\n    w: {},\n    h: {},\n    \
             stride: {stride},\n    bits: &[",
            bmp.w, bmp.h
        );
        bytes_lit(&mut out, &bits);
        out.push_str("],\n};\n");
        if preview {
            write_png(
                &bmp,
                &preview_dir.join(format!("{}.png", name.to_lowercase())),
            );
        }
    }

    let qr = qr_bitmap(REPO_QR);
    let (stride, bits) = qr.pack();
    let _ = write!(
        out,
        "\n/// QR code for `{REPO_QR}` (version 2-M, 1 = dark module, no quiet zone).\n\
         pub static QR_REPO: Image = Image {{\n    w: {},\n    h: {},\n    stride: {stride},\n    bits: &[",
        qr.w, qr.h
    );
    bytes_lit(&mut out, &bits);
    out.push_str("],\n};\n");
    if preview {
        write_png(&qr, &preview_dir.join("qr_repo.png"));
    }

    let dest = root.join("crates/swet-heart/src/gfx/assets.rs");
    std::fs::write(&dest, out).expect("write assets.rs");
    eprintln!("wrote {}", dest.display());
}

fn write_png(bmp: &Bitmap, path: &Path) {
    let data: Vec<u8> = bmp
        .px
        .iter()
        .map(|&p| if p { 0xFF } else { 0x00 })
        .collect();
    let file = std::io::BufWriter::new(std::fs::File::create(path).expect("png"));
    let mut enc = png::Encoder::new(file, bmp.w as u32, bmp.h as u32);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(&data))
        .expect("png data");
}
