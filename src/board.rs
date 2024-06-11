use embedded_hal_async::{i2c::I2c, spi::SpiBus};

use crate::{
    rgbleds::{RGBLeds, TransitionFunction},
    Button, ButtonCode, ButtonState, Colour,
};

pub struct Board<I2C, SPI> {
    i2c: I2C,
    buttons: [Button<SPI>; 16],
    rgb_leds: RGBLeds<SPI>,
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
        rgb_leds.clear();
        rgb_leds.refresh().await;

        Self {
            i2c,
            buttons,
            rgb_leds,
        }
    }

    pub fn add_callback_pressed(
        &mut self,
        button_idx: u8,
        callback: fn(&Button<SPI>, &mut RGBLeds<SPI>) -> (),
    ) {
        if let Some(b) = self
            .buttons
            .iter_mut()
            .find(|b| b.rgb_led_index == button_idx)
        {
            b.callback_pressed = callback;
        };
    }

    pub fn add_callback_released(
        &mut self,
        button_idx: u8,
        callback: fn(&Button<SPI>, &mut RGBLeds<SPI>) -> (),
    ) {
        if let Some(b) = self
            .buttons
            .iter_mut()
            .find(|b| b.rgb_led_index == button_idx)
        {
            b.callback_released = callback;
        };
    }

    pub fn add_led_state(
        &mut self,
        led_idx: usize,
        transition: TransitionFunction,
        for_state: &ButtonState,
    ) {
        self.rgb_leds.add_state(led_idx, transition, for_state);
    }

    pub async fn refresh_leds(&mut self) {
        self.rgb_leds.refresh().await;
    }

    // Return 6 first pressed keys (max supported by `usbd_hid`'s `KeyboardReport`)
    pub async fn update_status(&mut self) -> Result<[u8; 6], &str> {
        let mut i2c_read_buffer = [0u8; 2];
        let temp = [1];
        self.i2c.write(0x20, &temp).await.unwrap();
        self.i2c.read(0x20, &mut i2c_read_buffer).await.unwrap();
        let states = !((i2c_read_buffer[0] as u16) | ((i2c_read_buffer[1] as u16) << 8));

        let mut pressed_buffer = [0u8; 6];
        let mut counter = 0usize;

        for i in 0..16 {
            match ButtonCode::try_from(1 << i) {
                Ok(_btn) => {
                    let pressed_now = ((states >> i) & 0b1) == 0b1;
                    match (pressed_now, self.buttons[i].pressed) {
                        (true, true) => {
                            // Was pressed before and is still pressed
                            pressed_buffer[counter] = self.buttons[i].rgb_led_index + 4;
                            self.rgb_leds
                                .set_button_state(map_idx_from_button_to_led(i), ButtonState::Held);
                            counter += 1;
                        }
                        (true, false) => {
                            // Was not pressed before but is pressed now, call the callback
                            pressed_buffer[counter] = self.buttons[i].rgb_led_index + 4;
                            self.rgb_leds.set_button_state(
                                map_idx_from_button_to_led(i),
                                ButtonState::Pressed,
                            );
                            counter += 1;
                            self.buttons[i].pressed = true;
                            (self.buttons[i].callback_pressed)(&self.buttons[i], &mut self.rgb_leds)
                        }
                        (false, true) => {
                            // Button was pressed but now is released, call the released callback
                            self.rgb_leds.set_button_state(
                                map_idx_from_button_to_led(i),
                                ButtonState::Released,
                            );
                            self.buttons[i].pressed = false;
                            (self.buttons[i].callback_released)(
                                &self.buttons[i],
                                &mut self.rgb_leds,
                            )
                        }
                        (false, false) => {
                            // Was not pressed and is still not pressed now, do nothing
                            self.rgb_leds
                                .set_button_state(map_idx_from_button_to_led(i), ButtonState::Idle);
                        }
                    }
                    // Collect only 6 buttons at once since there is no NKR
                    if counter == 6 {
                        return Ok(pressed_buffer);
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
