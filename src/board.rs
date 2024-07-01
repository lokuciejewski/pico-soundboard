extern crate alloc;

use alloc::boxed::Box;
use embedded_hal_async::{i2c::I2c, spi::SpiBus};
use heapless::Vec;

use crate::{
    rgbleds::RGBLeds, transitions::TransitionFunction, Button, ButtonCode, ButtonState, Colour,
};

type ButtonCallback<I2C, SPI> = Option<Box<dyn Fn(&mut Board<I2C, SPI>) -> ButtonCallbackResult>>;

pub enum ButtonCallbackResult {
    Remove,
    Keep,
}

pub struct Board<I2C, SPI> {
    i2c: I2C,
    buttons: [Button; 16],
    callbacks_pressed: Vec<ButtonCallback<I2C, SPI>, 16>,
    callbacks_released: Vec<ButtonCallback<I2C, SPI>, 16>,
    rgb_leds: RGBLeds<SPI>,
    keyboard_input_enabled: bool,
}

impl<I2C: I2c, SPI: SpiBus> Board<I2C, SPI> {
    pub async fn new(i2c: I2C, spi: SPI) -> Self {
        let buttons = core::array::from_fn(|i| Button::new(ButtonCode::try_from(1 << i).unwrap()));
        let mut rgb_leds = RGBLeds::new(spi);
        // Needed for initialisation
        rgb_leds.full(0xff, Colour::white());
        rgb_leds.refresh().await;
        rgb_leds.full(0xff, Colour::rgb(0x0, 0x0, 0x0));
        rgb_leds.refresh().await;
        rgb_leds.clear_all();
        rgb_leds.refresh().await;

        let mut callbacks_pressed: Vec<ButtonCallback<I2C, SPI>, 16> = Vec::new();
        let mut callbacks_released: Vec<ButtonCallback<I2C, SPI>, 16> = Vec::new();

        (0..16).for_each(|_| {
            let _ = callbacks_pressed.push(None);
            let _ = callbacks_released.push(None);
        });

        Self {
            i2c,
            buttons,
            rgb_leds,
            callbacks_pressed,
            callbacks_released,
            keyboard_input_enabled: false,
        }
    }

    pub fn add_callback_pressed(&mut self, button_idx: usize, callback: ButtonCallback<I2C, SPI>) {
        self.callbacks_pressed[map_idx_from_button_to_led(button_idx)] = callback;
    }

    pub fn remove_callback_pressed(&mut self, button_idx: usize) {
        self.callbacks_pressed[map_idx_from_button_to_led(button_idx)] = None;
    }

    pub fn add_callback_released(&mut self, button_idx: usize, callback: ButtonCallback<I2C, SPI>) {
        self.callbacks_released[map_idx_from_button_to_led(button_idx)] = callback;
    }

    pub fn remove_callback_released(&mut self, button_idx: usize) {
        self.callbacks_released[map_idx_from_button_to_led(button_idx)] = None;
    }

    pub fn disable_keyboard_input(&mut self) {
        self.keyboard_input_enabled = false;
    }

    pub fn enable_keyboard_input(&mut self) {
        self.keyboard_input_enabled = true;
    }

    pub fn add_led_state(
        &mut self,
        led_idx: usize,
        state_idx: usize,
        transition: TransitionFunction,
        for_state: &ButtonState,
    ) {
        self.rgb_leds
            .add_state(led_idx, state_idx, transition, for_state);
    }

    pub fn remove_led_state(&mut self, led_idx: usize, state_idx: usize, for_state: &ButtonState) {
        self.rgb_leds.remove_state(led_idx, state_idx, for_state);
    }

    pub async fn refresh_leds(&mut self) {
        self.rgb_leds.refresh().await;
    }

    pub fn lock_led_states(&mut self, state: &ButtonState) {
        for i in 0..16 {
            self.rgb_leds.lock_led_state(i, state);
        }
    }

    pub fn lock_led_state(&mut self, led_idx: usize, state: &ButtonState) {
        self.rgb_leds.lock_led_state(led_idx, state);
    }

    pub fn unlock_led_states(&mut self) {
        for i in 0..16 {
            self.rgb_leds.unlock_led_state(i);
        }
    }

    pub fn unlock_led_state(&mut self, led_idx: usize) {
        self.rgb_leds.unlock_led_state(led_idx);
    }

    pub fn clear_led_queues(&mut self, index: usize) {
        self.rgb_leds
            .clear(index, &[&ButtonState::Idle, &ButtonState::Pressed]);
    }

    pub fn clear_led_queue(&mut self, index: usize, states: &[&ButtonState]) {
        self.rgb_leds.clear(index, states);
    }

    // Return 6 first pressed keys (max supported by `usbd_hid`'s `KeyboardReport`)
    pub async fn update_status(&mut self) -> Result<[u8; 16], &str> {
        let mut i2c_read_buffer = [0u8; 2];
        let temp = [1];
        self.i2c.write(0x20, &temp).await.unwrap();
        self.i2c.read(0x20, &mut i2c_read_buffer).await.unwrap();
        let states = !((i2c_read_buffer[0] as u16) | ((i2c_read_buffer[1] as u16) << 8));

        let mut pressed_buffer = [0u8; 16];

        for i in 0..16 {
            match ButtonCode::try_from(1 << i) {
                Ok(_btn) => {
                    let pressed_now = ((states >> i) & 0b1) == 0b1;
                    match (pressed_now, self.buttons[i].pressed) {
                        (true, true) => {
                            // Was pressed before and is still pressed
                            if self.keyboard_input_enabled {
                                pressed_buffer[i] =
                                    map_led_idx_to_key_code(self.buttons[i].rgb_led_index);
                            }
                            self.rgb_leds.set_button_state(
                                map_idx_from_button_to_led(i),
                                ButtonState::Pressed,
                            );
                        }
                        (true, false) => {
                            // Was not pressed before but is pressed now, call the callback
                            if self.keyboard_input_enabled {
                                pressed_buffer[i] =
                                    map_led_idx_to_key_code(self.buttons[i].rgb_led_index);
                            }
                            self.rgb_leds.set_button_state(
                                map_idx_from_button_to_led(i),
                                ButtonState::Pressed,
                            );
                            self.buttons[i].pressed = true;
                            let callback = self.callbacks_pressed.get_mut(i).unwrap().take();
                            if callback.is_some() {
                                let cb = callback.unwrap();
                                match cb(self) {
                                    ButtonCallbackResult::Remove => {}
                                    ButtonCallbackResult::Keep => {
                                        self.callbacks_pressed[i] = Some(cb);
                                    }
                                }
                            }
                        }
                        (false, true) => {
                            // Button was pressed but now is released, call the released callback
                            self.rgb_leds
                                .set_button_state(map_idx_from_button_to_led(i), ButtonState::Idle);
                            self.buttons[i].pressed = false;
                            let callback = self.callbacks_released.get_mut(i).unwrap().take();
                            if callback.is_some() {
                                let cb = callback.unwrap();
                                match cb(self) {
                                    ButtonCallbackResult::Remove => {}
                                    ButtonCallbackResult::Keep => {
                                        self.callbacks_released[i] = Some(cb);
                                    }
                                }
                            }
                        }
                        (false, false) => {
                            // Was not pressed and is still not pressed now, do nothing
                            self.rgb_leds
                                .set_button_state(map_idx_from_button_to_led(i), ButtonState::Idle);
                        }
                    }
                }
                Err(_) => {
                    return Err("Invalid value when parsing");
                }
            }
        }
        Ok(pressed_buffer)
    }
}

fn map_idx_from_button_to_led(button_idx: usize) -> usize {
    (button_idx + 8) % 16
}

fn map_led_idx_to_key_code(led_idx: u8) -> u8 {
    led_idx + 4
}
