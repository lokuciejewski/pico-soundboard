use embedded_hal_async::spi::SpiBus;

use heapless::Vec;
extern crate alloc;
use alloc::boxed::Box;

use crate::Colour;

pub struct RGBLeds<SPI> {
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
            led.state_queue.clear();
            led.add_state(Box::new(move |_: usize| {
                TransitionResult::InProgress(LedState::new(brightness, &colour))
            }))
        });
    }

    pub fn clear(&mut self) {
        self.leds.iter_mut().for_each(|led| led.state_queue.clear());
    }

    pub fn add_state(&mut self, i: u8, transition: Transition) {
        self.leds
            .get_mut((i % 16) as usize)
            .unwrap()
            .add_state(transition);
    }

    pub fn pop_state(&mut self, i: u8) {
        self.leds.get_mut(i as usize).unwrap().pop_state();
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

struct RGBLed {
    current_state: LedState,
    state_queue: LedStateQueue,
    counter: usize,
}

impl RGBLed {
    pub fn new() -> Self {
        Self {
            current_state: LedState::default(),
            state_queue: LedStateQueue::new(),
            counter: 0usize,
        }
    }

    pub fn run(&mut self) {
        if let Some(transition) = self.state_queue.current() {
            match transition(self.counter) {
                TransitionResult::InProgress(state) => {
                    self.current_state = state;
                    self.counter += 1;
                }
                TransitionResult::Finished => {
                    // Transition complete, move to the next state
                    self.state_queue.advance();
                    self.counter = 0;
                }
            }
        }
    }

    /// Add new state at the end of state queue. If full, will replace the last used state
    pub fn add_state(&mut self, transition: Transition) {
        self.state_queue.push(transition)
    }

    /// Pop the current state
    pub fn pop_state(&mut self) {
        self.state_queue.pop();
    }
}

struct LedStateQueue {
    queue: Vec<Transition, 16>,
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
        self.current_element = (self.current_element + 1) % self.queue.len();
    }

    pub fn current(&self) -> Option<&Transition> {
        self.queue.get(self.current_element)
    }

    pub fn push(&mut self, transition: Transition) {
        match self.queue.push(transition) {
            Ok(_) => (),
            Err(elem) => {
                let cur_len = self.len();
                self.queue[(self.current_element - 1) % cur_len] = elem;
            }
        }
    }

    pub fn pop(&mut self) -> Option<Transition> {
        if let Some(x) = self.queue.pop() {
            self.current_element = (self.current_element - 1) % self.len();
            Some(x)
        } else {
            None
        }
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
pub type Transition = Box<dyn Fn(usize) -> TransitionResult>;

pub fn solid(brightness: u8, colour: Colour, duration_ticks: usize) -> Transition {
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

pub fn fade_out(initial_brightness: u8, colour: Colour, duration_ticks: usize) -> Transition {
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

pub fn fade_in(target_brightness: u8, colour: Colour, duration_ticks: usize) -> Transition {
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
