# Espresso scale (ESP32 / SSD1331)

Bare-metal Rust firmware for an ESP32, HX711 load-cell ADC, SSD1331 RGB OLED,
and two buttons. This branch remains an experimental scale/loop-timing build:
it calibrates at startup, uses a two-button hold to tare, and stops after 250
completed measurement iterations. The menu is not connected yet.

## Build

On an x86_64 Debian/Ubuntu orb, install the prerequisites and fetch the locked
dependencies with:

```sh
.agents/setup
. "$HOME/.cargo/env"
. "$HOME/export-esp.sh"
cargo build --release --locked
```

Setup pins `espup` 0.19.0 and the Espressif Rust toolchain 1.97.0.0, installed
under the name `esp`. It is safe to rerun. On other systems, install Rust with
[rustup](https://rustup.rs/) and follow the
[Espressif toolchain instructions](https://docs.espressif.com/projects/rust/book/getting-started/toolchain.html),
using `espup install --targets esp32 --toolchain-version 1.97.0.0`.
Unix shells must source `export-esp.sh` before building.

The output is `target/xtensa-esp32-none-elf/release/espresso_scale_rs`.
Use the release build for hardware: unoptimized GPIO/delay code can violate
the HX711's timing requirements. Flashing additionally requires `espflash`
and a connected ESP32; `cargo run --release --locked` uses the configured runner.

## Dependencies (November 2026)

The firmware uses `esp-hal` 1.2.2 (replacing `esp32-hal`), `esp-alloc` 0.11.0,
`esp-backtrace` 0.20.0, `esp-println` 0.18.0, and the ESP-IDF bootloader app
descriptor from `esp-bootloader-esp-idf` 0.6.0. The HAL's `unstable` feature is
needed for the existing periodic hardware timer. `Cargo.lock` records the
resolved dependency versions.

The published `hx711` 0.7.0 driver supports `embedded-hal` 1.0 and replaces the
old Git branch. Button sampling also uses HAL 1.0 directly;
[rust-embedded-community/ssd1331](https://github.com/rust-embedded-community/ssd1331)
still uses HAL 0.2, so `embedded-hal-compat` bridges its GPIO/SPI interfaces.

Upstream SSD1331's latest release is **0.3.0**; the old local path dependency
claimed 0.4.0 but its source was not in this repository. No sibling checkout is
needed now. Its legacy graphics feature is disabled, and `src/display.rs`
connects embedded-graphics 0.8.2 to the driver's existing framebuffer without
duplicating it or maintaining a driver fork.

## Button and ADC timing

Once weighing starts, TIMG1 timer0 samples the active-low buttons every 10 ms,
independently of ADC conversion waits and blocking display transfers. Both
buttons need 20 consecutive pressed samples (nominally 200 ms). The first
button may already be held when the second is pressed. Each combined hold
queues one tare; releasing either button rearms it. Saturating counters avoid
the old debounce dependency's long-hold rollover. Multiple requests before a
measurement coalesce; the foreground applies the pending tare to its rolling
average after incorporating the next reading. Calibration does not accept tare
requests, and the benchmark's final infinite loop does not process them.

Each HX711 readiness check/readout runs in a critical section. A `WouldBlock`
retry releases it, so interrupts remain available while waiting for conversion;
the 24 data pulses and gain-selection pulse cannot be stretched by ordinary
interrupt handlers. The driver's blocking reset/first-read constructor runs
before application interrupts are enabled, outside a critical section.

This is not a measured pulse-width guarantee. Validate the release firmware on
hardware with a logic analyzer: PD_SCK high must stay within the HX711's 50 µs
maximum during readout (more than 60 µs powers it down). The intentional startup
reset holds it high for 70 µs. Check button sampling under display/UART load too;
long critical sections can delay/coalesce timer interrupts rather than replaying
missed samples. Display refresh remains blocking, at least 24.576 ms per full
frame at 4 MHz.

## Checks

```sh
cargo +stable test --locked --lib --target x86_64-unknown-linux-gnu
cargo +stable fmt --all -- --check
cargo build --release --locked
```

Host tests check button hold thresholds, bounce rejection, staggered presses,
release/rearm and long-hold suppression, plus RGB565 output bytes, clipping,
all display rotations, and clearing through the real SSD1331 driver's SPI
output. They do not test ESP32 interrupt delivery or electrical timing, and do
not replace a load-cell/display hardware test. Existing unused-code warnings
come from unfinished scale/menu functionality.

## Wiring retained from this branch

| Function | ESP32 GPIO |
|---|---:|
| Display SCK | 18 |
| SPI MISO (retained, not used by the display) | 19 |
| Display DC | 22 |
| Display MOSI | 23 |
| Display reset | 5 |
| HX711 DOUT | 25 |
| HX711 clock | 26 |
| Menu button (pull-up) | 2 |
| Tare button (pull-up) | 4 |

Display SPI remains mode 0 at 4 MHz. There is no software-controlled chip select;
the hardware must keep the SSD1331 selected. Cycle logs now use the HAL's
monotonic clock and include time units rather than raw timer ticks.
