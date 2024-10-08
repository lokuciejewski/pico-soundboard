#![no_std]
#![no_main]

use core::cell::RefCell;
extern crate alloc;

use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_rp::gpio::{Level, Output};

use embassy_rp::peripherals::{I2C0, USB};
use embassy_rp::spi::{self, Phase, Polarity, Spi};
use embassy_rp::usb::Driver;
use embassy_rp::{bind_interrupts, i2c};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;

use core::mem::MaybeUninit;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Ticker};
use embedded_alloc::Heap;
use pico_soundboard::animations::loading_circle;
use pico_soundboard::board::Board;
use pico_soundboard::usb_device::setup_usb_device;
use pico_soundboard::{ButtonState, Colour};
use {defmt_rtt as _, panic_probe as _};

#[global_allocator]
static HEAP: Heap = Heap::empty();
const HEAP_SIZE: usize = 8192;
static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
    I2C0_IRQ => i2c::InterruptHandler<I2C0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // Init heap
    unsafe { HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE) }

    let p = embassy_rp::init(Default::default());
    let usb_driver = Driver::new(p.USB, Irqs);

    let sda = p.PIN_4;
    let scl = p.PIN_5;

    info!("I2C setup...");
    let i2c = i2c::I2c::new_async(p.I2C0, scl, sda, Irqs, i2c::Config::default());

    info!("SPI setup...");
    let miso = p.PIN_16;
    let mosi = p.PIN_19;
    let clk = p.PIN_18;
    let cs = p.PIN_17;

    let _cs = Output::new(cs, Level::Low);

    // create SPI
    let mut config = spi::Config::default();
    config.frequency = 4_000_000;
    config.phase = Phase::CaptureOnFirstTransition;
    config.polarity = Polarity::IdleLow;

    let spi = Spi::new(p.SPI0, clk, mosi, miso, p.DMA_CH0, p.DMA_CH1, config);

    // RefCell needed for mutable access
    let board: Mutex<ThreadModeRawMutex, _> = Mutex::new(RefCell::new(Board::new(i2c, spi).await));
    info!("Board initialised!");

    {
        let mut _board = board.lock().await;
        _board.get_mut().lock_led_states(&ButtonState::Idle);
        loading_circle(&mut _board.get_mut(), Colour::rgb(0x50, 0x0, 0x50), 100);
        // Await serial connection to proceed
    }

    // This is needed so that led refresh rate is independent of USB poll rate
    let rgb_fut = async {
        let mut ticker = Ticker::every(Duration::from_millis(1));
        loop {
            {
                board.lock().await.get_mut().refresh_leds().await;
            }
            ticker.next().await;
        }
    };

    // Run everything concurrently.
    join(rgb_fut, setup_usb_device(usb_driver, &board)).await;
}
