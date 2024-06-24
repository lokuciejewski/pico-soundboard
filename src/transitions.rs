extern crate alloc;
use crate::{rgbleds::LedState, serial_protocol::ParseError, Colour};
use alloc::boxed::Box;

/// Transition defines the state in a function of time
// pub type Transition = fn(current_ticks: usize) -> TransitionResult;
pub(crate) type TransitionFunction = Box<dyn Fn(usize) -> TransitionResult>;

pub fn transition_function_try_from_bytes(
    bytes: &[u8; 7],
) -> Result<TransitionFunction, ParseError> {
    // Function[0] Brightness[1] Colour[2, 3, 4] Duration[5, 6]
    let function = match bytes[0] {
        0 => solid,
        1 => fade_out,
        2 => fade_in,
        _ => return Err(ParseError::InvalidData),
    };

    let brightness = bytes[1];
    let colour = Colour::rgb(bytes[2], bytes[3], bytes[4]);
    let duration_ticks = (bytes[5] as usize) << 8 | bytes[6] as usize;
    Ok(function(brightness, colour, duration_ticks))
}

pub fn solid(brightness: u8, colour: Colour, duration_ticks: usize) -> TransitionFunction {
    if duration_ticks != 0 {
        Box::new(move |counter: usize| {
            if counter < duration_ticks {
                TransitionResult::InProgress(LedState::new(brightness, &colour))
            } else {
                TransitionResult::Finished
            }
        })
    } else {
        Box::new(move |_: usize| TransitionResult::InProgress(LedState::new(brightness, &colour)))
    }
}

pub fn fade_out(
    initial_brightness: u8,
    colour: Colour,
    duration_ticks: usize,
) -> TransitionFunction {
    Box::new(move |counter: usize| {
        if counter < duration_ticks {
            TransitionResult::InProgress(LedState::new(
                initial_brightness
                    - (counter * (initial_brightness & 0b00011111) as usize / duration_ticks) as u8,
                &colour,
            ))
        } else {
            TransitionResult::Finished
        }
    })
}

pub fn fade_in(target_brightness: u8, colour: Colour, duration_ticks: usize) -> TransitionFunction {
    Box::new(move |counter: usize| {
        if counter < duration_ticks {
            TransitionResult::InProgress(LedState::new(
                (counter * (target_brightness & 0b00011111) as usize / duration_ticks) as u8,
                &colour,
            ))
        } else {
            TransitionResult::Finished
        }
    })
}

pub enum TransitionResult {
    InProgress(LedState),
    Finished,
}
