use crate::camera_interface::CameraInterface;
use crate::camera_interface::messaging::CameraCommand;

use anyhow::{Result, anyhow};

#[cfg(test)]
use mockall::{predicate::*, Sequence};

enum MemoHolderSetting {
    DoNotStore,
    Minimum,
    Intermediate,
    Full,
}

impl MemoHolderSetting {
    fn get_bytes_per_frame(&self) -> u8 {
        match self {
            Self::DoNotStore   => 0,
            Self::Minimum      => 2,
            Self::Intermediate => 4,
            Self::Full         => 6,
        }
    }
}

struct MemoHolderInfo {
    roll_id: u16,
    bytes_to_read: u16,
}

struct RingBufferAddresses {
    start: u16, // 0xFD00
    end: u16,   // 0xFD02
}
struct MemoHolderAddresses {
    start: u16,              // 0xFD44
    current_roll_start: u16, // 0xFD46
    current: u16,            // 0xFD42
}

fn get_ring_buffer_addresses<T: CameraInterface>(camera: &mut T) -> Result<RingBufferAddresses> {
    camera.send_command(&CameraCommand::ReadMemory { memory_space: 0, address: 0xFD00, length: 4})?;
    let data_packet = camera.expect_data_packet(4)?;

    let start = read_little_endian_u16(&data_packet.bytes, 0)?;
    let end = read_little_endian_u16(&data_packet.bytes, 2)?;

    return Ok(RingBufferAddresses { start, end });
}

fn get_memo_holder_addresses<T: CameraInterface>(camera: &mut T) -> Result<MemoHolderAddresses> {
    camera.send_command(&CameraCommand::ReadMemory { memory_space: 0, address: 0xFD42, length: 6})?;
    let data_packet = camera.expect_data_packet(6)?;

    let current = read_little_endian_u16(&data_packet.bytes, 0)?;
    let start = read_little_endian_u16(&data_packet.bytes, 2)?;
    let current_roll_start = read_little_endian_u16(&data_packet.bytes, 4)?;

    return Ok(MemoHolderAddresses { start, current_roll_start, current });
}

fn get_memo_holder_setting<T: CameraInterface>(camera: &mut T) -> Result<MemoHolderSetting> {
    camera.send_command(&CameraCommand::ReadMemory { memory_space: 0, address: 0xFD40, length: 1})?;
    let data_packet = camera.expect_data_packet(1)?;
    let value = data_packet.bytes.first().ok_or(anyhow!("Could not get the memory value"))?;
    const MEMO_HOLDER_ENABLED_FLAG: u8 = 0x40;
    if (value & MEMO_HOLDER_ENABLED_FLAG) == 0x00 {
        return Ok(MemoHolderSetting::DoNotStore);
    }
    match value {
        &0x45 => Ok(MemoHolderSetting::Minimum),
        &0x4E => Ok(MemoHolderSetting::Intermediate),
        &0x5F => Ok(MemoHolderSetting::Full),
        _ => Err(anyhow!("Unspecified memo holder setting value: {:02X?}", value))
    }
}

fn get_memo_holder_info<T: CameraInterface>(camera: &mut T) -> Result<MemoHolderInfo> {
    camera.send_command(&CameraCommand::ReadMemoHolderInfo)?;
    let data_packet = camera.expect_data_packet(4)?;
    let bytes_to_read = read_little_endian_u16(&data_packet.bytes, 2)?;

    let roll_id_raw = read_little_endian_u16(&data_packet.bytes, 0)?;
    let roll_id = read_4_digit_bcd(roll_id_raw)?;

    return Ok(MemoHolderInfo { roll_id, bytes_to_read });
}

/// Read little endian u16 from the given vector.
///
/// Returns error if the vector doesn't have enough bytes after the given index.
fn read_little_endian_u16(bytes: &Vec<u8>, start_index: usize) -> Result<u16> {
    if start_index + 1 >= bytes.len() {
        return Err(anyhow!("Not enough bytes to read a u16. Given bytes: {:?}, given index: {}",
                           bytes, start_index));
    }

    let mut bytes_to_read = [0u8, 2];
    bytes_to_read.clone_from_slice(&bytes[start_index..start_index+2]);
    return Ok(u16::from_le_bytes(bytes_to_read));
}

/// Reads a 4 byte coded decimal.
///
/// Returns error if invalid nibbles are given. For example if the nibble value is not 0-9 in hex.
fn read_4_digit_bcd(encoded: u16) -> Result<u16> {
    let mut digits: [u16; 4] = [0; 4];
    digits[0] = encoded & 0x0F;
    digits[1] = (encoded >> 4) & 0x0F;
    digits[2] = (encoded >> 8) & 0x0F;
    digits[3] = (encoded >> 12) & 0x0F;
    for digit in digits {
        if digit > 9 {
            return Err(anyhow!("Invalid nibble value: {:02X?}", digit));
        }
    }

    return Ok(
        digits[0] +
        digits[1] * 10 +
        digits[2] * 100 +
        digits[3] * 1000
    );

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;
    use crate::camera_interface::messaging::CameraCommand;
    use crate::camera_interface::messaging::DataPacket;
    use crate::camera_interface::MockCameraInterface;

    #[test]
    fn should_read_ring_buffer_addresses_correctly() {
        let mut sequence = Sequence::new();
        let mut mock_camera = MockCameraInterface::new();
        mock_camera.expect_send_command()
                   .with(eq(CameraCommand::ReadMemory {memory_space: 0, address: 0xFD00, length: 4}))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(()));
        mock_camera.expect_expect_data_packet()
                   .with(eq(4))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(DataPacket {bytes: vec![0xAB, 0xCD, 0x12, 0x34]}));
        let result = get_ring_buffer_addresses(&mut mock_camera).unwrap();
        assert_eq!(result.start, 0xCDAB);
        assert_eq!(result.end, 0x3412);
    }

    #[test]
    fn should_read_shooting_data_addresses_correctly() {
        let mut sequence = Sequence::new();
        let mut mock_camera = MockCameraInterface::new();
        mock_camera.expect_send_command()
                   .with(eq(CameraCommand::ReadMemory {memory_space: 0, address: 0xFD42, length: 6}))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(()));
        mock_camera.expect_expect_data_packet()
                   .with(eq(6))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(DataPacket {bytes: vec![0x98, 0x76, 0xAB, 0xCD, 0x12, 0x34]}));
        let result = get_memo_holder_addresses(&mut mock_camera).unwrap();
        assert_eq!(result.start, 0xCDAB);
        assert_eq!(result.current_roll_start, 0x3412);
        assert_eq!(result.current, 0x7698);
    }

    #[test]
    fn frame_length_for_no_data_should_be_correct() {
        let setting = MemoHolderSetting::DoNotStore;
        assert_eq!(0, setting.get_bytes_per_frame());
    }

    #[test]
    fn frame_length_for_minimum_data_should_be_correct() {
        let setting = MemoHolderSetting::Minimum;
        assert_eq!(2, setting.get_bytes_per_frame());
    }

    #[test]
    fn frame_length_for_intermediate_data_should_be_correct() {
        let setting = MemoHolderSetting::Intermediate;
        assert_eq!(4, setting.get_bytes_per_frame());
    }

    #[test]
    fn frame_length_for_full_data_should_be_correct() {
        let setting = MemoHolderSetting::Full;
        assert_eq!(6, setting.get_bytes_per_frame());
    }

    #[test]
    fn should_read_disabled_memo_holder_settings_correctly() {
        memo_holder_setting_test(0x00, MemoHolderSetting::DoNotStore);
        memo_holder_setting_test(0x05, MemoHolderSetting::DoNotStore);
        memo_holder_setting_test(0x0E, MemoHolderSetting::DoNotStore);
        memo_holder_setting_test(0x1F, MemoHolderSetting::DoNotStore);
    }

    #[test]
    fn should_read_minimum_memo_holder_setting_correctly() {
        memo_holder_setting_test(0x45, MemoHolderSetting::Minimum);
    }

    #[test]
    fn should_read_intermediate_memo_holder_setting_correctly() {
        memo_holder_setting_test(0x4E, MemoHolderSetting::Intermediate);
    }

    #[test]
    fn should_read_full_memo_holder_setting_correctly() {
        memo_holder_setting_test(0x5F, MemoHolderSetting::Full);
    }

    #[test]
    fn should_raise_error_on_unknown_memo_holder_setting_with_enabled_flag() {
        let mut sequence = Sequence::new();
        let mut mock_camera = MockCameraInterface::new();
        mock_camera.expect_send_command()
            .with(eq(CameraCommand::ReadMemory {memory_space: 0, address: 0xFD40, length: 1}))
            .times(1)
            .in_sequence(&mut sequence)
            .returning(|_| Ok(()));
        mock_camera.expect_expect_data_packet()
            .with(eq(1))
            .times(1)
            .in_sequence(&mut sequence)
            .returning(move |_| Ok(DataPacket {bytes: vec![0x41]}));

        let result = get_memo_holder_setting(&mut mock_camera);
        assert!(result.is_err());
    }

    fn memo_holder_setting_test(camera_value: u8, expected_result: MemoHolderSetting) {
        let mut sequence = Sequence::new();
        let mut mock_camera = MockCameraInterface::new();
        mock_camera.expect_send_command()
            .with(eq(CameraCommand::ReadMemory {memory_space: 0, address: 0xFD40, length: 1}))
            .times(1)
            .in_sequence(&mut sequence)
            .returning(|_| Ok(()));
        mock_camera.expect_expect_data_packet()
            .with(eq(1))
            .times(1)
            .in_sequence(&mut sequence)
            .returning(move |_| Ok(DataPacket {bytes: vec![camera_value]}));

        let result = get_memo_holder_setting(&mut mock_camera).unwrap();
        assert_eq!(mem::discriminant(&expected_result), mem::discriminant(&result));
    }

    #[test]
    fn should_read_little_endian_u16_correctly() {
        let bytes: Vec<u8> = vec![0x12, 0x34, 0x56, 0x78];
        assert_eq!(0x7856, read_little_endian_u16(&bytes, 2).unwrap());

        let bytes: Vec<u8> = vec![0x12, 0x34];
        assert_eq!(0x3412, read_little_endian_u16(&bytes, 0).unwrap());
    }

    #[test]
    fn should_return_error_if_not_enough_bytes_for_little_endian_u16() {
        let bytes: Vec<u8> = vec![0x11];
        assert!(read_little_endian_u16(&bytes, 0).is_err());
    }

    #[test]
    fn should_return_error_if_not_enough_bytes_remaining_for_little_endian_u16() {
        let bytes: Vec<u8> = vec![0xAA, 0xBB, 0xCC];
        assert!(read_little_endian_u16(&bytes, 2).is_err());
    }

    #[test]
    fn should_read_4_digit_bcd_correctly() {
        let encoded: u16 = 0x3162;
        assert_eq!(3162, read_4_digit_bcd(encoded).unwrap());
    }

    #[test]
    fn should_return_error_if_4_digit_bcd_is_invalid() {
        let encoded: u16 = 0x101A;
        assert!(read_4_digit_bcd(encoded).is_err());
    }

    #[test]
    fn should_read_memo_holder_info_correctly() {
        let mut sequence = Sequence::new();
        let mut mock_camera = MockCameraInterface::new();
        mock_camera.expect_send_command()
                   .with(eq(CameraCommand::ReadMemoHolderInfo))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(()));
        mock_camera.expect_expect_data_packet()
                   .with(eq(4))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(DataPacket {bytes: vec![0x37, 0x13, 0xCD, 0xAB]}));
        let result = get_memo_holder_info(&mut mock_camera).unwrap();
        assert_eq!(result.roll_id, 1337);
        assert_eq!(result.bytes_to_read, 0xABCD);
    }
}


// TODO
// Externally needed things:
// - Read next completed shooting data
// - Delete shooting data
// - Read unfinished shooting data
// Internally needed things:
// - Check if there is data, read 0xFD42 (6 bytes)
// + Get ring buffer start and end address (0xFD00)
// + Get shooting data settings (0xFD40)
// + Get data pointers (0xFD42)
// + Get memo holder info, how many bytes?
// - Do the actual reading, possibly wraparound for the ring buffer.
// - Delete?

