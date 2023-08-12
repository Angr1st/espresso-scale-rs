#![no_std]
#![no_main]
#![feature(unwrap_infallible)]

extern crate alloc;
use alloc::format;
use alloc::string::ToString;
use debounced_button::ButtonConfig;
use embedded_graphics::{
    mono_font::{
        ascii::{FONT_6X10, FONT_9X18_BOLD},
        MonoTextStyleBuilder,
    },
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, Text},
    Drawable,
};
use esp_backtrace as _;
use esp_println::println;
use hal::{
    clock::ClockControl,
    peripherals::Peripherals,
    prelude::{nb::block, *},
    timer::TimerGroup,
    Delay, Rtc, IO,
};
use hx711::Hx711;
use ssd1309::prelude::*;

use crate::scale::{Calibrating, Init, Scale, Weighing};

mod scale;

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

fn init_heap() {
    const HEAP_SIZE: usize = 32 * 1024;

    extern "C" {
        static mut _heap_start: u32;
        static mut _heap_end: u32;
    }

    unsafe {
        let heap_start = &_heap_start as *const _ as usize;
        let heap_end = &_heap_end as *const _ as usize;
        assert!(
            heap_end - heap_start > HEAP_SIZE,
            "Not enough available heap memory."
        );
        ALLOCATOR.init(heap_start as *mut u8, HEAP_SIZE);
    }
}

const SAMPLES : usize = 8;
const SAMPLES_INDEX : usize = SAMPLES - 1;

#[entry]
fn main() -> ! {
    init_heap();
    let peripherals = Peripherals::take();
    let mut system = peripherals.DPORT.split();
    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();

    // Disable the RTC and TIMG watchdog timers
    let mut rtc = Rtc::new(peripherals.RTC_CNTL);
    let timer_group0 = TimerGroup::new(
        peripherals.TIMG0,
        &clocks,
        &mut system.peripheral_clock_control,
    );
    let mut wdt0 = timer_group0.wdt;
    let timer_group1 = TimerGroup::new(
        peripherals.TIMG1,
        &clocks,
        &mut system.peripheral_clock_control,
    );
    let mut wdt1 = timer_group1.wdt;
    rtc.rwdt.disable();
    wdt0.disable();
    wdt1.disable();

    let io: IO = IO::new(peripherals.GPIO, peripherals.IO_MUX);

    let mut delay = Delay::new(&clocks);

    //Display Config
    // esp32 -> (read connected to) ssd1309
    let dc = io.pins.gpio18.into_push_pull_output(); //V_SPI_CLK SCK -> dc
    let cs = io.pins.gpio19; //V_SPI_Q MISO -> cs
    let scl = io.pins.gpio22; //V_SPI_WP SCL -> SCL
    let sda = io.pins.gpio23; //V_SPI_D MOSI -> SDA
    let mut res = io.pins.gpio5.into_push_pull_output(); //V_SPI_CSO SS -> RES

    let spi = hal::Spi::new_no_cs(
        peripherals.SPI3,
        scl,
        sda,
        cs,
        400u32.kHz(),
        hal::spi::SpiMode::Mode0,
        &mut system.peripheral_clock_control,
        &clocks,
    );

    let spi_interface = display_interface_spi::SPIInterfaceNoCS::new(spi, dc);

    let mut display: GraphicsMode<_> = ssd1309::Builder::new().connect(spi_interface).into();

    display.reset(&mut res, &mut delay).unwrap();

    display.init().unwrap();
    display.flush().unwrap();

    //Init hx711
    let dout = io.pins.gpio25.into_floating_input();
    let pd_sck = io.pins.gpio26.into_push_pull_output();

    let mut hx711 = Hx711::new(delay, dout, pd_sck).into_ok();

    //Init Button B1 Menu(GPIO 2)
    let menu_button = io.pins.gpio2.into_pull_up_input();

    //Init Button B2 Tara(GPIO4)
    let tara_button = io.pins.gpio4.into_pull_up_input();
    let mut tara_button = debounced_button::Button::new(tara_button, 3u16, ButtonConfig::default());

    // Start timer (5 second interval)
    let mut timer0 = timer_group0.timer0;
    timer0.start(5u64.secs());

    // Specify different text styles
    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();
    let text_style_big = MonoTextStyleBuilder::new()
        .font(&FONT_9X18_BOLD)
        .text_color(BinaryColor::On)
        .build();

    // Fill display bufffer with a centered text with two lines (and two text
    // styles)
    Text::with_alignment(
        "esp32",
        display.bounding_box().center() + Point::new(0, 0),
        text_style_big,
        Alignment::Center,
    )
    .draw(&mut display)
    .unwrap();

    Text::with_alignment(
        "espresso-scale-rs",
        display.bounding_box().center() + Point::new(0, 14),
        text_style,
        Alignment::Center,
    )
    .draw(&mut display)
    .unwrap();

    // Write buffer to display
    display.flush().unwrap();
    // Clear display buffer
    display.clear();

    // Wait 5 seconds
    println!("Waiting for 5 seconds!");
    block!(timer0.wait()).unwrap();

    let state = Scale::<Init>::new();

    // Obtain the tara value
    let tara_message = "Obtaining tara ...";
    println!("{}", tara_message);

    // Write single-line centered text "Hello World" to buffer
    Text::with_alignment(
        tara_message,
        display.bounding_box().center(),
        text_style,
        Alignment::Center,
    )
    .draw(&mut display)
    .unwrap();

    // Write buffer to display
    display.flush().unwrap();
    // Clear display buffer
    display.clear();

    let tara = block!(receive_average(&mut hx711, 16)).into_ok();

    let tara_result = format!("Tara: {}", tara);
    println!("{}", tara_result);

    // Write single-line centered text "Hello World" to buffer
    Text::with_alignment(
        tara_message,
        display.bounding_box().center(),
        text_style,
        Alignment::Center,
    )
    .draw(&mut display)
    .unwrap();

    // Write buffer to display
    display.flush().unwrap();
    // Clear display buffer
    display.clear();

    let state = state.init(tara);
    //scale.set_scale(-0.360342);

    // Wait 3 seconds
    timer0.start(3u64.secs());
    //println!("Waiting for 3 seconds! Next output message");
    //block!(timer0.wait()).unwrap();

    // Write single-line centered text "Start Calibration" to buffer
    //Text::with_alignment(
    //    "Start Calibration",
    //    display.bounding_box().center(),
    //    text_style,
    //    Alignment::Center,
    //)
    //.draw(&mut display)
    //.unwrap();

    // Write buffer to display
    //display.flush().unwrap();
    // Clear display buffer
    //display.clear();

    //println!("Waiting for 3 seconds! Next calibrate with 100g");
    //block!(timer0.wait()).unwrap();

    //let mut initialised = false;
    //load first raw value
    //2586.4148
    let raw_value = block!(receive_average(&mut hx711, 8)).into_ok();
    let mut state = state.calibrate_fixed_value(2586.4148);

    //let current_val = state.get_value(raw_value);
    //let scale_scale = state.get_scale();
    //println!(
    //    "Raw: {}; Result: {}; Current scale: {}",
    //    raw_value, current_val, scale_scale
    //);

    let mut measurements : [i32; SAMPLES] = [0; SAMPLES];
    let mut measurement_index: PositionTracker<SAMPLES_INDEX> = PositionTracker::default();

    //get initial samples
    for _ in 0..SAMPLES {
        let measurement = block!(hx711.retrieve()).into_ok();
        update_array(&mut measurements, measurement, &measurement_index);
        measurement_index.next();
    }

    println!("Current index is {}", measurement_index.current_index());

    loop {
        tara_button.poll();
        let measurement = block!(hx711.retrieve()).into_ok();
        update_array(&mut measurements, measurement, &measurement_index);
        measurement_index.next();
        let raw_value = compute_average(&measurements);
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
            &format!("{:.1$}g", current_val_g, 2),
            display.bounding_box().center(),
            text_style_big,
            Alignment::Center,
        )
        .draw(&mut display)
        .unwrap();

        tara_button.poll();

        // Write buffer to display
        display.flush().unwrap();
        // Clear display buffer
        display.clear();

        tara_button.poll();
        let tara_state = tara_button.read();
        match tara_state {
            debounced_button::ButtonState::Down => {println!("Down")},
            debounced_button::ButtonState::Press => println!("Press"),
            debounced_button::ButtonState::Pressing => state.tare(raw_value),
            debounced_button::ButtonState::LongPress => {println!("LongPress")},
            debounced_button::ButtonState::Idle => {println!("Idle")},
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
    D: embedded_hal::blocking::delay::DelayUs<u32>,
    IN: embedded_hal::digital::v2::InputPin<Error = EIN>,
    OUT: embedded_hal::digital::v2::OutputPin<Error = EOUT>,
{
    let mut val: i32 = 0;
    // Obtain the tara value
    for _ in 0..times {
        val += block!(hx711.retrieve())?; // or unwrap, see features below
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
        Self {
            index: 0
        }
    }
}

fn update_array<const ARRAYLENGTH: usize, const MAXINDEX: usize>(array : &mut [i32; ARRAYLENGTH], next_value: i32, position: &PositionTracker<MAXINDEX>) {
    array[position.current_index()] = next_value;
}

fn compute_average(slice: &[i32]) -> i32 {
    let sum : i32 = slice.iter().sum();

    sum / (slice.len() as i32)
}