extern crate alloc;
use crate::{rgbleds::LedState, Colour};
use alloc::boxed::Box;

/// Transition defines the state in a function of time
// pub type Transition = fn(current_ticks: usize) -> TransitionResult;
pub(crate) type TransitionFunction = Box<dyn Fn(usize) -> TransitionResult>;

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
