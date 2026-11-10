#![no_std]
#![no_main]

extern crate alloc;
use alloc::format;
use core::{
    cell::RefCell,
    sync::atomic::{AtomicBool, Ordering},
};
use critical_section::Mutex;
use embedded_graphics::{
    mono_font::{
        ascii::{FONT_6X10, FONT_9X18_BOLD},
        MonoTextStyleBuilder,
    },
    pixelcolor::Rgb565,
    prelude::*,
    text::{Alignment, Text},
    Drawable,
};
use embedded_hal_compat::ReverseCompat;
use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    spi::{
        master::{Config as SpiConfig, Spi},
        Mode,
    },
    time::{Duration, Instant, Rate},
    timer::{timg::TimerGroup, PeriodicTimer},
    Blocking,
};
use esp_println::println;
use espresso_scale_rs::{
    buttons::{TareButtons, POLL_INTERVAL_MS},
    display::Graphics,
};
use hx711::Hx711;
use nb::block;
use ssd1331::*;

use crate::scale::{Init, Scale};

mod scale;

esp_bootloader_esp_idf::esp_app_desc!();

const SAMPLES: usize = 8;
const SAMPLES_INDEX: usize = SAMPLES - 1;

struct ButtonSampler {
    timer: PeriodicTimer<'static, Blocking>,
    buttons: TareButtons<Input<'static>>,
}

static BUTTON_SAMPLER: Mutex<RefCell<Option<ButtonSampler>>> = Mutex::new(RefCell::new(None));
static TARE_REQUESTED: AtomicBool = AtomicBool::new(false);

#[esp_hal::handler]
fn sample_buttons() {
    critical_section::with(|cs| {
        let mut sampler = BUTTON_SAMPLER.borrow_ref_mut(cs);
        let sampler = sampler.as_mut().unwrap();
        sampler.timer.clear_interrupt();
        if sampler.buttons.poll() {
            // Only a flag crosses into the foreground; scale state stays there.
            TARE_REQUESTED.store(true, Ordering::Relaxed);
        }
    });
}

#[esp_hal::main]
fn main() -> ! {
    // init configures the clocks and disables the RTC/TIMG watchdogs.
    let peripherals = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(size: 32 * 1024);
    let timer_group0 = TimerGroup::new(peripherals.TIMG0);
    let delay = Delay::new();

    // Preserve the existing SSD1331 wiring; chip select is externally asserted.
    let dc = Output::new(peripherals.GPIO22, Level::Low, OutputConfig::default());
    let mut reset = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());
    let spi = Spi::new(
        peripherals.SPI3,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(4))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(peripherals.GPIO18)
    .with_mosi(peripherals.GPIO23)
    .with_miso(peripherals.GPIO19);
    let mut display = Ssd1331::new(spi.reverse(), dc.reverse_cell(), DisplayRotation::Rotate0);

    // Same reset sequence as Ssd1331::reset, using embedded-hal 1.0's delay.
    delay.delay_millis(1);
    reset.set_low();
    delay.delay_millis(1);
    reset.set_high();
    display.init().unwrap();
    display.flush().unwrap();

    let center = Graphics(&mut display).bounding_box().center();
    println!("Display Center: {}, {}", center.x, center.y);

    //Init hx711
    let dout = Input::new(peripherals.GPIO25, InputConfig::default());
    let pd_sck = Output::new(peripherals.GPIO26, Level::Low, OutputConfig::default());

    // The driver's constructor resets the ADC and waits for a first conversion.
    // Do this before enabling application interrupts, not inside a critical section.
    let mut hx711 = Hx711::new(delay, dout, pd_sck).unwrap();

    //Init Button B1 Menu(GPIO 2)
    let menu_button = Input::new(
        peripherals.GPIO2,
        InputConfig::default().with_pull(Pull::Up),
    );

    //Init Button B2 Tara(GPIO4)
    let tara_button = Input::new(
        peripherals.GPIO4,
        InputConfig::default().with_pull(Pull::Up),
    );
    let buttons = TareButtons::new(menu_button, tara_button);

    // Start timer (5 second interval)
    let mut timer0 = PeriodicTimer::new(timer_group0.timer0);
    timer0.start(Duration::from_secs(5)).unwrap();

    // Specify different text styles
    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(Rgb565::WHITE)
        .build();
    let text_style_big = MonoTextStyleBuilder::new()
        .font(&FONT_9X18_BOLD)
        .text_color(Rgb565::WHITE)
        .build();

    // Fill display bufffer with a centered text with two lines (and two text
    // styles)
    Text::with_alignment(
        "esp32",
        center + Point::new(0, 0),
        text_style_big,
        Alignment::Center,
    )
    .draw(&mut Graphics(&mut display))
    .unwrap();

    Text::with_alignment(
        "espresso-scale",
        center + Point::new(0, 14),
        text_style,
        Alignment::Center,
    )
    .draw(&mut Graphics(&mut display))
    .unwrap();

    // Write buffer to display
    display.flush().unwrap();
    // Clear display buffer
    display.clear();

    // Wait 5 seconds
    println!("Waiting for 5 seconds!");
    timer0.wait();

    let state = Scale::<Init>::new();

    // Obtain the tara value
    let tara_message = "Taring...";
    println!("{}", tara_message);

    // Write single-line centered text "Hello World" to buffer
    Text::with_alignment(tara_message, center, text_style, Alignment::Center)
        .draw(&mut Graphics(&mut display))
        .unwrap();

    // Write buffer to display
    display.flush().unwrap();
    // Clear display buffer
    display.clear();

    let tara = block!(receive_average(&mut hx711, 16)).unwrap();

    let tara_result = format!("Tara: {}", tara);
    println!("{}", tara_result);

    // Write single-line centered text "Hello World" to buffer
    Text::with_alignment(tara_message, center, text_style, Alignment::Center)
        .draw(&mut Graphics(&mut display))
        .unwrap();

    // Write buffer to display
    display.flush().unwrap();
    // Clear display buffer
    display.clear();

    let state = state.init(tara);
    //scale.set_scale(-0.360342);

    // Wait 3 seconds
    timer0.start(Duration::from_secs(3)).unwrap();
    println!("Waiting for 3 seconds! Next output message");
    timer0.wait();

    // Write single-line centered text "Calibration" to buffer
    Text::with_alignment("Calibration", center, text_style, Alignment::Center)
        .draw(&mut Graphics(&mut display))
        .unwrap();

    // Write buffer to display
    display.flush().unwrap();
    // Clear display buffer
    display.clear();

    println!("Waiting for 3 seconds! Next calibrate with 100g");
    timer0.wait();

    let mut initialised = false;
    //load first raw value
    //2586.4148
    let _raw_value = block!(receive_average(&mut hx711, 8))
        .expect("Should be able to retrieve value from hx711!");
    let mut state = state.calibrate(_raw_value); //calibrate_fixed_value(2586.4148);

    //let current_val = state.get_value(raw_value);
    //let scale_scale = state.get_scale();
    //println!(
    //    "Raw: {}; Result: {}; Current scale: {}",
    //    raw_value, current_val, scale_scale
    //);

    let mut measurements: [i32; SAMPLES] = [0; SAMPLES];
    let mut measurement_index: PositionTracker<SAMPLES_INDEX> = PositionTracker::default();

    //get initial samples
    for _ in 0..SAMPLES {
        let measurement = block!(receive_average(&mut hx711, 1))
            .expect("Should be able to retrieve value from hx711!");
        update_array(&mut measurements, measurement, &measurement_index);
        measurement_index.next();
    }

    println!("Current index is {}", measurement_index.current_index());

    // Button sampling starts when weighing starts. TIMG1 is independent of the
    // startup countdown and continues polling during ADC waits and display writes.
    let timer_group1 = TimerGroup::new(peripherals.TIMG1);
    let mut button_timer = PeriodicTimer::new(timer_group1.timer0);
    critical_section::with(|cs| {
        button_timer.set_interrupt_handler(sample_buttons);
        button_timer
            .start(Duration::from_millis(POLL_INTERVAL_MS))
            .unwrap();
        button_timer.listen();
        BUTTON_SAMPLER.borrow_ref_mut(cs).replace(ButtonSampler {
            timer: button_timer,
            buttons,
        });
    });

    let mut cycle_count: u32 = 0;
    let before_cycles = Instant::now();
    println!("Current timestamp: {before_cycles}");

    loop {
        let measurement = block!(receive_average(&mut hx711, 1))
            .expect("Should be able to retrieve value from hx711!");
        update_array(&mut measurements, measurement, &measurement_index);
        measurement_index.next();
        let raw_value = compute_average(&measurements);
        if TARE_REQUESTED.swap(false, Ordering::Relaxed) {
            state.tare(raw_value);
        }
        let current_val = state.get_value(raw_value);
        let current_val_g = state.get_units(raw_value);
        let current_offset = state.get_offset();
        let current_scale = state.get_scale();

        println!(
            "Raw: {}; Result: {}; {}g; Offset: {}; Scale: {}",
            raw_value, current_val, current_val_g, current_offset, current_scale
        );

        // Write single-line centered text "Hello World" to buffer
        Text::with_alignment(
            &format!("{:.1$}g", current_val_g, 1),
            center,
            text_style_big,
            Alignment::Center,
        )
        .draw(&mut Graphics(&mut display))
        .unwrap();

        // Write buffer to display
        display.flush().unwrap();
        // Clear display buffer
        display.clear();

        // Count the completed iteration before checking the benchmark limit.
        cycle_count += 1;
        if cycle_count == 250 {
            let after_cycles = Instant::now();
            let diff = after_cycles - before_cycles;
            println!("Current timestamp: {after_cycles} after {cycle_count} cycles. Diff: {diff}");
            loop {}
            //Start timestamp:   50965234
            //After timestamp: 1191432346 after 250 cycles.
            //Diff:            1140467112
        }

        // Wait 5 seconds
        //block!(timer0.wait()).unwrap();
    }
}

fn receive_average<D, IN, OUT, EIN, EOUT>(
    hx711: &mut Hx711<D, IN, OUT>,
    times: i32,
) -> nb::Result<i32, hx711::Error<EIN, EOUT>>
where
    D: embedded_hal::delay::DelayNs,
    IN: embedded_hal::digital::InputPin<Error = EIN>,
    OUT: embedded_hal::digital::OutputPin<Error = EOUT>,
{
    let mut val: i32 = 0;
    // Obtain the tara value
    for _ in 0..times {
        // Mask interrupts only for one readiness check / 25-pulse readout.
        // WouldBlock retries happen outside the critical section, so waiting
        // for an ADC conversion never blocks the button timer interrupt.
        val += block!(critical_section::with(|_| hx711.retrieve()))?;
    }
    let tara = val / times;

    Ok(tara)
}

struct PositionTracker<const MAXINDEX: usize> {
    index: usize,
}

impl<const MAXINDEX: usize> PositionTracker<MAXINDEX> {
    pub fn next(&mut self) {
        if self.index == MAXINDEX {
            self.index = 0;
        } else {
            self.index = self.index + 1;
        }
    }

    pub fn current_index(&self) -> usize {
        self.index
    }
}

impl<const MAXINDEX: usize> Default for PositionTracker<MAXINDEX> {
    fn default() -> Self {
        Self { index: 0 }
    }
}

fn update_array<const ARRAYLENGTH: usize, const MAXINDEX: usize>(
    array: &mut [i32; ARRAYLENGTH],
    next_value: i32,
    position: &PositionTracker<MAXINDEX>,
) {
    array[position.current_index()] = next_value;
}

fn compute_average(slice: &[i32]) -> i32 {
    let sum: i32 = slice.iter().sum();

    sum / (slice.len() as i32)
}
