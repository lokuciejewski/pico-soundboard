use defmt::info;
use embedded_hal_async::spi::SpiBus;

use heapless::Vec;
extern crate alloc;
use alloc::boxed::Box;

use crate::{ButtonState, Colour};

pub(crate) struct RGBLeds<SPI> {
    spi: SPI,
    _start_frame: [u8; 4],
    leds: Vec<RGBLed, 16>,
    _end_frame: [u8; 4],
}

impl<SPI> RGBLeds<SPI>
where
    SPI: SpiBus,
{
    pub fn new(spi: SPI) -> Self {
        let mut l = Self {
            spi,
            _start_frame: [0; 4],
            leds: Vec::new(),
            _end_frame: [0; 4],
        };
        for _ in 0..16 {
            l.leds.push(RGBLed::new()).ok();
        }
        l
    }

    pub fn full(&mut self, brightness: u8, colour: Colour) {
        self.leds.iter_mut().for_each(|led| {
            led.clear(&[
                ButtonState::Held,
                ButtonState::Idle,
                ButtonState::Pressed,
                ButtonState::Released,
            ]);
            led.add_state(
                Box::new(move |_: usize| {
                    TransitionResult::InProgress(LedState::new(brightness, &colour))
                }),
                &ButtonState::Idle,
            )
        });
    }

    pub fn clear(&mut self) {
        self.leds.iter_mut().for_each(|led| {
            led.clear(&[
                ButtonState::Held,
                ButtonState::Idle,
                ButtonState::Pressed,
                ButtonState::Released,
            ])
        });
    }

    pub fn add_state(&mut self, i: usize, transition: TransitionFunction, for_state: &ButtonState) {
        self.leds
            .get_mut(i % 16)
            .unwrap()
            .add_state(transition, for_state);
    }

    pub fn pop_state(&mut self, i: usize, from_state: &ButtonState) {
        self.leds.get_mut(i).unwrap().pop_state(from_state);
    }

    pub fn set_button_state(&mut self, i: usize, new_state: ButtonState) {
        let led = self.leds.get_mut(i).unwrap();
        if new_state != led.button_state {
            led.button_state = new_state;
            match led.button_state {
                ButtonState::Pressed => led.on_pressed.restart(),
                ButtonState::Held => led.on_held.restart(),
                ButtonState::Released => led.on_released.restart(),
                ButtonState::Idle => led.on_idle.restart(),
            };
            led.counter = 0;
        }
    }

    pub async fn refresh(&mut self) {
        self.spi.write(&self._start_frame).await.unwrap();

        for led in self.leds.iter_mut() {
            led.run();
            self.spi
                .write(&[
                    led.current_state.brightness,
                    led.current_state.b,
                    led.current_state.g,
                    led.current_state.r,
                ])
                .await
                .unwrap()
        }

        self.spi.write(&self._end_frame).await.unwrap();
    }
}

pub(crate) struct RGBLed {
    current_state: LedState,
    button_state: ButtonState,
    on_pressed: LedStateQueue,
    on_held: LedStateQueue,
    on_released: LedStateQueue,
    on_idle: LedStateQueue,
    counter: usize,
}

impl RGBLed {
    pub fn new() -> Self {
        Self {
            current_state: LedState::default(),
            button_state: ButtonState::Idle,
            on_pressed: LedStateQueue::new(),
            on_held: LedStateQueue::new(),
            on_released: LedStateQueue::new(),
            on_idle: LedStateQueue::new(),
            counter: 0usize,
        }
    }

    pub fn run(&mut self) {
        let queue = match self.button_state {
            ButtonState::Pressed => &mut self.on_pressed,
            ButtonState::Held => &mut self.on_held,
            ButtonState::Released => &mut self.on_released,
            ButtonState::Idle => &mut self.on_idle,
        };
        if let Some(transition) = queue.current() {
            match transition(self.counter) {
                TransitionResult::InProgress(state) => {
                    self.current_state = state;
                    self.counter += 1;
                }
                TransitionResult::Finished => {
                    // Transition complete, move to the next state
                    queue.advance();
                    self.counter = 0;
                }
            }
        }
    }

    pub fn clear(&mut self, from_states: &[ButtonState]) {
        for state in from_states {
            match state {
                ButtonState::Pressed => self.on_pressed.clear(),
                ButtonState::Held => self.on_held.clear(),
                ButtonState::Released => self.on_released.clear(),
                ButtonState::Idle => self.on_idle.clear(),
            }
        }
    }

    /// Add new state at the end of state queue. If full, will replace the last used state
    pub fn add_state(&mut self, transition: TransitionFunction, for_state: &ButtonState) {
        match for_state {
            ButtonState::Pressed => self.on_pressed.push(transition),
            ButtonState::Held => self.on_held.push(transition),
            ButtonState::Released => self.on_released.push(transition),
            ButtonState::Idle => self.on_idle.push(transition),
        }
    }

    /// Pop the current state
    pub fn pop_state(&mut self, from_state: &ButtonState) {
        match from_state {
            ButtonState::Pressed => self.on_pressed.pop(),
            ButtonState::Held => self.on_held.pop(),
            ButtonState::Released => self.on_released.pop(),
            ButtonState::Idle => self.on_idle.pop(),
        };
    }
}

struct LedStateQueue {
    queue: Vec<TransitionFunction, 8>,
    current_element: usize,
}

impl LedStateQueue {
    pub fn new() -> Self {
        let mut q = Self {
            queue: Vec::new(),
            current_element: 0,
        };
        q.push(Box::new(|_: usize| TransitionResult::Finished));
        q
    }

    pub fn advance(&mut self) {
        if !self.queue.is_empty() {
            self.current_element = (self.current_element + 1) % self.queue.len();
        }
    }

    pub fn current(&self) -> Option<&TransitionFunction> {
        self.queue.get(self.current_element)
    }

    pub fn push(&mut self, transition: TransitionFunction) {
        match self.queue.push(transition) {
            Ok(_) => (),
            Err(elem) => {
                let cur_len = self.len();
                self.queue[(self.current_element - 1) % cur_len] = elem;
            }
        }
    }

    pub fn pop(&mut self) -> Option<TransitionFunction> {
        if let Some(x) = self.queue.pop() {
            self.current_element = (self.current_element - 1) % self.len();
            Some(x)
        } else {
            None
        }
    }

    pub fn restart(&mut self) {
        self.current_element = 0;
    }

    pub fn insert(
        &mut self,
        index: usize,
        transition: TransitionFunction,
    ) -> Result<(), TransitionFunction> {
        self.queue.insert(index, transition)
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn clear(&mut self) {
        self.queue = Vec::new();
        self.current_element = 0;
    }
}

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

#[derive(Clone, Copy, Default)]
pub struct LedState {
    pub brightness: u8,
    pub b: u8,
    pub g: u8,
    pub r: u8,
}

impl LedState {
    pub fn new(brightness: u8, colour: &Colour) -> Self {
        Self {
            brightness: brightness | 0b11100000,
            b: colour.blue,
            g: colour.green,
            r: colour.red,
        }
    }
}
