use crate::camera_interface::{CameraInterface, SerialConnection};
use crate::camera_interface::messaging::CameraCommand;

use anyhow::Result;

pub fn read_memory_in_new_session(serial_device: &String, address: u16, length: u8, memory_space: u8) -> Result<()> {
    let serial = SerialConnection::new(&serial_device)?;
    let mut camera = CameraInterface::new(serial);
    camera.start_new_session()?;
    camera.send_command(&CameraCommand::ReadMemory { memory_space, address, length })?;
    let data_packet = camera.expect_data_packet(length)?;
    println!("Memory value: {:02X?}", &data_packet.bytes);

    return Ok(());
}

pub fn autofocus_in_new_session(serial_device: &String) -> Result<()> {
    let serial = SerialConnection::new(&serial_device)?;
    let mut camera = CameraInterface::new(serial);
    camera.start_new_session()?;
    camera.send_command(&CameraCommand::Focus)?;
    camera.expect_ok_response()?;

    return Ok(());
}

pub fn release_shutter_in_new_session(serial_device: &String) -> Result<()> {
    let serial = SerialConnection::new(&serial_device)?;
    let mut camera = CameraInterface::new(serial);
    camera.start_new_session()?;
    camera.send_command(&CameraCommand::Shoot)?;
    camera.expect_ok_response()?;

    return Ok(());
}

