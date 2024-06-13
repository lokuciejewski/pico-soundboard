#![no_std]

use defmt::Format;
use rand::RngCore;

pub mod animations;
pub mod board;
pub mod rgbleds;
pub mod transitions;
pub mod usb_keyboard;

#[derive(Clone)]
pub struct Button {
    _code: ButtonCode,
    pub rgb_led_index: u8,
    pressed: bool,
}

impl Button {
    pub fn new(code: ButtonCode) -> Self {
        Self {
            _code: code,
            rgb_led_index: code.to_index(),
            pressed: false,
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

    pub fn random(rng: &mut dyn RngCore) -> Self {
        Colour {
            red: rng.next_u32() as u8,
            green: rng.next_u32() as u8,
            blue: rng.next_u32() as u8,
        }
    }

    pub fn invert(&self) -> Colour {
        Colour::rgb(!self.red, !self.green, !self.blue)
    }
}

#[derive(Format, PartialEq, Clone)]
pub enum ButtonState {
    Pressed,
    Held,
    Released,
    Idle,
}
