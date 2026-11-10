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
    pixelcolor::Rgb565,
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
use ssd1331::*;

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
    let sck = io.pins.gpio18; //Serial Clock V_SPI_CLK SCK -> dc
    let miso = io.pins.gpio19; //Master In V_SPI_Q MISO -> cs
    let scl_dc = io.pins.gpio22.into_push_pull_output(); //Also Serial Clock V_SPI_WP SCL -> SCL
    let sda = io.pins.gpio23; //Master out V_SPI_D MOSI -> SDA
    let mut cs_rst = io.pins.gpio5.into_push_pull_output(); //Slave Select V_SPI_CSO SS -> RES

    let spi = hal::Spi::new_no_cs(
        peripherals.SPI3,
        sck,
        sda,
        miso,
        4u32.MHz(),
        hal::spi::SpiMode::Mode0,
        &mut system.peripheral_clock_control,
        &clocks,
    );

    let mut display = Ssd1331::new(spi, scl_dc, DisplayRotation::Rotate0);

    display.reset(&mut cs_rst, &mut delay).unwrap();

    display.init().unwrap();
    display.flush().unwrap();

    let center = display.bounding_box().center();
    println!("Display Center: {}, {}", center.x, center.y);

    //Init hx711
    let dout = io.pins.gpio25.into_floating_input();
    let pd_sck = io.pins.gpio26.into_push_pull_output();

    let mut hx711 = Hx711::new(delay, dout, pd_sck).into_ok();

    //Init Button B1 Menu(GPIO 2)
    let menu_button = io.pins.gpio2.into_pull_up_input();
    let mut menu_button = debounced_button::Button::new(menu_button, 2u16, ButtonConfig::default());

    //Init Button B2 Tara(GPIO4)
    let tara_button = io.pins.gpio4.into_pull_up_input();
    let mut tara_button = debounced_button::Button::new(tara_button, 2u16, ButtonConfig::default());

    // Start timer (5 second interval)
    let mut timer0 = timer_group0.timer0;
    timer0.start(5u64.secs());

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
        display.bounding_box().center() + Point::new(0, 0),
        text_style_big,
        Alignment::Center,
    )
    .draw(&mut display)
    .unwrap();

    Text::with_alignment(
        "espresso-scale",
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
    let tara_message = "Taring...";
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
    println!("Waiting for 3 seconds! Next output message");
    block!(timer0.wait()).unwrap();

    // Write single-line centered text "Calibration" to buffer
    Text::with_alignment(
        "Calibration",
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

    println!("Waiting for 3 seconds! Next calibrate with 100g");
    block!(timer0.wait()).unwrap();

    let mut initialised = false;
    //load first raw value
    //2586.4148
    let _raw_value = block!(receive_average(&mut hx711, 8)).expect("Should be able to retrieve value from hx711!");
    let mut state = state.calibrate(_raw_value);//calibrate_fixed_value(2586.4148);

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
        let measurement = block!(hx711.retrieve()).expect("Should be able to retrieve value from hx711!");
        update_array(&mut measurements, measurement, &measurement_index);
        measurement_index.next();
    }

    println!("Current index is {}", measurement_index.current_index());

    let mut cycle_count: u32 = 0;
    let before_cycles = timer0.now();
    println!("Current timestamp: {before_cycles}");

    loop {
        let measurement = block!(hx711.retrieve()).expect("Should be able to retrieve value from hx711!");
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
            &format!("{:.1$}g", current_val_g, 1),
            display.bounding_box().center(),
            text_style_big,
            Alignment::Center,
        )
        .draw(&mut display)
        .unwrap();

        // Write buffer to display
        display.flush().unwrap();
        // Clear display buffer
        display.clear();

        menu_button.poll();
        tara_button.poll();

        let current_index = measurement_index.current_index();
        
        if current_index == SAMPLES_INDEX {
            let menu_state = menu_button.read();

            let tara_state = tara_button.read();
            match (menu_state, tara_state) {
                (debounced_button::ButtonState::Down, debounced_button::ButtonState::Down) => no_op(),
                (debounced_button::ButtonState::Press, debounced_button::ButtonState::Press) => no_op(),
                (debounced_button::ButtonState::Pressing, debounced_button::ButtonState::Pressing) => state.tare(raw_value),
                (debounced_button::ButtonState::LongPress, debounced_button::ButtonState::LongPress) => no_op(),
                (debounced_button::ButtonState::Idle, debounced_button::ButtonState::Idle) => no_op(),
                _ => println!("different state")
            }
        }

        //Stop if certain amount of cycles was reached.
        if cycle_count == 250 {
            let after_cycles = timer0.now();
            let diff = after_cycles - before_cycles;
            println!("Current timestamp: {after_cycles} after 250 cycles. Diff: {diff}");
            loop {}
            //Start timestamp:   50965234
            //After timestamp: 1191432346 after 250 cycles. 
            //Diff:            1140467112
        }
        cycle_count += 1;

        // Wait 5 seconds
        //block!(timer0.wait()).unwrap();
    }
}

fn no_op() { }

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
