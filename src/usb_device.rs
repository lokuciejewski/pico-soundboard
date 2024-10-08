use core::cell::RefCell;
use core::panic;
use core::sync::atomic::{AtomicBool, Ordering};
extern crate alloc;

use crate::board::Board;
use crate::serial_protocol::{NackType, ParseError, SerialCommand, SerialMessage};
use crate::transitions::{solid, transition_function_try_from_bytes};
use crate::{ButtonState, Colour};
use core::todo;
use defmt::*;
use embassy_futures::join::join4;
use embassy_rp::i2c;
use embassy_rp::i2c::I2c;
use embassy_rp::peripherals::{I2C0, SPI0, USB};
use embassy_rp::spi::{self, Spi};
use embassy_rp::usb::{Driver, Instance};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_usb::class::cdc_acm::CdcAcmClass;
use embassy_usb::class::hid::{HidReaderWriter, ReportId, RequestHandler};
use embassy_usb::control::OutResponse;
use embassy_usb::driver::EndpointError;
use embassy_usb::{Config, Handler};
use static_cell::StaticCell;
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

use {defmt_rtt as _, panic_probe as _};

type MutexedBoard = Mutex<
    ThreadModeRawMutex,
    RefCell<Board<I2c<'static, I2C0, i2c::Async>, Spi<'static, SPI0, spi::Async>>>,
>;

pub async fn setup_usb_device(driver: Driver<'static, USB>, board: &MutexedBoard) {
    // Create embassy-usb Config
    let mut config = Config::new(0x1209, 0x2137);
    // config.device_class = 0x3;
    config.device_class = 0xef;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;
    config.composite_with_iads = true;
    config.manufacturer = Some("");
    config.product = Some("DnD Soundboard");
    config.serial_number = Some("00000001");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    let mut builder = {
        static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static MSOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

        let builder = embassy_usb::Builder::new(
            driver,
            config,
            CONFIG_DESCRIPTOR.init([0; 256]),
            BOS_DESCRIPTOR.init([0; 256]),
            MSOS_DESCRIPTOR.init([0; 256]),
            CONTROL_BUF.init([0; 64]),
        );
        builder
    };

    let mut request_handler = MyRequestHandler {};
    static DEVICE_HANDLER: StaticCell<DeviceHandler> = StaticCell::new();
    static HID_STATE: StaticCell<embassy_usb::class::hid::State> = StaticCell::new();

    builder.handler(DEVICE_HANDLER.init(DeviceHandler::new()));

    // Create classes on the builder.
    let config = embassy_usb::class::hid::Config {
        report_descriptor: KeyboardReport::desc(),
        request_handler: None,
        poll_ms: 30,
        max_packet_size: 64,
    };

    let hid = HidReaderWriter::<_, 1, 8>::new(
        &mut builder,
        HID_STATE.init(embassy_usb::class::hid::State::new()),
        config,
    );

    let mut serial_class = {
        static STATE: StaticCell<embassy_usb::class::cdc_acm::State> = StaticCell::new();
        let state = STATE.init(embassy_usb::class::cdc_acm::State::new());
        CdcAcmClass::new(&mut builder, state, 64)
    };
    // Build the builder.
    let mut usb = builder.build();

    // Run the USB device.
    let usb_fut = usb.run();

    let (reader, mut writer) = hid.split();

    let in_fut = async {
        loop {
            let key_states = { board.lock().await.get_mut().update_status().await.unwrap() };
            let mut keycodes = [0u8; 6];

            key_states
                .into_iter()
                .filter(|&k| k != 0)
                .enumerate()
                .take(6)
                .for_each(|(idx, k)| keycodes[idx] = k);

            let report = KeyboardReport {
                keycodes,
                leds: 0,
                modifier: if keycodes == [0; 6] {
                    0
                } else {
                    KeyboardModifiers::LeftShift as u8 | KeyboardModifiers::LeftCtrl as u8
                },
                reserved: 0,
            };
            // Send the report.
            match writer.write_serialize(&report).await {
                Ok(()) => {}
                Err(e) => warn!("Failed to send report: {:?}", e),
            };
        }
    };

    let serial_loop = async {
        loop {
            serial_class.wait_connection().await;
            info!("Serial connected!");
            {
                let mut _board = board.lock().await;
                _board.get_mut().unlock_led_states();
                _board.get_mut().enable_keyboard_input();
                (0..16).for_each(|led| {
                    _board.get_mut().add_led_state(
                        led,
                        0,
                        solid(0x00, Colour::white(), 0, 0),
                        &ButtonState::Idle,
                    );
                });
            }
            let _ = serial_loop(&mut serial_class, board).await;
            info!("Serial disconnected!");
        }
    };

    let out_fut = async {
        reader.run(false, &mut request_handler).await;
    };

    join4(in_fut, out_fut, usb_fut, serial_loop).await;
}

#[allow(dead_code)]
#[repr(u8)]
enum KeyboardModifiers {
    None,
    LeftCtrl,
    LeftShift,
    LeftAlt,
    LeftGui,
    RightCtrl,
    RightShift,
    RightAlt,
    RightGui,
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

struct DeviceHandler {
    configured: AtomicBool,
}

impl DeviceHandler {
    fn new() -> Self {
        DeviceHandler {
            configured: AtomicBool::new(false),
        }
    }
}

impl Handler for DeviceHandler {
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

struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

async fn serial_loop<'d, T: Instance + 'd>(
    class: &mut CdcAcmClass<'d, Driver<'d, T>>,
    board: &MutexedBoard,
) -> Result<(), Disconnected> {
    let mut buf = [0; 10];
    loop {
        let n = class.read_packet(&mut buf).await?;
        debug!("Received {} bytes: {:x}", n, buf[0..n]);
        if n == 10 {
            match TryInto::<SerialMessage>::try_into(buf.as_slice()) {
                Ok(sm) => {
                    info!("Received message: {}", sm);
                    match sm.get_command() {
                        SerialCommand::EndOfStream | SerialCommand::ToBeContinued => {
                            send_message(
                                class,
                                SerialMessage::nack_to_message(&sm, NackType::InvalidCommand),
                            )
                            .await?;
                        }
                        SerialCommand::SyncRequest => todo!(),
                        SerialCommand::DeviceReset => {
                            send_message(class, SerialMessage::ack_to(&sm)).await?;
                            info!("Resetting the device");
                            cortex_m::peripheral::SCB::sys_reset()
                        }
                        SerialCommand::DisableKeyboardInput => {
                            board.lock().await.get_mut().disable_keyboard_input();
                            send_message(class, SerialMessage::ack_to(&sm)).await?;
                        }
                        SerialCommand::EnableKeyboardInput => {
                            board.lock().await.get_mut().enable_keyboard_input();
                            send_message(class, SerialMessage::ack_to(&sm)).await?;
                        }
                        SerialCommand::LockButtonState => {
                            let data = sm.get_data();
                            let led_idx = 0b00001111 & data[0];
                            let to_state = ButtonState::try_from(data[0] >> 7).unwrap();
                            board
                                .lock()
                                .await
                                .get_mut()
                                .lock_led_state(led_idx as usize, &to_state);
                            send_message(class, SerialMessage::ack_to(&sm)).await?;
                        }
                        SerialCommand::LockAllButtonStates => {
                            let data = sm.get_data();
                            let to_state = ButtonState::try_from(data[0] >> 7).unwrap();
                            board.lock().await.get_mut().lock_led_states(&to_state);
                            send_message(class, SerialMessage::ack_to(&sm)).await?;
                        }
                        SerialCommand::UnlockButtonState => {
                            let data = sm.get_data();
                            let led_idx = 0b00001111 & data[0];

                            board
                                .lock()
                                .await
                                .get_mut()
                                .unlock_led_state(led_idx as usize);
                            send_message(class, SerialMessage::ack_to(&sm)).await?;
                        }
                        SerialCommand::UnlockAllButtonStates => {
                            board.lock().await.get_mut().unlock_led_states();
                            send_message(class, SerialMessage::ack_to(&sm)).await?;
                        }
                        SerialCommand::AddState => {
                            let data = sm.get_data();
                            let led_idx = 0b00001111 & data[0];
                            let transition_function = match transition_function_try_from_bytes(data)
                            {
                                Ok(f) => f,
                                Err(e) => {
                                    send_message(class, SerialMessage::nack_from_error(e)).await?;
                                    continue;
                                }
                            };
                            let for_state = ButtonState::try_from(data[0] >> 7).unwrap();
                            let state_idx = data[1] >> 4;
                            info!(
                                "Added state for led {}: for_state: {}, state index: {}",
                                led_idx, for_state, state_idx
                            );
                            board.lock().await.get_mut().add_led_state(
                                led_idx as usize,
                                state_idx as usize,
                                transition_function,
                                &for_state,
                            );
                            send_message(class, SerialMessage::ack_to(&sm)).await?;
                        }
                        SerialCommand::RemoveState => {
                            let data = sm.get_data();
                            let led_idx = 0b00001111 & data[0];
                            let state_idx = data[1] >> 4;
                            let for_state = ButtonState::try_from(data[0] >> 7).unwrap();
                            board.lock().await.get_mut().remove_led_state(
                                led_idx as usize,
                                state_idx as usize,
                                &for_state,
                            );
                            send_message(class, SerialMessage::ack_to(&sm)).await?;
                        }
                        SerialCommand::ClearStates => {
                            let data = sm.get_data();
                            let led_idx = 0b00001111 & data[0];
                            let for_state = ButtonState::try_from(data[0] >> 7).unwrap();
                            board
                                .lock()
                                .await
                                .get_mut()
                                .clear_led_queue(led_idx as usize, &[&for_state]);
                            send_message(class, SerialMessage::ack_to(&sm)).await?;
                        }
                        SerialCommand::NackGeneral => todo!(),
                        SerialCommand::NackInvalidCommand => todo!(),
                        SerialCommand::NackParseError => todo!(),
                        SerialCommand::NackDeviceError => todo!(),
                        SerialCommand::NackDeviceBusy => todo!(),
                        SerialCommand::Reserved => todo!(),
                        SerialCommand::Ping => {
                            send_message(class, SerialMessage::ack_to(&sm)).await?;
                        }
                        SerialCommand::Ack => todo!(),
                    }
                }
                Err(err) => {
                    error!("Failed to parse serial message: {}", err);
                    send_message(class, SerialMessage::nack_from_error(err)).await?
                }
            }
        } else {
            error!("Failed to parse serial message - invalid length {}", n);
            send_message(
                class,
                SerialMessage::nack_from_error(ParseError::InvalidMessageLength),
            )
            .await?
        }
    }
}

async fn send_message<'d, T: Instance + 'd>(
    class: &mut CdcAcmClass<'d, Driver<'d, T>>,
    message: SerialMessage,
) -> Result<(), EndpointError> {
    let bytes = message.to_bytes();
    info!("Sending message: {}", message);
    class.write_packet(&bytes).await
}
