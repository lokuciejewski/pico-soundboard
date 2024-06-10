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
