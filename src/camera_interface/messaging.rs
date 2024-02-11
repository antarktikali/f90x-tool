use anyhow::{Result, anyhow};

pub const OK_RESPONSE: &'static [u8] = &[0x06, 0x00];

pub enum CameraCommand {
    Wakeup,
    UnitInquiry,
    Focus,
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
            CameraCommand::ReadMemory { memory_space, address, length } => {
                vec![0x01, 0x20, 0x80, *memory_space, ((address >> 8) as u8), (*address as u8), 0x00, *length, 0x03]
            }
        }
    }
}

pub fn parse_data_packet(bytes: &Vec<u8>, expected_payload_length: u8) -> Result<Vec<u8>, anyhow::Error> {
    if bytes.is_empty() {
        return Err(anyhow!["Received empty packet."]);
    }
    if bytes.len() != (expected_payload_length as usize) + 3 {
        return Err(anyhow!["Received incorrect number of bytes."]);
    }
    if bytes[0] != 0x02u8 {
        return Err(anyhow!["Data packet header is wrong."]);
    }
    if bytes[bytes.len() - 1] != 0x03u8 {
        return Err(anyhow!["Data packet end is wrong."]);
    }

    let checksum_index: usize = bytes.len() - 2;
    let mut expected_checksum: u64 = 0;
    let payload_bytes: &[u8] = &bytes[1 .. checksum_index];
    for byte in payload_bytes {
        expected_checksum += *byte as u64;
    }
    if bytes[checksum_index] as u64 != (expected_checksum % 0xFF) {
        return Err(anyhow!["Received wrong checksum."]);
    }

    return Ok(payload_bytes.to_vec());
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
    fn test_unit_focus_command() {
        let cmd = CameraCommand::Focus;
        let expected: Vec<u8> = vec![0x01, 0x20, 0x86, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03];
        assert_eq!(expected, cmd.get_bytes());
    }

    #[test]
    fn empty_data_packet_should_be_error() {
        let packet: Vec<u8> = Vec::new();
        assert!(parse_data_packet(&packet, 0).is_err());
    }

    #[test]
    fn data_packet_with_shorter_length_should_be_error() {
        let packet: Vec<u8> = vec![0x02, 0x00, 0x00, 0x03];
        assert!(parse_data_packet(&packet, 2).is_err());
    }

    #[test]
    fn data_packet_with_longer_length_should_be_error() {
        let packet: Vec<u8> = vec![0x02, 0x00, 0x00, 0x03, 0x01, 0x04, 0x03];
        assert!(parse_data_packet(&packet, 1).is_err());
    }

    #[test]
    fn data_packet_with_wrong_checksum_should_be_error() {
        let packet: Vec<u8> = vec![0x02, 0x04, 0x03, 0x06, 0x03];
        assert!(parse_data_packet(&packet, 2).is_err());
    }

    #[test]
    fn data_packet_with_wrong_start_should_be_error() {
        let packet: Vec<u8> = vec![0x01, 0x04, 0x03, 0x07, 0x03];
        assert!(parse_data_packet(&packet, 2).is_err());
    }

    #[test]
    fn data_packet_with_wrong_end_should_be_error() {
        let packet: Vec<u8> = vec![0x02, 0x04, 0x03, 0x07, 0x04];
        assert!(parse_data_packet(&packet, 2).is_err());
    }

    #[test]
    fn data_packet_should_be_parsed_correctly() {
        let expected_payload: Vec<u8> = vec![0x04, 0x03];
        let packet: Vec<u8> = vec![0x02, 0x04, 0x03, 0x07, 0x03];
        let result = parse_data_packet(&packet, 2);
        match result {
            Ok(payload) => { assert_eq!(&expected_payload, &payload); },
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn data_packet_with_large_checksum_should_be_parsed_correctly() {
        let expected_payload: Vec<u8> = vec![0xFA, 0x0A, 0x04];
        // 250 + 10 + 4: 264 -> 9
        let packet: Vec<u8> = vec![0x02, 0xFA, 0x0A, 0x04, 0x09, 0x03];
        let result = parse_data_packet(&packet, 3);
        match result {
            Ok(payload) => { assert_eq!(&expected_payload, &payload); },
            Err(_) => assert!(false),
        }
    }
}

