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
            fade_out(0b11110000, colour.clone(), 500),
            &ButtonState::Idle,
        );
        board.add_led_state(i, solid(0x00, colour.clone(), timeout), &ButtonState::Idle);
        board.add_led_state(
            i,
            fade_in(0b11110000, colour.clone(), 500),
            &ButtonState::Idle,
        );
        board.add_led_state(
            i,
            solid(0b11110000, colour.clone(), timeout),
            &ButtonState::Idle,
        );

        board.add_led_state(i, solid(0xff, colour.invert(), 100), &ButtonState::Held);
        board.add_led_state(i, fade_out(0xff, colour.invert(), 250), &ButtonState::Held);
        board.add_led_state(i, solid(0x00, colour.invert(), 0), &ButtonState::Held);
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
            solid(0x00, colour.clone(), (idx + 1) * speed),
            &ButtonState::Idle,
        );
        board.add_led_state(
            i,
            fade_in(0b11110000, colour.clone(), speed),
            &ButtonState::Idle,
        );
        board.add_led_state(
            i,
            solid(0b11110000, colour.clone(), (12 - idx) * speed),
            &ButtonState::Idle,
        );

        board.add_led_state(
            i,
            solid(0b11110000, colour.clone(), (idx + 1) * speed),
            &ButtonState::Idle,
        );
        board.add_led_state(
            i,
            fade_out(0b11110000, colour.clone(), speed),
            &ButtonState::Idle,
        );
        board.add_led_state(
            i,
            solid(0x0, colour.clone(), (12 - idx) * speed),
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
    board.add_led_state(
        led_index,
        fade_out(0b11110000, colour.clone(), speed),
        state,
    );
    board.add_led_state(led_index, solid(0x00, colour.clone(), speed), state);
    board.add_led_state(led_index, fade_in(0b11110000, colour.clone(), speed), state);
    board.add_led_state(led_index, solid(0b11110000, colour.clone(), speed), state);
}
