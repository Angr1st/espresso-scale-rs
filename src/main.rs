#![no_std]
#![no_main]
#![feature(unwrap_infallible)]

extern crate alloc;
use alloc::{vec::Vec, string::ToString};
use embedded_graphics::{
    mono_font::{
        ascii::{FONT_6X10, FONT_9X18_BOLD},
        MonoTextStyleBuilder,
    },
    pixelcolor::BinaryColor, text::{Text, Alignment}, prelude::*, Drawable,
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

#[entry]
fn main() -> ! {
    init_heap();
    let peripherals = Peripherals::take();
    let mut system = peripherals.DPORT.split();
    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();

    // Disable the RTC and TIMG watchdog timers
    let mut rtc = Rtc::new(peripherals.RTC_CNTL);
    let timer_group0 = TimerGroup::new(peripherals.TIMG0, &clocks);
    let mut wdt0 = timer_group0.wdt;
    let timer_group1 = TimerGroup::new(peripherals.TIMG1, &clocks);
    let mut wdt1 = timer_group1.wdt;
    rtc.rwdt.disable();
    wdt0.disable();
    wdt1.disable();
    println!("Hello world!");

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

    let mut val: i32 = 0;
    // Obtain the tara value
    println!("Obtaining tara ...");
    const N: i32 = 8;
    for _ in 0..N {
        val += block!(hx711.retrieve()).into_ok(); // or unwrap, see features below
    }
    let tara = val / N;
    println!("Tara: {}", tara);

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
    block!(timer0.wait()).unwrap();

    // Write single-line centered text "Hello World" to buffer
    Text::with_alignment(
        &val.to_string(),
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

    loop {
        let current_val = block!(receive_average(&mut hx711, 8)).into_ok();
        println!("Result: {}", current_val);

        // Write single-line centered text "Hello World" to buffer
        Text::with_alignment(
            &current_val.to_string(),
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

        // Wait 5 seconds
        //block!(timer0.wait()).unwrap();
    }
}

fn receive_average<D, IN, OUT, EIN, EOUT>(
    hx711: &mut Hx711<D, IN, OUT>,
    times: u8,
) -> nb::Result<i32, hx711::Error<EIN, EOUT>>
where
    D: embedded_hal::blocking::delay::DelayUs<u32>,
    IN: embedded_hal::digital::v2::InputPin<Error = EIN>,
    OUT: embedded_hal::digital::v2::OutputPin<Error = EOUT>,
{
    let mut results = Vec::with_capacity(times as usize);

    for _ in 0..times {
        let value = block!(hx711.retrieve())?;
        results.push(value);
    }

    let avg = results.iter().sum::<i32>() / times as i32;

    //use micromath::F32Ext;

    //let avg = avg.round() as i32;

    Ok(avg)
}
