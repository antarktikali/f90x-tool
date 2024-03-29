use crate::camera_interface::{SerialCameraConnection, CameraInterface, SerialConnection};
use crate::camera_interface::messaging::CameraCommand;

use anyhow::{Result, anyhow};

pub fn read_memory_in_new_session(
        serial_device: &String,
        address: u16,
        length: u8,
        memory_space: u8,
        use_fast_session: bool) -> Result<()> {
    let serial = SerialConnection::new(&serial_device)?;
    let mut camera = SerialCameraConnection::new(serial);
    camera.start_new_session()?;
    if use_fast_session {
        camera.upgrade_to_fast_session()?;
    }

    camera.send_command(&CameraCommand::ReadMemory { memory_space, address, length })?;
    let data_packet = camera.expect_data_packet(length)?;
    println!("Memory value: {:02X?}", &data_packet.bytes);

    if use_fast_session {
        camera.end_fast_session()?;
    }

    return Ok(());
}

pub fn write_memory_in_new_session(
        serial_device: &String,
        address: u16,
        values: Vec<u8>,
        use_fast_session: bool) -> Result<()> {
    if values.len() > (u8::MAX as usize) {
        return Err(anyhow!("Too many values given."));
    }
    let serial = SerialConnection::new(&serial_device)?;
    let mut camera = SerialCameraConnection::new(serial);
    camera.start_new_session()?;
    if use_fast_session {
        camera.upgrade_to_fast_session()?;
    }

    camera.send_command(&CameraCommand::WriteToMemory { address, values })?;
    camera.expect_ok_response()?;
    println!("Successfully written.");

    if use_fast_session {
        camera.end_fast_session()?;
    }

    return Ok(());
}

pub fn autofocus_in_new_session(serial_device: &String) -> Result<()> {
    let serial = SerialConnection::new(&serial_device)?;
    let mut camera = SerialCameraConnection::new(serial);
    camera.start_new_session()?;
    camera.send_command(&CameraCommand::Focus)?;
    camera.expect_ok_response()?;

    return Ok(());
}

pub fn release_shutter_in_new_session(serial_device: &String) -> Result<()> {
    let serial = SerialConnection::new(&serial_device)?;
    let mut camera = SerialCameraConnection::new(serial);
    camera.start_new_session()?;
    camera.send_command(&CameraCommand::Shoot)?;
    camera.expect_ok_response()?;

    return Ok(());
}

pub fn read_and_print_memo_holder_info_in_new_session(serial_device: &String) -> Result<()> {
    let serial = SerialConnection::new(&serial_device)?;
    let mut camera = SerialCameraConnection::new(serial);
    camera.start_new_session()?;
    camera.send_command(&CameraCommand::ReadMemoHolderInfo)?;
    let data_packet = camera.expect_data_packet(4)?;
    // TODO
    // Parse the response.
    // First 2 bytes are the roll number, byte-coded decimal.
    // Then comes the number of bytes in the current roll.
    println!("Received bytes: {:02X?}", &data_packet.bytes);

    return Ok(());
}

