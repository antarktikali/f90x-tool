mod messaging;

use anyhow::{Context, Result, anyhow};
use messaging::CameraCommand;
use log::debug;
use std::thread;
use std::time::Duration;

pub fn read_memory_in_new_session(serial_device: &String, address: u16, length: u8, memory_space: u8) -> Result<()> {
    let default_baud_rate = 1200;
    let default_serial_timeout = 2000;

    let mut serial = serialport::new(serial_device, default_baud_rate)
            .timeout(Duration::from_millis(default_serial_timeout))
            .open()
            .with_context(|| format!("Could not open the serial device \"{}\"", &serial_device))?;

    send_wakeup_command(&mut serial)?;
    do_unit_inquiry(&mut serial)?;
    let payload = read_memory(&mut serial, address, length, memory_space)?;
    println!("Memory value: {:02X?}", payload);

    return Ok(());
}

pub fn autofocus_in_new_session(serial_device: &String) -> Result<()> {
    let default_baud_rate = 1200;
    let default_serial_timeout = 2000;

    let mut serial = serialport::new(serial_device, default_baud_rate)
            .timeout(Duration::from_millis(default_serial_timeout))
            .open()
            .with_context(|| format!("Could not open the serial device \"{}\"", &serial_device))?;

    send_wakeup_command(&mut serial)?;
    do_unit_inquiry(&mut serial)?;
    send_focus_command(&mut serial)?;
    expect_ok_response(&mut serial)?;

    return Ok(());
}

pub fn release_shutter_in_new_session(serial_device: &String) -> Result<()> {
    let default_baud_rate = 1200;
    let default_serial_timeout = 2000;

    let mut serial = serialport::new(serial_device, default_baud_rate)
            .timeout(Duration::from_millis(default_serial_timeout))
            .open()
            .with_context(|| format!("Could not open the serial device \"{}\"", &serial_device))?;

    send_wakeup_command(&mut serial)?;
    do_unit_inquiry(&mut serial)?;
    send_shoot_command(&mut serial)?;
    expect_ok_response(&mut serial)?;

    return Ok(());
}

fn send_wakeup_command(serial: &mut Box<dyn serialport::SerialPort>) -> Result<()> {
    // Send "wakeup"
    let cmd = CameraCommand::Wakeup.get_bytes();
    debug!("Sending wakeup command: {:02X?}", cmd);
    serial.write(&cmd.as_slice())?;

    // If the camera was already awake, we might get some bytes. We don't really care about them.
    // If the camera was asleep, we won't get a response.
    thread::sleep(Duration::from_millis(200));
    let num_bytes_available = serial.bytes_to_read()?;
    if 0 < num_bytes_available {
        let mut read_buffer: [u8; 16] = [ 0; 16 ];
        let num_bytes_read = serial.read(&mut read_buffer)?;
        debug!("Cleaned the bytes from the input buffer: {:02X?}", &read_buffer[0..num_bytes_read]);
    }
    debug!("Clearing input buffer");
    serial.clear(serialport::ClearBuffer::Input)?;

    return Ok(());
}

fn do_unit_inquiry<T: std::io::Write + std::io::Read>(serial: &mut T) -> Result<()> {
    // Send the unit inquiry, this starts the "session"
    let cmd = CameraCommand::UnitInquiry.get_bytes();
    debug!("Sending unit inquiry: {:02X?}", cmd);
    serial.write(&cmd.as_slice())?;

    // Handle the unit inquiry response
    let mut read_buffer: [u8; 16] = [ 0; 16 ];
    serial.read_exact(&mut read_buffer)?;
    debug!("Received unit inquiry response: {:02X?}", read_buffer);
    validate_unit_response(&read_buffer)?;

    return Ok(());
}

fn read_memory<T: std::io::Write + std::io::Read>(
        serial: &mut T,
        address: u16,
        length: u8,
        memory_space: u8
    ) -> Result<Vec<u8>, anyhow::Error> {
    let cmd = CameraCommand::ReadMemory {
        memory_space,
        address,
        length
    }.get_bytes();
    debug!("Sending read memory command: {:02X?}", cmd);
    serial.write(&cmd.as_slice())?;

    // Handle the response
    let mut read_buffer: Vec<u8> = vec![0x0; (length + 3).into()];
    serial.read_exact(&mut read_buffer)?;
    debug!("Received response: {:02X?}", read_buffer);

    return messaging::parse_data_packet(&read_buffer, length);
}

fn validate_unit_response(response: &[u8; 16]) -> Result<()> {
    // "1020F90X/N90S[null][end of text][ack]"
    let expected_response: [u8; 16] = [0x31, 0x30, 0x32, 0x30, 0x46, 0x39, 0x30, 0x58, 0x2F, 0x4E, 0x39, 0x30, 0x53, 0x00, 0x03, 0x06];
    if response == &expected_response {
        return Ok(());
    } else {
        return Err(anyhow!("Unexpected response to unit inquiry command: {:02X?}", response));
    }
}

fn send_focus_command<T: std::io::Write>(serial: &mut T) -> Result<()> {
    let cmd = CameraCommand::Focus.get_bytes();
    debug!("Sending focus command: {:02X?}", cmd);
    serial.write(&cmd)?;

    return Ok(());
}

fn send_shoot_command<T: std::io::Write>(serial: &mut T) -> Result<()> {
    let cmd = CameraCommand::Shoot.get_bytes();
    debug!("Sending shutter release command: {:02X?}", cmd);
    serial.write(&cmd)?;

    return Ok(());
}

fn expect_ok_response<T: std::io::Read>(serial: &mut T) -> Result<()> {
    let mut read_buffer: [u8; 2] = [0; 2];
    serial.read_exact(&mut read_buffer)?;
    debug!("Received response: {:02X?}", read_buffer);
    if read_buffer != messaging::OK_RESPONSE {
        return Err(anyhow!("Received unexpected response."));
    }
    return Ok(());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_unit_inquiry_response_should_be_error() {
        let invalid_response: [u8; 16] = [0x31, 0x30, 0x32, 0x30, 0x45, 0x39, 0x30, 0x58, 0x2F, 0x4E, 0x39, 0x30, 0x53, 0x00, 0x03, 0x06];
        assert!(validate_unit_response(&invalid_response).is_err());
    }

    #[test]
    fn correct_unit_inquiry_response_should_be_validated() {
        let correct_response: [u8; 16] = [0x31, 0x30, 0x32, 0x30, 0x46, 0x39, 0x30, 0x58, 0x2F, 0x4E, 0x39, 0x30, 0x53, 0x00, 0x03, 0x06];
        assert!(validate_unit_response(&correct_response).is_ok());
    }

    #[test]
    fn correct_focus_command_should_be_sent() {
        let mut buf: Vec<u8> = Vec::new();
        assert!(send_focus_command(&mut buf).is_ok());
        assert_eq!(CameraCommand::Focus.get_bytes(), buf);
    }

    #[test]
    fn correct_shoot_command_should_be_sent() {
        let mut buf: Vec<u8> = Vec::new();
        assert!(send_shoot_command(&mut buf).is_ok());
        assert_eq!(CameraCommand::Shoot.get_bytes(), buf);
    }

    #[test]
    fn too_short_response_should_fail() {
        let buf: [u8; 1] = [0x06];
        assert!(expect_ok_response(&mut &buf[..]).is_err());
    }

    #[test]
    fn wrong_response_should_fail() {
        let buf: [u8; 2] = [0x06, 0x01];
        assert!(expect_ok_response(&mut &buf[..]).is_err());
    }

    #[test]
    fn correct_response_should_be_ok() {
        let buf: [u8; 2] = [0x06, 0x00];
        assert!(expect_ok_response(&mut &buf[..]).is_ok());
    }
}

