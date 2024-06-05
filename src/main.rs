#![no_std]
#![no_main]

use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};
extern crate alloc;

use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::{join, join3};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{I2C0, USB};
use embassy_rp::spi::{self, Phase, Polarity, Spi};
use embassy_rp::usb::Driver;
use embassy_rp::{bind_interrupts, i2c};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Ticker};
use embassy_usb::class::hid::{HidReaderWriter, ReportId, RequestHandler, State};
use embassy_usb::control::OutResponse;
use embassy_usb::{Builder, Config, Handler};
use embedded_alloc::Heap;
use pico_soundboard::board::Board;
use pico_soundboard::rgbleds::{fade_in, fade_out, solid};
use pico_soundboard::Colour;
use rand::rngs::SmallRng;
use rand::{RngCore, SeedableRng};
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};
use {defmt_rtt as _, panic_probe as _};

#[global_allocator]
static HEAP: Heap = Heap::empty();
use core::mem::MaybeUninit;
const HEAP_SIZE: usize = 1024;
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
    // Create the driver, from the HAL.
    let driver = Driver::new(p.USB, Irqs);

    // Create embassy-usb Config
    let mut config = Config::new(0x1209, 0x2137);
    config.device_class = 0x3;
    config.manufacturer = Some("");
    config.product = Some("DnD Soundboard");
    config.serial_number = Some("00000001");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    // Add Microsoft OS descriptor.
    let mut msos_descriptor = [0; 256];
    let mut control_buf = [0; 64];
    let mut request_handler = MyRequestHandler {};
    let mut device_handler = MyDeviceHandler::new();

    let mut state = State::new();

    let mut builder = Builder::new(
        driver,
        config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut msos_descriptor,
        &mut control_buf,
    );

    builder.handler(&mut device_handler);

    // Create classes on the builder.
    let config = embassy_usb::class::hid::Config {
        report_descriptor: KeyboardReport::desc(),
        request_handler: None,
        poll_ms: 60,
        max_packet_size: 64,
    };
    let hid = HidReaderWriter::<_, 1, 8>::new(&mut builder, &mut state, config);

    // Build the builder.
    let mut usb = builder.build();

    // Run the USB device.
    let usb_fut = usb.run();

    let (reader, mut writer) = hid.split();

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
    let mut small_rng = SmallRng::seed_from_u64(2137);

    {
        let _board = board.lock().await;
        for i in 0..16 {
            let timeout = small_rng.next_u32() as u16 as usize / 10;
            let colour = Colour::random(&mut small_rng);
            _board
                .borrow_mut()
                .rgb_leds
                .add_state(i, fade_out(0b11110000, colour.clone(), 500));
            _board
                .borrow_mut()
                .rgb_leds
                .add_state(i, solid(0x00, colour.clone(), timeout));
            _board
                .borrow_mut()
                .rgb_leds
                .add_state(i, fade_in(0b11110000, colour.clone(), 500));
            _board
                .borrow_mut()
                .rgb_leds
                .add_state(i, solid(0b11110000, colour, timeout));
        }
    }

    let in_fut = async {
        loop {
            let keycodes: [u8; 6];
            {
                keycodes = board
                    .lock()
                    .await
                    .borrow_mut()
                    .update_status()
                    .await
                    .unwrap();
            }
            let report = KeyboardReport {
                keycodes,
                leds: 0,
                modifier: 0,
                reserved: 0,
            };
            // Send the report.
            match writer.write_serialize(&report).await {
                Ok(()) => {}
                Err(e) => warn!("Failed to send report: {:?}", e),
            };
        }
    };

    let out_fut = async {
        reader.run(false, &mut request_handler).await;
    };

    // This is needed so that led refresh rate is independent of USB poll rate
    let rgb_fut = async {
        let mut ticker = Ticker::every(Duration::from_millis(1));
        loop {
            {
                board.lock().await.borrow_mut().rgb_leds.refresh().await;
            }
            ticker.next().await;
        }
    };

    // Run everything concurrently.
    join3(usb_fut, rgb_fut, join(in_fut, out_fut)).await;
}

struct MyRequestHandler {}

impl RequestHandler for MyRequestHandler {
    fn get_report(&mut self, id: ReportId, _buf: &mut [u8]) -> Option<usize> {
        info!("Get report for {:?}", id);
        None
    }

    fn set_report(&mut self, id: ReportId, data: &[u8]) -> OutResponse {
        info!("Set report for {:?}: {=[u8]}", id, data);
        OutResponse::Accepted
    }

    fn set_idle_ms(&mut self, id: Option<ReportId>, dur: u32) {
        info!("Set idle rate for {:?} to {:?}", id, dur);
    }

    fn get_idle_ms(&mut self, id: Option<ReportId>) -> Option<u32> {
        info!("Get idle rate for {:?}", id);
        None
    }
}

struct MyDeviceHandler {
    configured: AtomicBool,
}

impl MyDeviceHandler {
    fn new() -> Self {
        MyDeviceHandler {
            configured: AtomicBool::new(false),
        }
    }
}

impl Handler for MyDeviceHandler {
    fn enabled(&mut self, enabled: bool) {
        self.configured.store(false, Ordering::Relaxed);
        if enabled {
            info!("Device enabled");
        } else {
            info!("Device disabled");
        }
    }

    fn reset(&mut self) {
        self.configured.store(false, Ordering::Relaxed);
        info!("Bus reset, the Vbus current limit is 100mA");
    }

    fn addressed(&mut self, addr: u8) {
        self.configured.store(false, Ordering::Relaxed);
        info!("USB address set to: {}", addr);
    }

    fn configured(&mut self, configured: bool) {
        self.configured.store(configured, Ordering::Relaxed);
        if configured {
            info!(
                "Device configured, it may now draw up to the configured current limit from Vbus."
            )
        } else {
            info!("Device is no longer configured, the Vbus current limit is 100mA.");
        }
    }
}
