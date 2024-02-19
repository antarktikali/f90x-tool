use anyhow::{Result, anyhow};
use log::error;

pub const OK_RESPONSE: &'static [u8] = &[0x06, 0x00];
// "1020F90X/N90S[null][end of text][ack]"
pub const EXPECTED_UNIT_INQUIRY_RESPONSE: &'static [u8; 16] = &[
    0x31, 0x30, 0x32, 0x30, 0x46, 0x39, 0x30, 0x58, 0x2F, 0x4E, 0x39, 0x30, 0x53, 0x00, 0x03, 0x06
];

#[derive(Debug)]
pub enum CameraCommand {
    Wakeup,
    UnitInquiry,
    Focus,
    Shoot,
    ReadMemory {
        memory_space: u8,
        address: u16,
        length: u8,
    },
    WriteToMemory {
        address: u16,
        values: Vec<u8>,
    },
}

impl CameraCommand {
    pub fn get_bytes(&self) -> Vec<u8> {
        match self {
            CameraCommand::Wakeup => vec![0x00],
            CameraCommand::UnitInquiry => vec![0x53, 0x31, 0x30, 0x30, 0x30, 0x05],
            CameraCommand::Focus => vec![0x01, 0x20, 0x86, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03],
            CameraCommand::Shoot => vec![0x01, 0x20, 0x85, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03],
            CameraCommand::ReadMemory { memory_space, address, length } => {
                CameraCommand::build_read_memory_command(*memory_space, *address, *length)
            },
            CameraCommand::WriteToMemory { address, values } => {
                CameraCommand::build_write_to_memory_command(*address, &values)
            },
        }
    }

    fn build_read_memory_command(memory_space: u8, address: u16, length: u8) -> Vec<u8> {
        vec![0x01, 0x20, 0x80,
             memory_space,
             ((address >> 8) as u8), (address as u8),
             0x00,
             length,
             0x03
        ]
    }

    fn build_write_to_memory_command(address: u16, values: &Vec<u8>) -> Vec<u8> {
        if values.len() > (u8::MAX as usize) {
            error!("Too many values ({}) given for the write command. Returning empty bytes.", values.len());
            return Vec::new();
        }
        let data_packet = DataPacket { bytes: values.clone() };
        let mut data_packet = data_packet.serialize();
        let mut write_packet = vec![
                0x01, 0x20, 0x81,
                0x00,
                ((address >> 8) as u8), (address as u8),
                0x00,
                values.len() as u8,
        ];
        write_packet.append(&mut data_packet);
        return write_packet;
    }
}

pub struct DataPacket {
    pub bytes: Vec<u8>
}

impl DataPacket {
    pub fn serialize(&self) -> Vec<u8> {
        let mut serialized: Vec<u8> = Vec::new();

        serialized.push(0x02); // "start" byte
        serialized.extend(&self.bytes);
        serialized.push(DataPacket::calculate_checksum(&self.bytes));
        serialized.push(0x03); // "end" byte

        return serialized;
    }

    pub fn deserialize(data: &Vec<u8>) -> Result<DataPacket> {
        if data.len() < 4 {
            return Err(anyhow!["Data packet has incorrect number of bytes."]);
        }
        if data[0] != 0x02u8 {
            return Err(anyhow!["Data packet header is wrong."]);
        }
        if data[data.len() - 1] != 0x03u8 {
            return Err(anyhow!["Data packet end is wrong."]);
        }

        let checksum_index: usize = data.len() - 2;
        let payload_bytes: &[u8] = &data[1 .. checksum_index];

        let expected_checksum = DataPacket::calculate_checksum(payload_bytes);
        if expected_checksum != data[checksum_index] {
            return Err(anyhow!["Received wrong checksum."]);
        }

        return Ok(DataPacket {
            bytes: payload_bytes.to_vec()
        });
    }

    fn calculate_checksum(data: &[u8]) -> u8 {
        let mut checksum: u16 = 0;
        for &byte in data {
            checksum += byte as u16;
            checksum %= 0xFF;
        }
        return (checksum % 0xFF) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_unit_inquiry_response_should_be_correct() {
        let expected: [u8; 16] = [0x31, 0x30, 0x32, 0x30, 0x46, 0x39, 0x30, 0x58, 0x2F, 0x4E, 0x39, 0x30, 0x53, 0x00, 0x03, 0x06];
        assert_eq!(&expected, EXPECTED_UNIT_INQUIRY_RESPONSE);
    }

    #[test]
    fn test_read_memory_command() {
        let cmd = CameraCommand::ReadMemory { memory_space: 0xA1, address: 0xB2C3, length: 0xD4 };
        let expected: Vec<u8> = vec![0x01, 0x20, 0x80, 0xA1, 0xB2, 0xC3, 0x00, 0xD4, 0x03];
        assert_eq!(expected, cmd.get_bytes());
    }

    #[test]
    fn test_write_memory_command() {
        let cmd = CameraCommand::WriteToMemory { address: 0xAABB, values: vec![0x0C, 0x0D, 0x0E] };
        let expected: Vec<u8> = vec![
            0x01, 0x20, 0x81,
            0x00,
            0xAA, 0xBB,
            0x00,
            0x03, // length
            0x02, // "start"
            0x0C, 0x0D, 0x0E, // payload
            0x27, // checksum
            0x03 // "stop"
        ];
        assert_eq!(expected, cmd.get_bytes());
    }

    #[test]
    fn test_write_memory_command_with_large_checksum() {
        let cmd = CameraCommand::WriteToMemory { address: 0x1122, values: vec![0xFA, 0x10] };
        let expected: Vec<u8> = vec![
            0x01, 0x20, 0x81,
            0x00,
            0x11, 0x22,
            0x00,
            0x02, // length
            0x02, // "start"
            0xFA, 0x10, // payload
            0x0B, // checksum
            0x03 // "stop"
        ];
        assert_eq!(expected, cmd.get_bytes());
    }

    #[test]
    fn write_memory_with_too_many_values_should_return_empty_bytes() {
        let cmd = CameraCommand::WriteToMemory { address: 0xAABB, values: vec![0x00; 256] };
        assert!(cmd.get_bytes().is_empty());
    }

    #[test]
    fn test_unit_inquiry_command() {
        let cmd = CameraCommand::UnitInquiry;
        let expected: Vec<u8> = vec![0x53, 0x31, 0x30, 0x30, 0x30, 0x05];
        assert_eq!(expected, cmd.get_bytes());
    }

    #[test]
    fn test_wakeup_command() {
        let cmd = CameraCommand::Wakeup;
        let expected: Vec<u8> = vec![0x00];
        assert_eq!(expected, cmd.get_bytes());
    }

    #[test]
    fn test_camera_focus_command() {
        let cmd = CameraCommand::Focus;
        let expected: Vec<u8> = vec![0x01, 0x20, 0x86, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03];
        assert_eq!(expected, cmd.get_bytes());
    }

    #[test]
    fn test_camera_shoot_command() {
        let cmd = CameraCommand::Shoot;
        let expected: Vec<u8> = vec![0x01, 0x20, 0x85, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03];
        assert_eq!(expected, cmd.get_bytes());
    }

    #[test]
    fn too_short_data_packet_should_be_error() {
        let packet: Vec<u8> = vec![0x02, 0x00, 0x03];
        assert!(DataPacket::deserialize(&packet).is_err());
    }

    #[test]
    fn data_packet_with_wrong_checksum_should_be_error() {
        let packet: Vec<u8> = vec![0x02, 0x04, 0x03, 0x06, 0x03];
        assert!(DataPacket::deserialize(&packet).is_err());
    }

    #[test]
    fn data_packet_with_wrong_start_should_be_error() {
        let packet: Vec<u8> = vec![0x01, 0x04, 0x03, 0x07, 0x03];
        assert!(DataPacket::deserialize(&packet).is_err());
    }

    #[test]
    fn data_packet_with_wrong_end_should_be_error() {
        let packet: Vec<u8> = vec![0x02, 0x04, 0x03, 0x07, 0x04];
        assert!(DataPacket::deserialize(&packet).is_err());
    }

    #[test]
    fn data_packet_should_be_deserialized_correctly() {
        let expected_payload: Vec<u8> = vec![0x04, 0x03];
        let packet: Vec<u8> = vec![0x02, 0x04, 0x03, 0x07, 0x03];
        let result = DataPacket::deserialize(&packet);
        match result {
            Ok(deserialized) => { assert_eq!(&expected_payload, &deserialized.bytes); },
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn data_packet_with_large_checksum_should_be_deserialized_correctly() {
        let expected_payload: Vec<u8> = vec![0xFA, 0x0A, 0x04];
        // 250 + 10 + 4: 264 -> 9
        let packet: Vec<u8> = vec![0x02, 0xFA, 0x0A, 0x04, 0x09, 0x03];
        let result = DataPacket::deserialize(&packet);
        match result {
            Ok(deserialized) => { assert_eq!(&expected_payload, &deserialized.bytes); },
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn data_packet_should_be_serialized_correctly() {
        let packet = DataPacket {
            bytes: vec![0x04, 0x03]
        };
        let expected: Vec<u8> = vec![0x02, 0x04, 0x03, 0x07, 0x03];
        assert_eq!(expected, packet.serialize());
    }
}

