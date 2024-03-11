pub mod messaging;

#[cfg(test)]
use mockall::{automock, predicate::*, Sequence};

use anyhow::Result;
use messaging::CameraCommand;
use log::{warn, debug};
use std::thread;
use std::time::Duration;

const DEFAULT_BAUD_RATE: u32 = 1200;

#[cfg_attr(test, automock)]
pub trait SerialInterface {
    /// Reads given number of bytes. Implementation is assumed to be blocking.
    fn read(&mut self, length: usize) -> Result<Vec<u8>, std::io::Error>;
    /// Writes the given data. Implementation is assumed to be blocking.
    fn write(&mut self, data: &Vec<u8>) -> Result<(), std::io::Error>;
    /// Clears the available data in the input buffer by reading all the available bytes. The bytes
    /// that were cleared are returned for debugging purposes.
    fn clear_input(&mut self) -> Result<Vec<u8>, std::io::Error>;
    /// Sets the BAUD rate of the serial interface.
    fn set_baud_rate(&mut self, baud_rate: u32) -> Result<(), std::io::Error>;
}

pub struct SerialConnection<T: serialport::SerialPort> {
    serial: T
}

impl SerialConnection<serialport::TTYPort> {
    pub fn new(serial_device: &String) -> Result<SerialConnection<serialport::TTYPort>, std::io::Error> {
        let default_serial_timeout = 2000;

        let serial_port = serialport::new(serial_device, DEFAULT_BAUD_RATE)
                .timeout(Duration::from_millis(default_serial_timeout))
                .open_native()?;

        return Ok(SerialConnection { serial: serial_port });
    }
}

impl<T: serialport::SerialPort> SerialInterface for SerialConnection<T> {
    fn read(&mut self, length: usize) -> Result<Vec<u8>, std::io::Error> {
        let mut read_buffer: Vec<u8> = vec![0; length];
        self.serial.read_exact(&mut read_buffer)?;
        debug!("Received bytes: {:02X?}", &read_buffer);
        return Ok(read_buffer);
    }

    fn write(&mut self, data: &Vec<u8>) -> Result<(), std::io::Error> {
        if data.is_empty() {
            warn!("Received no bytes to write");
        }
        debug!("Sending bytes: {:02X?}", &data);
        self.serial.write(&data.as_slice())?;
        return Ok(());
    }

    fn clear_input(&mut self) -> Result<Vec<u8>, std::io::Error> {
        let num_bytes_available = self.serial.bytes_to_read()?;
        let mut read_buffer: Vec<u8> = vec![0; num_bytes_available as usize];
        if 0 < num_bytes_available {
            self.serial.read_exact(&mut read_buffer)?;
            debug!("Cleaned the bytes from the input buffer: {:02X?}", &read_buffer);
        }
        debug!("Clearing input buffer");
        self.serial.clear(serialport::ClearBuffer::Input)?;
        return Ok(read_buffer);
    }

    fn set_baud_rate(&mut self, baud_rate: u32) -> Result<(), std::io::Error> {
        debug!("Setting BAUD rate to {}", baud_rate);
        self.serial.set_baud_rate(baud_rate)?;
        return Ok(());
    }
}

pub struct CameraInterface<T: SerialInterface> {
    serial: T
}

impl<T: SerialInterface> CameraInterface<T> {
    pub fn new(serial: T) -> CameraInterface<T> {
        return CameraInterface { serial };
    }

    pub fn send_command(&mut self, command: &CameraCommand) -> Result<(), std::io::Error> {
        debug!("Will send camera command: {:?}", command);
        self.serial.write(&command.get_bytes())
    }

    pub fn expect_ok_response(&mut self) -> Result<(), std::io::Error> {
        let response = self.serial.read(messaging::OK_RESPONSE.len())?;
        if response != messaging::OK_RESPONSE {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, ""));
        }
        return Ok(());
    }

    pub fn start_new_session(&mut self) -> Result<(), std::io::Error> {
        self.send_command(&CameraCommand::Wakeup)?;
        thread::sleep(Duration::from_millis(200));
        // If the camera was already awake, we might get some bytes. We don't really care about them.
        // If the camera was asleep, we won't get a response.
        self.serial.clear_input()?;
        self.send_command(&CameraCommand::UnitInquiry)?;
        let response = self.serial.read(messaging::EXPECTED_UNIT_INQUIRY_RESPONSE.len())?;
        if response != messaging::EXPECTED_UNIT_INQUIRY_RESPONSE.to_vec() {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, ""));
        }
        return Ok(());
    }

    pub fn upgrade_to_fast_session(&mut self) -> Result<(), std::io::Error> {
        self.send_command(&CameraCommand::IncreaseBaudRate)?;
        self.expect_ok_response()?;

        thread::sleep(Duration::from_millis(200));
        self.serial.set_baud_rate(9600)?;
        return Ok(());
    }

    pub fn end_fast_session(&mut self) -> Result<(), std::io::Error> {
        debug!("Ending 9600 BAUD session");
        let end_transmission_message: Vec<u8> = vec![0x04, 0x04];
        self.serial.write(&end_transmission_message)?;

        let response = self.serial.read(end_transmission_message.len())?;
        if response != end_transmission_message {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, ""));
        }

        thread::sleep(Duration::from_millis(200));
        self.serial.set_baud_rate(DEFAULT_BAUD_RATE)?;
        return Ok(());
    }

    pub fn expect_data_packet(&mut self, payload_length: u8) -> Result<messaging::DataPacket, std::io::Error> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_command_should_send_command_bytes_via_serial() {
        let command = CameraCommand::UnitInquiry;

        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_write()
                   .with(eq(command.get_bytes()))
                   .times(1)
                   .returning(|_| Ok(()));

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.send_command(&command).is_ok());
    }

    #[test]
    fn send_command_should_return_error_if_serial_fails() {
        let command = CameraCommand::Wakeup;

        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_write()
                   .with(always())
                   .times(1)
                   .returning(|_| Err(std::io::Error::new(std::io::ErrorKind::Other, "")));

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.send_command(&command).is_err());
    }

    #[test]
    fn expect_ok_response_should_read_from_serial() {
        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_read()
                   .with(eq(2))
                   .times(1)
                   .returning(|_| Ok(messaging::OK_RESPONSE.to_vec()));

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.expect_ok_response().is_ok());
    }

    #[test]
    fn expect_ok_response_should_fail_with_wrong_response() {
        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_read()
                   .with(eq(2))
                   .times(1)
                   .returning(|_| Ok(vec![0x10u8, 0x20u8]));

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.expect_ok_response().is_err());
    }

    #[test]
    fn expect_ok_response_should_fail_if_serial_fails() {
        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_read()
                   .with(always())
                   .times(1)
                   .returning(|_| Err(std::io::Error::new(std::io::ErrorKind::Other, "")));

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.expect_ok_response().is_err());
    }

    #[test]
    /// A new 1200 baud session is started by sending a wakeup command to the camera, waiting
    /// 200ms, and then making a unit inquiry.
    fn start_new_session_should_send_correct_messages() {
        let mut sequence = Sequence::new();
        let mut mock_serial = MockSerialInterface::new();
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

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.start_new_session().is_ok());
    }

    #[test]
    fn start_new_session_should_fail_if_wakeup_fails() {
        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_write()
                   .with(eq(CameraCommand::Wakeup.get_bytes()))
                   .returning(|_| Err(std::io::Error::new(std::io::ErrorKind::Other, "")));
        mock_serial.expect_clear_input()
                   .returning(|| Ok(vec![0u8]));
        mock_serial.expect_write()
                   .with(ne(CameraCommand::Wakeup.get_bytes()))
                   .returning(|_| Ok(()));

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.start_new_session().is_err());
    }

    #[test]
    fn start_new_session_should_fail_if_clearing_serial_input_fails() {
        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_write()
                   .returning(|_| Ok(()));
        mock_serial.expect_clear_input()
                   .returning(|| Err(std::io::Error::new(std::io::ErrorKind::Other, "")));

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.start_new_session().is_err());
    }

    #[test]
    fn start_new_session_should_fail_if_unit_inquiry_writing_fails() {
        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_write()
                   .with(ne(CameraCommand::UnitInquiry.get_bytes()))
                   .returning(|_| Ok(()));
        mock_serial.expect_clear_input()
                   .returning(|| Ok(vec![0u8]));
        mock_serial.expect_write()
                   .with(eq(CameraCommand::UnitInquiry.get_bytes()))
                   .returning(|_| Err(std::io::Error::new(std::io::ErrorKind::Other, "")));

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.start_new_session().is_err());
    }

    #[test]
    fn start_new_session_should_fail_if_unit_inquiry_response_reading_fails() {
        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_write()
                   .returning(|_| Ok(()));
        mock_serial.expect_clear_input()
                   .returning(|| Ok(vec![0u8]));
        mock_serial.expect_read()
                   .with(eq(messaging::EXPECTED_UNIT_INQUIRY_RESPONSE.len()))
                   .returning(|_| Err(std::io::Error::new(std::io::ErrorKind::Other, "")));

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.start_new_session().is_err());
    }

    #[test]
    fn start_new_session_should_fail_if_unit_inquiry_response_is_wrong() {
        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_write()
                   .returning(|_| Ok(()));
        mock_serial.expect_clear_input()
                   .returning(|| Ok(vec![0u8]));
        mock_serial.expect_read()
                   .with(eq(messaging::EXPECTED_UNIT_INQUIRY_RESPONSE.len()))
                   .returning(|_| Ok(vec![1u8; messaging::EXPECTED_UNIT_INQUIRY_RESPONSE.len()]));

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.start_new_session().is_err());
    }

    #[test]
    fn expect_data_packet_should_read_from_serial() {
        const EXPECTED_PAYLOAD: &'static [u8] = &[0x11, 0x22, 0x33];
        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_read()
                   .with(eq(messaging::DataPacket { bytes: EXPECTED_PAYLOAD.to_vec() }.serialize().len()))
                   .times(1)
                   .returning(|_| Ok(messaging::DataPacket { bytes: EXPECTED_PAYLOAD.to_vec() }.serialize()));

        let mut camera_interface = CameraInterface {serial: mock_serial};

        let result = camera_interface.expect_data_packet(3);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().bytes, EXPECTED_PAYLOAD.to_vec());
    }

    #[test]
    fn expect_data_packet_should_fail_if_serial_read_fails() {
        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_read()
                   .with(always())
                   .times(1)
                   .returning(|_| Err(std::io::Error::new(std::io::ErrorKind::Other, "")));

        let mut camera_interface = CameraInterface {serial: mock_serial};

        assert!(camera_interface.expect_data_packet(3).is_err());
    }

    #[test]
    fn expect_data_packet_should_fail_on_deserialization_error() {
        const INVALID_RESPONSE: &'static [u8] = &[0x02, 0x07, 0x06, 0x03];
        assert!(messaging::DataPacket::deserialize(&INVALID_RESPONSE.to_vec()).is_err());

        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_read()
                   .with(eq(4))
                   .times(1)
                   .returning(|_| Ok(INVALID_RESPONSE.to_vec()));

        let mut camera_interface = CameraInterface {serial: mock_serial};

        assert!(camera_interface.expect_data_packet(1).is_err());
    }

    #[test]
    /// An existing session is upgraded to 9600 baud session by sending a special command, and
    /// waiting 200ms before continuing with 9600 baud.
    fn upgrade_to_fast_session_should_send_correct_messages() {
        let mut sequence = Sequence::new();
        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_write()
                   .with(eq(CameraCommand::IncreaseBaudRate.get_bytes()))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(()));
        mock_serial.expect_read()
                   .with(eq(2))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(messaging::OK_RESPONSE.to_vec()));
        mock_serial.expect_set_baud_rate()
                   .with(eq(9600))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(()));

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.upgrade_to_fast_session().is_ok());
    }

    #[test]
    fn upgrade_to_fast_session_should_fail_if_ok_response_is_not_received() {
        let mut sequence = Sequence::new();
        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_write()
                   .with(eq(CameraCommand::IncreaseBaudRate.get_bytes()))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(()));
        mock_serial.expect_read()
                   .with(eq(2))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(vec![0x10u8, 0x20u8]));

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.upgrade_to_fast_session().is_err());
    }

    #[test]
    fn end_fast_session_should_send_correct_messages() {
        let mut sequence = Sequence::new();
        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_write()
                   .with(eq(vec![0x04u8, 0x04u8]))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(()));
        mock_serial.expect_read()
                   .with(eq(2))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(vec![0x04u8, 0x04u8]));
        mock_serial.expect_set_baud_rate()
                   .with(eq(1200))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(()));

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.end_fast_session().is_ok());
    }

    #[test]
    fn end_fast_session_should_fail_if_eot_message_is_not_received() {
        let mut sequence = Sequence::new();
        let mut mock_serial = MockSerialInterface::new();
        mock_serial.expect_write()
                   .with(eq(vec![0x04u8, 0x04u8]))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(()));
        mock_serial.expect_read()
                   .with(eq(2))
                   .times(1)
                   .in_sequence(&mut sequence)
                   .returning(|_| Ok(vec![0x01u8, 0x01u8]));

        let mut camera_interface = CameraInterface {serial: mock_serial};
        assert!(camera_interface.end_fast_session().is_err());
    }

}

