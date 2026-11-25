# Local fonts (not in git)

The speed fonts below may be used in the firmware, but their licences don't
allow sharing the font files, so they stay out of the repo. Only their
rasterised digits are committed (`crates/swet-heart/src/gfx/assets.rs`).
`cargo run -p swet-assets` needs them here to regenerate that file:

| File | Font | Where | Licence |
|---|---|---|---|
| `digital-7 (mono).ttf` | Digital-7 Mono by Sizenko Alexander, Style-7 | [1001fonts: digital-7](https://www.1001fonts.com/digital-7-font.html) (`digital-7.zip`) | freeware for home use and in freeware software, credit needed; embedding allowed |
| `NexaRustSans-Black.otf` | Nexa Rust Sans Black by Fontfabric | [fontfabric.com](https://www.fontfabric.com/) (`nexa-rust.zip`, free version) | Fontfabric free font EULA, credit needed |
| `Blox2.ttf` | Blox by Brian Kent (Ænigma) | [1001fonts: blox](https://www.1001fonts.com/blox-brk-font.html) (`blox-brk.zip`) | freeware, any use; no selling, sharing for profit or altering without asking the author (kentpw@norwich.net) |
| `GLAMOURGIRL.TTF` | Glamour Girl by Jakob Fischer | [pizzadude.dk](https://www.pizzadude.dk/) (`glamourgirl.zip`) | non-commercial use only; no sharing without the author's permission |
| `plasdrip.ttf` | Plasma Drip by Brian Kent (Ænigma) | [1001fonts: plasma drip](https://www.1001fonts.com/plasma-drip-brk-font.html) (`plasma-drip.zip`) | as Blox |

Monoton (the "Neon" speed font) is SIL OFL 1.1, so its file is in git:
`assets/fonts/Monoton-Regular.ttf`, licence in `assets/fonts/Monoton-OFL.txt`.

Not used: **Pricedown**, **Mexcellent** and **Venus Rising** (Typodermic). The Typodermic Desktop
EULA forbids embedding in firmware, hardware or vehicles (§4.3), glyph libraries
made from rendered artwork (§4.5) and publicly available software (§4.6).
