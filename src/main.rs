#![no_std]
#![no_main]

extern crate alloc;
use esp_backtrace as _;
use esp_println::println;
use hal::{clock::ClockControl, peripherals::Peripherals, prelude::{*, nb::block}, timer::TimerGroup, Rtc, Delay, IO};
use hx711::Hx711;
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
    let system = peripherals.DPORT.split();
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

    let mut val: i32 = 0;

    let dout = io.pins.gpio6.into_floating_input();
    let pd_sck = io.pins.gpio7.into_push_pull_output();

    let mut hx711 = Hx711::new(Delay::new(&clocks), dout, pd_sck).unwrap();

    // Obtain the tara value
    println!("Obtaining tara ...");
    const N: i32 = 8;
    for _ in 0..N {
        val += block!(hx711.retrieve()).unwrap(); // or unwrap, see features below
    }
    let tara = val / N;
    println!("Tara:   {}", tara);

    loop {}
}
