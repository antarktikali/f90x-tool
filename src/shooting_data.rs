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

    let mut address_bytes = [0u8, 2];
    address_bytes.clone_from_slice(&data_packet.bytes[0..2]);
    let start = u16::from_le_bytes(address_bytes);
    address_bytes.clone_from_slice(&data_packet.bytes[2..4]);
    let end = u16::from_le_bytes(address_bytes);

    return Ok(RingBufferAddresses { start, end });
}

fn get_memo_holder_addresses<T: CameraInterface>(camera: &mut T) -> Result<MemoHolderAddresses> {
    camera.send_command(&CameraCommand::ReadMemory { memory_space: 0, address: 0xFD42, length: 6})?;
    let data_packet = camera.expect_data_packet(6)?;

    let mut address_bytes = [0u8, 2];
    address_bytes.clone_from_slice(&data_packet.bytes[0..2]);
    let current = u16::from_le_bytes(address_bytes);
    address_bytes.clone_from_slice(&data_packet.bytes[2..4]);
    let start = u16::from_le_bytes(address_bytes);
    address_bytes.clone_from_slice(&data_packet.bytes[4..6]);
    let current_roll_start = u16::from_le_bytes(address_bytes);

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
// - Get memo holder info, how many bytes?
// - Do the actual reading, possibly wraparound for the ring buffer.
// - Delete?

