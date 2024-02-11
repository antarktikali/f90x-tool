use anyhow::{Result, anyhow};

pub const OK_RESPONSE: &'static [u8] = &[0x06, 0x00];

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
}

impl CameraCommand {
    pub fn get_bytes(&self) -> Vec<u8> {
        match self {
            CameraCommand::Wakeup => vec![0x00],
            CameraCommand::UnitInquiry => vec![0x53, 0x31, 0x30, 0x30, 0x30, 0x05],
            CameraCommand::Focus => vec![0x01, 0x20, 0x86, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03],
            CameraCommand::Shoot => vec![0x01, 0x20, 0x85, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03],
            CameraCommand::ReadMemory { memory_space, address, length } => {
                vec![0x01, 0x20, 0x80, *memory_space, ((address >> 8) as u8), (*address as u8), 0x00, *length, 0x03]
            }
        }
    }
}

pub struct DataPacket {
    pub bytes: Vec<u8>
}

impl DataPacket {
    fn serialize(&self) -> Vec<u8> {
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
    fn test_read_memory_command() {
        let cmd = CameraCommand::ReadMemory { memory_space: 0xA1, address: 0xB2C3, length: 0xD4 };
        let expected: Vec<u8> = vec![0x01, 0x20, 0x80, 0xA1, 0xB2, 0xC3, 0x00, 0xD4, 0x03];
        assert_eq!(expected, cmd.get_bytes());
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

