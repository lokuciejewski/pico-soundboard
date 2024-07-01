use embedded_hal_async::{i2c::I2c, spi::SpiBus};
use rand::{rngs::SmallRng, RngCore};

use crate::{
    board::Board,
    transitions::{fade_in, fade_out, solid},
    ButtonState, Colour,
};

pub fn random_fades<I2C: I2c, SPI: SpiBus>(board: &mut Board<I2C, SPI>, small_rng: &mut SmallRng) {
    for i in 0..16 {
        let timeout = small_rng.next_u32() as u16 as usize / 10;
        let colour = Colour::random(small_rng);
        board.add_led_state(
            i,
            0,
            fade_out(0b11110000, colour, 500, 1),
            &ButtonState::Idle,
        );
        board.add_led_state(i, 1, solid(0x00, colour, timeout, 2), &ButtonState::Idle);
        board.add_led_state(
            i,
            2,
            fade_in(0b11110000, colour, 500, 3),
            &ButtonState::Idle,
        );
        board.add_led_state(
            i,
            3,
            solid(0b11110000, colour, timeout, 0),
            &ButtonState::Idle,
        );

        board.add_led_state(
            i,
            0,
            solid(0xff, colour.invert(), 100, 1),
            &ButtonState::Pressed,
        );
        board.add_led_state(
            i,
            1,
            fade_out(0xff, colour.invert(), 250, 2),
            &ButtonState::Pressed,
        );
        board.add_led_state(
            i,
            2,
            solid(0x00, colour.invert(), 0, 0),
            &ButtonState::Pressed,
        );
    }
}

pub fn loading_circle<I2C: I2c, SPI: SpiBus>(
    board: &mut Board<I2C, SPI>,
    colour: Colour,
    speed: usize,
) {
    for (idx, i) in [0, 1, 2, 3, 7, 11, 15, 14, 13, 12, 8, 4]
        .into_iter()
        .enumerate()
    {
        board.add_led_state(
            i,
            0,
            solid(0x00, colour, (idx + 1) * speed, 1),
            &ButtonState::Idle,
        );
        board.add_led_state(
            i,
            1,
            fade_in(0b11110000, colour, speed, 2),
            &ButtonState::Idle,
        );
        board.add_led_state(
            i,
            2,
            solid(0b11110000, colour, (12 - idx) * speed, 3),
            &ButtonState::Idle,
        );

        board.add_led_state(
            i,
            3,
            solid(0b11110000, colour, (idx + 1) * speed, 4),
            &ButtonState::Idle,
        );
        board.add_led_state(
            i,
            4,
            fade_out(0b11110000, colour, speed, 5),
            &ButtonState::Idle,
        );
        board.add_led_state(
            i,
            5,
            solid(0x0, colour, (12 - idx) * speed, 0),
            &ButtonState::Idle,
        );
    }
}

pub fn breathing<I2C: I2c, SPI: SpiBus>(
    board: &mut Board<I2C, SPI>,
    led_index: usize,
    state: &ButtonState,
    colour: Colour,
    speed: usize,
) {
    board.add_led_state(led_index, 0, fade_out(0b11110000, colour, speed, 1), state);
    board.add_led_state(led_index, 1, solid(0x00, colour, speed, 2), state);
    board.add_led_state(led_index, 2, fade_in(0b11110000, colour, speed, 3), state);
    board.add_led_state(led_index, 3, solid(0b11110000, colour, speed, 0), state);
}
