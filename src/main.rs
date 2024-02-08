mod camera_interface;

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
    // Reads given memory address
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
}

fn main() -> Result<()> {
    env_logger::init();
    let arguments = Arguments::parse();

    match arguments.command {
        Commands::Read { serial_device, address, length, memory_space } => {
            camera_interface::read_memory_in_new_session(&serial_device, address, length, memory_space)
        }?
    };

    return Ok(());
}

