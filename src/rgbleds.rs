extern crate alloc;
use alloc::boxed::Box;

use embedded_hal_async::spi::SpiBus;
use heapless::Vec;

use crate::{
    transitions::{TransitionFunction, TransitionResult},
    ButtonState, Colour,
};

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
            l.leds.push(RGBLed::new()).unwrap();
        }
        l
    }

    pub fn full(&mut self, brightness: u8, colour: Colour) {
        self.leds.iter_mut().for_each(|led| {
            led.clear(&[&ButtonState::Idle, &ButtonState::Pressed]);
            led.add_state(
                0,
                Box::new(move |_: usize| {
                    TransitionResult::InProgress(LedState::new(brightness, &colour))
                }),
                &ButtonState::Idle,
            )
        });
    }

    pub fn clear_all(&mut self) {
        self.leds
            .iter_mut()
            .for_each(|led| led.clear(&[&ButtonState::Idle, &ButtonState::Pressed]));
    }

    pub fn clear(&mut self, index: usize, states: &[&ButtonState]) {
        self.leds.get_mut(index % 16).unwrap().clear(states)
    }

    pub fn add_state(
        &mut self,
        i: usize,
        state_idx: usize,
        transition: TransitionFunction,
        for_state: &ButtonState,
    ) {
        self.leds
            .get_mut(i % 16)
            .unwrap()
            .add_state(state_idx, transition, for_state);
    }

    pub fn remove_state(&mut self, i: usize, state_idx: usize, from_state: &ButtonState) {
        self.leds
            .get_mut(i)
            .unwrap()
            .remove_state(state_idx, from_state);
    }

    pub fn set_button_state(&mut self, i: usize, new_state: ButtonState) {
        let led = self.leds.get_mut(i).unwrap();
        // Do not set the state if it is locked
        if led.lock_state.is_none() && new_state != led.button_state {
            led.button_state = new_state;
            match led.button_state {
                ButtonState::Pressed => led.on_pressed.restart(),
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

    pub fn lock_led_state(&mut self, index: usize, state: &ButtonState) {
        self.leds.get_mut(index % 16).unwrap().lock_state(state);
    }

    pub fn unlock_led_state(&mut self, index: usize) {
        self.leds.get_mut(index % 16).unwrap().unlock_state();
    }
}

#[derive(Debug)]
pub(crate) struct RGBLed {
    current_state: LedState,
    button_state: ButtonState,
    on_pressed: LedStateQueue,
    on_idle: LedStateQueue,
    counter: usize,
    lock_state: Option<ButtonState>,
}

impl RGBLed {
    pub fn new() -> Self {
        Self {
            current_state: LedState::default(),
            button_state: ButtonState::Idle,
            on_pressed: LedStateQueue::new(),
            on_idle: LedStateQueue::new(),
            counter: 0usize,
            lock_state: None,
        }
    }

    pub fn run(&mut self) {
        let queue = if let Some(locked_state) = &self.lock_state {
            match locked_state {
                ButtonState::Pressed => &mut self.on_pressed,
                ButtonState::Idle => &mut self.on_idle,
            }
        } else {
            match self.button_state {
                ButtonState::Pressed => &mut self.on_pressed,
                ButtonState::Idle => &mut self.on_idle,
            }
        };

        if let Some(transition) = queue.current() {
            match transition(self.counter) {
                TransitionResult::InProgress(state) => {
                    self.current_state = state;
                    self.counter += 1;
                }
                TransitionResult::Finished(next_state) => {
                    // Transition complete, move to the next state
                    queue.advance(next_state);
                    self.counter = 0;
                }
            }
        }
    }

    pub fn clear(&mut self, from_states: &[&ButtonState]) {
        for state in from_states {
            match state {
                ButtonState::Pressed => self.on_pressed.clear(),
                ButtonState::Idle => self.on_idle.clear(),
            }
        }
    }

    pub fn add_state(
        &mut self,
        state_idx: usize,
        transition: TransitionFunction,
        for_state: &ButtonState,
    ) {
        match for_state {
            ButtonState::Pressed => self.on_pressed.insert(state_idx, transition),
            ButtonState::Idle => self.on_idle.insert(state_idx, transition),
        }
    }

    pub fn remove_state(&mut self, state_idx: usize, from_state: &ButtonState) {
        match from_state {
            ButtonState::Pressed => self.on_pressed.remove(state_idx),
            ButtonState::Idle => self.on_idle.remove(state_idx),
        };
    }

    pub fn lock_state(&mut self, state: &ButtonState) {
        self.lock_state = Some(*state)
    }

    pub fn unlock_state(&mut self) {
        self.lock_state = None
    }
}

const LED_STATE_QUEUE_SIZE: usize = 16;

struct LedStateQueue {
    queue: Vec<TransitionFunction, LED_STATE_QUEUE_SIZE>,
    current_element: usize,
}

impl core::fmt::Debug for LedStateQueue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LedStateQueue")
            .field("current_element", &self.current_element)
            .finish()
    }
}

impl LedStateQueue {
    pub fn new() -> Self {
        let mut q = Self {
            queue: Vec::new(),
            current_element: 0,
        };
        for i in 0..LED_STATE_QUEUE_SIZE {
            q.insert(
                i,
                Box::new(move |_| TransitionResult::Finished((i + 1) % LED_STATE_QUEUE_SIZE)),
            )
        }
        q
    }

    pub fn advance(&mut self, to_element: usize) {
        self.current_element = to_element % LED_STATE_QUEUE_SIZE;
    }

    pub fn current(&self) -> Option<&TransitionFunction> {
        self.queue.get(self.current_element)
    }

    pub fn insert(&mut self, position: usize, f: TransitionFunction) {
        if self.queue.len() <= position {
            self.queue.push(f).ok();
        } else {
            self.queue[position] = f;
        }
    }

    pub fn remove(&mut self, position: usize) {
        self.queue[position] = Box::new(move |_: usize| {
            TransitionResult::Finished((position + 1) % LED_STATE_QUEUE_SIZE)
        });
    }

    pub fn restart(&mut self) {
        self.current_element = 0;
    }

    pub fn clear(&mut self) {
        self.queue = Vec::new();
        self.current_element = 0;
    }
}

#[derive(Clone, Copy, Default, Debug)]
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
