mod camera_interface;
mod cli_commands;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// A tool to read a bytes at a given memory address of a Nikon F90x camera
#[derive(Parser)]
struct Arguments {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Reads given memory address
    Read {
        /// Serial device to use.
        serial_device: String,
        /// Address to read. Prefix with 0x for hex value.
        #[clap(value_parser=clap_num::maybe_hex::<u16>)]
        address: u16,
        /// Number of bytes to read.
        #[arg(default_value_t = 1)]
        length: u8,
        /// Memory space to read from.
        #[arg(default_value_t = 0)]
        memory_space: u8,
    },
    /// Writes to the "0" memory space starting from the given address. Number of bytes to write
    /// depends on the number of values given.
    Write {
        /// Serial device to use.
        serial_device: String,
        /// Starting address to write to. Prefix with 0x for hex value.
        #[clap(value_parser=clap_num::maybe_hex::<u16>)]
        address: u16,
        /// Byte values to write. Separate by space for multiple bytes. Prefix each with 0x for hex value.
        #[clap(value_parser=clap_num::maybe_hex::<u8>)]
        write_values: Vec<u8>,
    },
    /// Triggers auto-focus.
    Focus {
        /// Serial device to use.
        serial_device: String,
    },
    /// Releases the shutter.
    Shoot {
        /// Serial device to use.
        serial_device: String,
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let arguments = Arguments::parse();

    match arguments.command {
        Commands::Read { serial_device, address, length, memory_space } => {
            cli_commands::read_memory_in_new_session(&serial_device, address, length, memory_space)
        }?,
        Commands::Write { serial_device, address, write_values } => {
            cli_commands::write_memory_in_new_session(&serial_device, address, write_values)?
        },
        Commands::Focus { serial_device } => cli_commands::autofocus_in_new_session(&serial_device)?,
        Commands::Shoot { serial_device } => cli_commands::release_shutter_in_new_session(&serial_device)?,
    };

    return Ok(());
}

