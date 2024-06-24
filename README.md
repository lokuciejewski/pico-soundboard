# Pico Soundboard/Macro Board

## How to build

### Requirements

1. [Rust](https://www.rust-lang.org/tools/install)
2. thumbv6m-none-eabi toolchain - run `rustup target add thumbv6m-none-eabi` after installing Rust
3. [elf2u2f-rs](https://github.com/JoNil/elf2uf2-rs) or [probe-rs](https://github.com/probe-rs/probe-rs) - `cargo install elf2uf2-rs/probe-rs`

### Building

1. Uncomment the correct runner line in [.cargo/config.toml](.cargo/config.toml) (you need `probe-rs` if using pico debug probe or `elf2uf2-rs` otherwise)
2. If using `probe-rs`, make sure both devices are plugged in and powered on
3. If using `elf2uf2-rs`, make sure the Pico is connected to the pc in USB bootloader mode (connect the device while holding BOOTSEL button)
4. Run `cargo run --release`
5. Done

## Device serial protocol

### Summary

The device sends key presses as a USB HID Keyboard. Currently, the buttons on the keyboard are mapped to letters `a` to `p` but in the future they will probably be mapped to something a little less intrusive. Possible mappings:

- (`mod` + `F1`) to (`mod` + `F16`) where `mod` would be a modifier key, e.g. Shift
- Media keys, e.g. `Play`, `Pause` etc. along with weird keys like `Open Email`
- `mod1` + `mod2` (+ `mod3`) + `a` to `p` where `modX` is a modifier key

Sending inputs as a USB Keyboard enables the use of simplified mode where the keypad can act independently of the app. However, the device should be able to receive and reply to commands issued by any application, which will be done via USB Serial Protocol.

### Hardware settings

#### USB Keyboard

For USB HID, the polling rate should be between 50 and 100 ms. The maximum current drawn is configured as 100mA and seems to be sufficient to power the board and all LEDs at the same time.

#### USB Serial Device

TODO

### Protocol proposal

The protocol is a two-device, synchronous, based on request-response with a fixed-length message. Each message consists of 10 bytes.

#### Message structure

 The message structure is shown below (first byte is the leftmost one):

```none
                    MESSAGE
[COMMAND BYTE] [8 BYTES OF DATA] [END BYTE]
```

Example request:

```none
            0xFD 0x00 0x00 0x00 0x00 0x00 0x00 0x00 0x00 0x80
PING command ^  |            8 bytes of data            | ^ END OF STREAM 
```

And the response:

```none
            0xFE 0xFD 0x00 0x00 0x00 0x00 0x00 0x00 0x00 0x80
 ACK command ^  | ^ argument to ACK: PING               | ^ EOS        
```

#### Timing rules

The protocol is mostly timing-independent. The only crucial timings are message timeouts in case the `ACK`/`NACK` is not send.
If the device does not receive a response (`ACK`/`NACK`) in **500ms**, it assumes the message was not read by the other device.

Normal operation timings should be baudrate-dependent, the default being 9600 (to be confirmed).

Any other timeout should also be **500ms** if not specified otherwise.

#### Communication errors

If the device receives less than 10 bytes in default communication timeout - **250ms** - it treats the message as incomplete and responds with the correct type of `NACK` (optionally including the first 7 bytes of message `DATA` section in message's `DATA`).

Example:

```none
REQUEST:
            0xFD 0x00 0x00 0x00 0x00 0x00 0x00 0x00 0x00 
PING command ^  |              8 bytes of data?         |
```

```none
RESPONSE:
                       0xF2 0xFD 0x00 0x00 0x00 0x00 0x00 0x00 0x00 0x80   
NACK ParseError command ^  |         8 bytes of data               | ^ END OF STREAM
```

#### Description of commands

##### `ACK` - Acknowledge command

- Command byte: `0xFF`
- Data bytes: should contain first 8 bytes (command byte and 7 data bytes) of the message being acknowledged
- End byte: [`END OF STREAM`](#end-of-stream)

##### `NACK - General`

- Command byte: `0xF0`
- Data bytes: should contain first 8 bytes (command byte and 7 data bytes) of the message being rejected
- End byte: [`END OF STREAM`](#end-of-stream)

##### `NACK - InvalidCommand`

##### `NACK - ParseError`

- Command byte: `0xF2`
- Data bytes: if the error ocurred **after** parsing the command, the first 8 bytes of the message being rejected are sent, otherwise:
  - Byte 0: [ParseError](#parseerror) indicating why the command could not be parsed
  - Bytes 1-7: ignored
- End byte: [`END OF STREAM`](#end-of-stream)

##### `NACK - DeviceError`

##### `NACK - DeviceBusy`

##### `END OF STREAM`

- Byte: `0x80`

##### `AddState`

Add a LedState (illumination state) to the chosen button when it is in the chosen [ButtonState](#buttonstate)

- Command byte: `0xB0`
- Data bytes:
  - Byte 0: [ButtonState](#buttonstate) (high nibble) and Led Index (low nibble). Example: a value of `0x36` - `0b00110110` in binary is interpreted as `ButtonState::Idle` (0b0011) and Led Index 6 (0b0110).
  - Byte 1: [TransitionFunction](#transitionfunction)
  - Byte 2: Led Brightness - value is masked using `0b11100000`, so valid values are from `0` to `0b00011111`
  - Bytes 3-5: [Colour](#colour) values for Red, Green and Blue respectively
  - Bytes 6-7: Duration of the state in led ticks (currently ms), interpreted MSB first, for example to send a value of decimal `500`, two bytes `0x01` and `0xf4` should be sent (`0x1f4` == `500`). If set to `0x0000`, the state will persist indefinitely.
- End byte: [`END OF STREAM`](#end-of-stream)

Valid responses:

- [ACK](#ack---acknowledge-command)
- [NACK - ParseError](#nack---parseerror)

##### `RemoveState`

TODO

##### `ClearStates`

Clear all states for the chosen button from the chosen [ButtonState](#buttonstate)

- Command byte: `0xB2`
- Data bytes:
  - Byte 0: [ButtonState](#buttonstate) (high nibble) and Led Index (low nibble). Example: a value of `0x36` - `0b00110110` in binary is interpreted as `ButtonState::Idle` (0b0011) and Led Index 6 (0b0110).
  - Bytes 1-7: ignored
- End byte: [`END OF STREAM`](#end-of-stream)

Valid responses:

- [ACK](#ack---acknowledge-command)
- [NACK - ParseError](#nack---parseerror)

#### Translation of enums and struct to bytes

##### SerialCommand

```rust
pub enum SerialCommand {
    EndOfStream = 0x80,
    ToBeContinued = 0x81,
    // Sync commands
    SyncRequest = 0x90,
    // Device related commands
    DeviceReset = 0xa0,
    DisableKeyboardInput,
    EnableKeyboardInput,
    // State related commands
    AddState = 0xb0,
    RemoveState,
    ClearStates,
    // Communication related commands
    // NACK types
    NackGeneral = 0xf0,
    NackInvalidCommand = 0xf1,
    NackParseError = 0xf2,
    NackDeviceError = 0xf3,
    NackDeviceBusy = 0xf4,
    // Reserved until 0xf9
    Reserved = 0xf9,
    Ping = 0xfe,
    Ack = 0xff,
}
```

##### ButtonState

```rust
pub enum ButtonState {
    Pressed = 0x0,
    Held = 0x1,
    Released = 0x2,
    Idle = 0x3,
}
```

##### Colour

```rust
pub struct Colour {
    red: u8,
    green: u8,
    blue: u8,
}
```

When translating from bytes, the bytes are in the order of `red`, `green`, `blue`, unless stated otherwise.

##### ParseError

```rust
pub enum ParseError {
    InvalidCommand = 0x0,
    InvalidData = 0x1,
    InvalidEndByte = 0x2,
    InvalidMessageLength = 0x3,
}
```

##### TransitionFunction

This is currently WIP, however as of now the translation is:

```rust
    solid = 0x0
    fade_out = 0x1
    fade_in = 0x2
```
