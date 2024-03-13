use crate::camera_interface::CameraInterface;
use crate::camera_interface::messaging::CameraCommand;

use anyhow::Result;

#[cfg(test)]
use mockall::{predicate::*, Sequence};

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

#[cfg(test)]
mod tests {
    use super::*;
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
}


// TODO
// Externally needed things:
// - Read shooting data (complete)
// - Delete shooting data
// - Read unfinished shooting data
// Internally needed things:
// - Check if there is data, read 0xFD42 (6 bytes)
// + Get ring buffer start and end address (0xFD00)
// - Get shooting data settings (0xFD40)
// + Get data pointers (0xFD42)
// - Get memo holder info, how many bytes?
// - Do the actual reading, possibly wraparound for the ring buffer.
// - Delete?

