use defmt::Format;

#[derive(Format)]
pub struct SerialMessage {
    command: SerialCommand,
    data: [u8; 8],
    end_byte: SerialCommand,
}

impl SerialMessage {
    pub fn new(command: SerialCommand, data: [u8; 8], end_byte: SerialCommand) -> Self {
        SerialMessage {
            command,
            data,
            end_byte,
        }
    }

    pub fn ack_to(other: &SerialMessage) -> Self {
        SerialMessage {
            command: SerialCommand::Ack,
            data: [
                other.command as u8,
                other.data[0],
                other.data[1],
                other.data[2],
                other.data[3],
                other.data[4],
                other.data[5],
                other.data[6],
            ],
            end_byte: SerialCommand::EndOfStream,
        }
    }

    pub fn nack_to_message(other: &SerialMessage, nack_type: NackType) -> Self {
        let command = match nack_type {
            NackType::General => SerialCommand::NackGeneral,
            NackType::InvalidCommand => SerialCommand::NackInvalidCommand,
            NackType::NackParseError => SerialCommand::NackParseError,
            NackType::DeviceError => SerialCommand::NackDeviceError,
            NackType::DeviceBusy => SerialCommand::NackDeviceBusy,
        };
        SerialMessage {
            command,
            data: [
                other.command as u8,
                other.data[0],
                other.data[1],
                other.data[2],
                other.data[3],
                other.data[4],
                other.data[5],
                other.data[6],
            ],
            end_byte: SerialCommand::EndOfStream,
        }
    }

    pub fn nack_from_error(parse_error: ParseError) -> Self {
        let command = SerialCommand::NackParseError;
        let data = [parse_error as u8, 0, 0, 0, 0, 0, 0, 0];
        let end_byte = SerialCommand::EndOfStream;

        SerialMessage {
            command,
            data,
            end_byte,
        }
    }

    pub fn to_bytes(&self) -> [u8; 10] {
        [
            self.command as u8,
            self.data[0],
            self.data[1],
            self.data[2],
            self.data[3],
            self.data[4],
            self.data[5],
            self.data[6],
            self.data[7],
            self.end_byte as u8,
        ]
    }

    pub fn get_command(&self) -> &SerialCommand {
        &self.command
    }

    pub fn get_data(&self) -> &[u8; 8] {
        &self.data
    }

    pub fn get_end_byte(&self) -> &SerialCommand {
        &self.end_byte
    }
}

#[derive(Format)]
#[repr(u8)]
pub enum NackType {
    General,
    InvalidCommand,
    NackParseError,
    DeviceError,
    DeviceBusy,
}

impl TryFrom<&[u8]> for SerialMessage {
    type Error = ParseError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != 10 {
            return Err(ParseError::InvalidMessageLength);
        }
        let command = match SerialCommand::try_from(value[0]) {
            Ok(command) => command,
            Err(_) => return Err(ParseError::InvalidCommand),
        };
        let end_byte = match SerialCommand::try_from(value[9]) {
            Ok(command) => command,
            Err(_) => return Err(ParseError::InvalidEndByte),
        };
        Ok(SerialMessage {
            command,
            data: value[1..9]
                .try_into()
                .map_err(|_| ParseError::InvalidData)?,
            end_byte,
        })
    }
}

#[derive(Format, Debug)]
pub enum ParseError {
    InvalidCommand,
    InvalidData,
    InvalidEndByte,
    InvalidMessageLength,
}

#[derive(Format, Clone, Copy)]
#[repr(u8)]
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

impl TryFrom<u8> for SerialCommand {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x80 => Ok(SerialCommand::EndOfStream),
            0x81 => Ok(SerialCommand::ToBeContinued),
            0x90 => Ok(SerialCommand::SyncRequest),
            0xa0 => Ok(SerialCommand::DeviceReset),
            0xa1 => Ok(SerialCommand::DisableKeyboardInput),
            0xa2 => Ok(SerialCommand::EnableKeyboardInput),
            0xb0 => Ok(SerialCommand::AddState),
            0xb1 => Ok(SerialCommand::RemoveState),
            0xb2 => Ok(SerialCommand::ClearStates),
            0xf0 => Ok(SerialCommand::NackGeneral),
            0xf1 => Ok(SerialCommand::NackInvalidCommand),
            0xf2 => Ok(SerialCommand::NackParseError),
            0xf3 => Ok(SerialCommand::NackDeviceError),
            0xf4 => Ok(SerialCommand::NackDeviceBusy),
            0xfe => Ok(SerialCommand::Ping),
            0xff => Ok(SerialCommand::Ack),
            _ => Err(value),
        }
    }
}
