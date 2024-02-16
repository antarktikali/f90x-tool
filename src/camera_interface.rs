mod messaging;

#[cfg(test)]
use mockall::{automock, predicate::*, Sequence};

use anyhow::{Context, Result, anyhow};
use messaging::CameraCommand;
use log::debug;
use std::thread;
use std::time::Duration;
use std::io::Read;

#[cfg_attr(test, automock)]
trait SerialConnection {
    /// Reads given number of bytes. Implementation is assumed to be blocking.
    fn read(&self, length: usize) -> Result<Vec<u8>, std::io::Error>;
    /// Writes the given data. Implementation is assumed to be blocking.
    fn write(&self, data: &Vec<u8>) -> Result<(), std::io::Error>;
    /// Clears the available data in the input buffer by reading all the available bytes. The bytes
    /// that were cleared are returned for debugging purposes.
    fn clear_input(&self) -> Result<Vec<u8>, std::io::Error>;
}

struct CameraInterface<T: SerialConnection> {
    serial: T
}

impl<T: SerialConnection> CameraInterface<T> {
    fn send_command(&self, command: &CameraCommand) -> Result<(), std::io::Error> {
        self.serial.write(&command.get_bytes())
    }

    fn expect_ok_response(&self) -> Result<(), std::io::Error> {
        let response = self.serial.read(messaging::OK_RESPONSE.len())?;
        if response != messaging::OK_RESPONSE {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, ""));
        }
        return Ok(());
    }

    fn start_new_session(&self) -> Result<(), std::io::Error> {
        self.send_command(&CameraCommand::Wakeup)?;
        thread::sleep(Duration::from_millis(200));
        self.serial.clear_input()?;
        self.send_command(&CameraCommand::UnitInquiry)?;
        let response = self.serial.read(messaging::EXPECTED_UNIT_INQUIRY_RESPONSE.len())?;
        if response != messaging::EXPECTED_UNIT_INQUIRY_RESPONSE {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, ""));
        }
        return Ok(());
    }

    fn expect_data_packet(&self, payload_length: u8) -> Result<messaging::DataPacket, std::io::Error> {
        // Start byte(1) + payload + checksum(1) + stop byte(1)
        let expected_length: usize = (payload_length as usize) + 3;

        let response = self.serial.read(expected_length)?;
        // TODO clean up the usage of different types of errors...
        match messaging::DataPacket::deserialize(&response) {
            Ok(payload) => Ok(payload),
            Err(_) => Err(std::io::Error::new(std::io::ErrorKind::Other, ""))

        }
    }

}

pub fn read_memory_in_new_session(serial_device: &String, address: u16, length: u8, memory_space: u8) -> Result<()> {
    let mut serial = initialize_serial(&serial_device)?;
    start_new_session(&mut serial)?;
    let payload = read_memory(&mut serial, address, length, memory_space)?;
    println!("Memory value: {:02X?}", payload);

    return Ok(());
}

pub fn autofocus_in_new_session(serial_device: &String) -> Result<()> {
    let mut serial = initialize_serial(&serial_device)?;
    start_new_session(&mut serial)?;
    send_focus_command(&mut serial)?;
    expect_ok_response(&mut serial)?;

    return Ok(());
}

pub fn release_shutter_in_new_session(serial_device: &String) -> Result<()> {
    let mut serial = initialize_serial(&serial_device)?;
    start_new_session(&mut serial)?;
    send_shoot_command(&mut serial)?;
    expect_ok_response(&mut serial)?;

    return Ok(());
}

fn initialize_serial(serial_device: &String) -> Result<Box<dyn serialport::SerialPort>> {
    let default_baud_rate = 1200;
    let default_serial_timeout = 2000;

    return serialport::new(serial_device, default_baud_rate)
            .timeout(Duration::from_millis(default_serial_timeout))
            .open()
            .with_context(|| format!("Could not open the serial device \"{}\"", &serial_device));
}

fn start_new_session(serial: &mut Box<dyn serialport::SerialPort>) -> Result<()> {
    send_wakeup_command(serial)?;
    return do_unit_inquiry(serial);
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

    let data_packet = messaging::DataPacket::deserialize(&read_buffer)?;
    return Ok(data_packet.bytes);
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

    #[test]
    fn send_command_should_send_command_bytes_via_serial() {
        let command = CameraCommand::UnitInquiry;

        let mut mock_serial = MockSerialConnection::new();
        mock_serial.expect_write()
                   .with(eq(command.get_bytes()))
                   .times(1)
                   .returning(|_| Ok(()));

        let camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.send_command(&command).is_ok());
    }

    #[test]
    fn send_command_should_return_error_if_serial_fails() {
        let command = CameraCommand::Wakeup;

        let mut mock_serial = MockSerialConnection::new();
        mock_serial.expect_write()
                   .with(always())
                   .times(1)
                   .returning(|_| Err(std::io::Error::new(std::io::ErrorKind::Other, "")));

        let camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.send_command(&command).is_err());
    }

    #[test]
    fn expect_ok_response_should_read_from_serial() {
        let mut mock_serial = MockSerialConnection::new();
        mock_serial.expect_read()
                   .with(eq(2))
                   .times(1)
                   .returning(|_| Ok(messaging::OK_RESPONSE.to_vec()));

        let camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.expect_ok_response().is_ok());
    }

    #[test]
    fn expect_ok_response_should_fail_with_wrong_response() {
        let mut mock_serial = MockSerialConnection::new();
        mock_serial.expect_read()
                   .with(eq(2))
                   .times(1)
                   .returning(|_| Ok(vec![0x10u8, 0x20u8]));

        let camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.expect_ok_response().is_err());
    }

    #[test]
    fn expect_ok_response_should_fail_if_serial_fails() {
        let mut mock_serial = MockSerialConnection::new();
        mock_serial.expect_read()
                   .with(always())
                   .times(1)
                   .returning(|_| Err(std::io::Error::new(std::io::ErrorKind::Other, "")));

        let camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.expect_ok_response().is_err());
    }

    #[test]
    /// A new 1200 baud session is started by sending a wakeup command to the camera, waiting
    /// 200ms, and then making a unit inquiry.
    fn start_new_session_should_send_correct_messages() {
        let mut sequence = Sequence::new();
        let mut mock_serial = MockSerialConnection::new();
        mock_serial.expect_write()
                   .with(eq(CameraCommand::Wakeup.get_bytes()))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(()));
        mock_serial.expect_clear_input()
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|| Ok(vec![0u8]));
        mock_serial.expect_write()
                   .with(eq(CameraCommand::UnitInquiry.get_bytes()))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(()));
        mock_serial.expect_read()
                   .with(eq(messaging::EXPECTED_UNIT_INQUIRY_RESPONSE.len()))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(messaging::EXPECTED_UNIT_INQUIRY_RESPONSE.to_vec()));

        let camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.start_new_session().is_ok());
    }

    #[test]
    fn start_new_session_should_fail_if_wakeup_fails() {
        let mut mock_serial = MockSerialConnection::new();
        mock_serial.expect_write()
                   .with(eq(CameraCommand::Wakeup.get_bytes()))
                   .returning(|_| Err(std::io::Error::new(std::io::ErrorKind::Other, "")));
        mock_serial.expect_clear_input()
                   .returning(|| Ok(vec![0u8]));
        mock_serial.expect_write()
                   .with(ne(CameraCommand::Wakeup.get_bytes()))
                   .returning(|_| Ok(()));

        let camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.start_new_session().is_err());
    }

    #[test]
    fn start_new_session_should_fail_if_clearing_serial_input_fails() {
        let mut mock_serial = MockSerialConnection::new();
        mock_serial.expect_write()
                   .returning(|_| Ok(()));
        mock_serial.expect_clear_input()
                   .returning(|| Err(std::io::Error::new(std::io::ErrorKind::Other, "")));

        let camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.start_new_session().is_err());
    }

    #[test]
    fn start_new_session_should_fail_if_unit_inquiry_writing_fails() {
        let mut mock_serial = MockSerialConnection::new();
        mock_serial.expect_write()
                   .with(ne(CameraCommand::UnitInquiry.get_bytes()))
                   .returning(|_| Ok(()));
        mock_serial.expect_clear_input()
                   .returning(|| Ok(vec![0u8]));
        mock_serial.expect_write()
                   .with(eq(CameraCommand::UnitInquiry.get_bytes()))
                   .returning(|_| Err(std::io::Error::new(std::io::ErrorKind::Other, "")));

        let camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.start_new_session().is_err());
    }

    #[test]
    fn start_new_session_should_fail_if_unit_inquiry_response_reading_fails() {
        let mut mock_serial = MockSerialConnection::new();
        mock_serial.expect_write()
                   .returning(|_| Ok(()));
        mock_serial.expect_clear_input()
                   .returning(|| Ok(vec![0u8]));
        mock_serial.expect_read()
                   .with(eq(messaging::EXPECTED_UNIT_INQUIRY_RESPONSE.len()))
                   .returning(|_| Err(std::io::Error::new(std::io::ErrorKind::Other, "")));

        let camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.start_new_session().is_err());
    }

    #[test]
    fn start_new_session_should_fail_if_unit_inquiry_response_is_wrong() {
        let mut mock_serial = MockSerialConnection::new();
        mock_serial.expect_write()
                   .returning(|_| Ok(()));
        mock_serial.expect_clear_input()
                   .returning(|| Ok(vec![0u8]));
        mock_serial.expect_read()
                   .with(eq(messaging::EXPECTED_UNIT_INQUIRY_RESPONSE.len()))
                   .returning(|_| Ok(vec![1u8; messaging::EXPECTED_UNIT_INQUIRY_RESPONSE.len()]));

        let camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.start_new_session().is_err());
    }

    #[test]
    fn expect_data_packet_should_read_from_serial() {
        const EXPECTED_PAYLOAD: &'static [u8] = &[0x11, 0x22, 0x33];
        let mut mock_serial = MockSerialConnection::new();
        mock_serial.expect_read()
                   .with(eq(messaging::DataPacket { bytes: EXPECTED_PAYLOAD.to_vec() }.serialize().len()))
                   .times(1)
                   .returning(|_| Ok(messaging::DataPacket { bytes: EXPECTED_PAYLOAD.to_vec() }.serialize()));

        let camera_interface = CameraInterface {serial: mock_serial};

        let result = camera_interface.expect_data_packet(3);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().bytes, EXPECTED_PAYLOAD.to_vec());
    }

    #[test]
    fn expect_data_packet_should_fail_if_serial_read_fails() {
        let mut mock_serial = MockSerialConnection::new();
        mock_serial.expect_read()
                   .with(always())
                   .times(1)
                   .returning(|_| Err(std::io::Error::new(std::io::ErrorKind::Other, "")));

        let camera_interface = CameraInterface {serial: mock_serial};

        assert!(camera_interface.expect_data_packet(3).is_err());
    }

    #[test]
    fn expect_data_packet_should_fail_on_deserialization_error() {
        const INVALID_RESPONSE: &'static [u8] = &[0x02, 0x07, 0x06, 0x03];
        assert!(messaging::DataPacket::deserialize(&INVALID_RESPONSE.to_vec()).is_err());

        let mut mock_serial = MockSerialConnection::new();
        mock_serial.expect_read()
                   .with(eq(4))
                   .times(1)
                   .returning(|_| Ok(INVALID_RESPONSE.to_vec()));

        let camera_interface = CameraInterface {serial: mock_serial};

        assert!(camera_interface.expect_data_packet(1).is_err());
    }

}

