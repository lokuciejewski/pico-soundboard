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

The protocol is a two-device, synchronous, based on request-response with a fixed-length message. Each message consists of 10 bytes (exact number may change).

#### Message structure

 The message structure is shown below (first byte is the leftmost one):

```none
                    MESSAGE
[COMMAND BYTE] [8 BYTES OF DATA] [END BYTE]
```

Example request:

```none
            0xFD 0x00 0x00 0x00 0x00 0x00 0x00 0x00 0x00 0x00
PING command ^  |            8 bytes of data            | ^ END OF STREAM 
```

And the response:

```none
            0xFE 0xFD 0x00 0x00 0x00 0x00 0x00 0x00 0x00 0x00
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
                                0xF2 0xFD 0x00 0x00 0x00 0x00 0x00 0x00 0x00 0x00   
NACK Communication Error command ^  |         8 bytes of data               | ^ END OF STREAM
```

#### Description of commands
