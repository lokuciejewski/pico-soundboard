extern crate alloc;
use crate::{rgbleds::LedState, serial_protocol::ParseError, Colour};
use alloc::boxed::Box;

/// Transition defines the state in a function of time
// pub type Transition = fn(current_ticks: usize) -> TransitionResult;
pub(crate) type TransitionFunction = Box<dyn Fn(usize) -> TransitionResult>;

pub fn transition_function_try_from_bytes(
    bytes: &[u8; 8],
) -> Result<TransitionFunction, ParseError> {
    let function = match (bytes[0] >> 4) & 0b0111 {
        0 => solid,
        1 => fade_out,
        2 => fade_in,
        _ => return Err(ParseError::InvalidData),
    };

    let next_state = bytes[1] & 0b00001111;
    let brightness = bytes[2];
    let colour = Colour::rgb(bytes[3], bytes[4], bytes[5]);
    let duration_ticks = (bytes[6] as usize) << 8 | bytes[7] as usize;
    Ok(function(
        brightness,
        colour,
        duration_ticks,
        next_state as usize,
    ))
}

pub fn solid(
    brightness: u8,
    colour: Colour,
    duration_ticks: usize,
    transition_index: TransitionIndex,
) -> TransitionFunction {
    if duration_ticks != 0 {
        Box::new(move |counter: usize| {
            if counter < duration_ticks {
                TransitionResult::InProgress(LedState::new(brightness, &colour))
            } else {
                TransitionResult::Finished(transition_index)
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
    transition_index: TransitionIndex,
) -> TransitionFunction {
    Box::new(move |counter: usize| {
        if counter < duration_ticks {
            TransitionResult::InProgress(LedState::new(
                (initial_brightness & 0b00011111)
                    - (counter * (initial_brightness & 0b00011111) as usize / duration_ticks) as u8,
                &colour,
            ))
        } else {
            TransitionResult::Finished(transition_index)
        }
    })
}

pub fn fade_in(
    target_brightness: u8,
    colour: Colour,
    duration_ticks: usize,
    transition_index: TransitionIndex,
) -> TransitionFunction {
    Box::new(move |counter: usize| {
        if counter < duration_ticks {
            TransitionResult::InProgress(LedState::new(
                (counter * (target_brightness & 0b00011111) as usize / duration_ticks) as u8,
                &colour,
            ))
        } else {
            TransitionResult::Finished(transition_index)
        }
    })
}

pub type TransitionIndex = usize;

pub enum TransitionResult {
    InProgress(LedState),
    Finished(TransitionIndex),
}
