#![no_std]

use embedded_hal_1::spi::SpiBus;
use embedded_hal_async::i2c::I2c;

pub struct RGBLeds<SPI> {
    spi: SPI,
    _start_frame: [u8; 4],
    leds: [RGBLed; 16],
    _end_frame: [u8; 4],
}

impl<SPI> RGBLeds<SPI>
where
    SPI: SpiBus,
{
    pub fn new(spi: SPI) -> Self {
        Self {
            spi,
            _start_frame: [0; 4],
            leds: [RGBLed::new(); 16],
            _end_frame: [0x0; 4],
        }
    }

    pub fn full(&mut self, colour: &Colour) {
        self.leds.iter_mut().for_each(|led| {
            led.set_colour(colour);
            led.set_brightness(0xff);
        });
    }

    pub fn gradient(&mut self, colour: &Colour) {
        self.leds.iter_mut().enumerate().for_each(|(idx, led)| {
            led.set_colour(colour);
            led.set_brightness(0b11100000 | ((idx * 2) as u8));
        });
    }

    pub fn clear(&mut self) {
        self.full(&Colour {
            red: 0x0,
            green: 0x0,
            blue: 0x0,
        })
    }

    pub fn set_xy(&mut self, x: u8, y: u8, colour: &Colour) {
        self.set_idx(x + y, colour);
    }

    pub fn set_idx(&mut self, i: u8, colour: &Colour) {
        self.leds[(i % 16) as usize].set_colour(colour);
    }

    pub fn refresh(&mut self) {
        self.spi.write(&self._start_frame).unwrap();

        self.leds.iter().for_each(|led| {
            self.spi
                .write(&[led.brightness, led.b, led.g, led.r])
                .unwrap()
        });

        self.spi.write(&self._end_frame).unwrap();
    }
}

#[repr(packed)]
#[derive(Clone, Copy)]
struct RGBLed {
    brightness: u8,
    b: u8,
    g: u8,
    r: u8,
}

impl RGBLed {
    pub fn new() -> Self {
        Self {
            brightness: 0x0,
            r: 0x0,
            g: 0x0,
            b: 0x0,
        }
    }
}

impl RGBLed {
    pub fn set_colour(&mut self, colour: &Colour) {
        self.r = colour.red;
        self.g = colour.green;
        self.b = colour.blue;
    }

    pub fn set_brightness(&mut self, brightness: u8) {
        self.brightness = brightness;
    }
}

pub struct Board<I2C, SPI> {
    i2c: I2C,
    buttons: [Button<SPI>; 16],
    pub rgb_leds: RGBLeds<SPI>,
}

impl<I2C: I2c, SPI: SpiBus> Board<I2C, SPI> {
    pub fn new(i2c: I2C, spi: SPI) -> Self {
        let buttons = core::array::from_fn(|i| Button::new(ButtonCode::try_from(1 << i).unwrap()));
        let mut rgb_leds = RGBLeds::new(spi);
        // Needed for initialisation
        rgb_leds.full(&Colour::white());
        rgb_leds.refresh();
        rgb_leds.clear();
        rgb_leds.refresh();

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
                    if pressed_now && self.buttons[i].pressed {
                        // Was pressed before and is still pressed
                        pressed_buffer[counter] = self.buttons[i].rgb_led_index + 4;
                        counter += 1;
                    } else if pressed_now {
                        // Was not pressed before but is pressed now, call the callback
                        self.buttons[i].pressed = true;
                        pressed_buffer[counter] = self.buttons[i].rgb_led_index + 4;
                        counter += 1;
                        (self.buttons[i].callback_pressed)(&self.buttons[i], &mut self.rgb_leds)
                    } else if self.buttons[i].pressed {
                        // Button was pressed but now is released, call the released callback
                        self.buttons[i].pressed = false;
                        (self.buttons[i].callback_released)(&self.buttons[i], &mut self.rgb_leds)
                    } else {
                        // Was not pressed and is still not pressed now, do nothing
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

#[derive(Clone, Copy)]
pub struct Button<SPI> {
    code: ButtonCode,
    pub rgb_led_index: u8,
    pressed: bool,
    callback_pressed: fn(&Button<SPI>, &mut RGBLeds<SPI>) -> (),
    callback_released: fn(&Button<SPI>, &mut RGBLeds<SPI>) -> (),
}

impl<SPI> Button<SPI> {
    pub fn new(code: ButtonCode) -> Self {
        Self {
            code,
            rgb_led_index: code.to_index(),
            pressed: false,
            callback_pressed: |_button, _leds| (),
            callback_released: |_button, _leds| (),
        }
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonCode {
    _8 = 0x1,
    _9 = 0x2,
    _A = 0x4,
    _B = 0x8,
    _C = 0x10,
    _D = 0x20,
    _E = 0x40,
    _F = 0x80,
    _0 = 0x100,
    _1 = 0x200,
    _2 = 0x400,
    _3 = 0x800,
    _4 = 0x1000,
    _5 = 0x2000,
    _6 = 0x4000,
    _7 = 0x8000,
}

impl ButtonCode {
    pub fn to_index(&self) -> u8 {
        match self {
            ButtonCode::_8 => 8,
            ButtonCode::_9 => 9,
            ButtonCode::_A => 10,
            ButtonCode::_B => 11,
            ButtonCode::_C => 12,
            ButtonCode::_D => 13,
            ButtonCode::_E => 14,
            ButtonCode::_F => 15,
            ButtonCode::_0 => 0,
            ButtonCode::_1 => 1,
            ButtonCode::_2 => 2,
            ButtonCode::_3 => 3,
            ButtonCode::_4 => 4,
            ButtonCode::_5 => 5,
            ButtonCode::_6 => 6,
            ButtonCode::_7 => 7,
        }
    }
}

impl TryFrom<u32> for ButtonCode {
    type Error = ();
    fn try_from(value: u32) -> Result<Self, ()> {
        match value {
            0x1 => Ok(ButtonCode::_8),
            0x2 => Ok(ButtonCode::_9),
            0x4 => Ok(ButtonCode::_A),
            0x8 => Ok(ButtonCode::_B),
            0x10 => Ok(ButtonCode::_C),
            0x20 => Ok(ButtonCode::_D),
            0x40 => Ok(ButtonCode::_E),
            0x80 => Ok(ButtonCode::_F),
            0x100 => Ok(ButtonCode::_0),
            0x200 => Ok(ButtonCode::_1),
            0x400 => Ok(ButtonCode::_2),
            0x800 => Ok(ButtonCode::_3),
            0x1000 => Ok(ButtonCode::_4),
            0x2000 => Ok(ButtonCode::_5),
            0x4000 => Ok(ButtonCode::_6),
            0x8000 => Ok(ButtonCode::_7),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy)]
pub enum Illumination {
    Steady(Colour),
    Blinking(Colour, u16),
    Rainbow(u16),
}

#[derive(Clone, Copy)]
pub struct Colour {
    red: u8,
    green: u8,
    blue: u8,
}

impl Colour {
    pub fn white() -> Colour {
        Colour {
            red: 0xff,
            green: 0xff,
            blue: 0xff,
        }
    }

    pub fn rgb(red: u8, green: u8, blue: u8) -> Colour {
        Colour { red, green, blue }
    }
}
