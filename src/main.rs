mod render;
mod usb;

use anyhow::{ensure, Context, Result};
use clap::{Args, Parser, Subcommand};
use image::ImageReader;
use serde_json::json;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "Experimental Linux USB CLI for Nemonic (SDK 0.9.1 / WASM API 2)")]
struct Cli {
    /// SDK WASM path. Download with python3 scripts/fetch_sdk.py.
    #[arg(long, global = true, env = "NEMONIC_WASM", default_value = "assets/Nemonic.wasm")]
    wasm: PathBuf,
    #[command(flatten)]
    device: DeviceOptions,
    #[command(subcommand)]
    command: Command,
}
#[derive(Args)]
struct DeviceOptions {
    #[arg(long, global = true)]
    bus: Option<u8>,
    #[arg(long, global = true)]
    address: Option<u8>,
    #[arg(long, global = true)]
    serial: Option<String>,
}
impl DeviceOptions {
    fn open(&self, require_in: bool) -> Result<usb::Session> {
        usb::Session::open(self.bus, self.address, self.serial.as_deref(), require_in)
    }
}
#[derive(Subcommand)]
enum Command {
    /// List supported USB printers as JSON (does not print).
    List,
    /// Generate a binary print command without accessing USB.
    Render {
        #[command(flatten)]
        image: ImageOptions,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Send one generated print job to USB. May feed/cut paper immediately.
    Print {
        #[command(flatten)]
        image: ImageOptions,
    },
    /// Query printer information (may wait up to 30 seconds for printing to finish).
    Info,
    /// Query basic printer status, after a printer-information sync marker.
    Status,
}
#[derive(Args)]
struct ImageOptions {
    /// PNG/JPEG path. No implicit scaling: prepare at desired pixel dimensions.
    input: PathBuf,
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=255))]
    copies: u8,
    #[arg(long)]
    dither: bool,
}
fn generate(wasm: &std::path::Path, opt: &ImageOptions) -> Result<Vec<u8>> {
    let mut reader = ImageReader::open(&opt.input)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let rgba = reader.decode().context("decode PNG/JPEG")?.to_rgba8();
    ensure!(u64::from(rgba.width()) * u64::from(rgba.height()) * 4 <= 128 * 1024 * 1024, "image too large");
    if rgba.height() > 2000 {
        eprintln!("warning: SDK limits effective height to 2000 pixels; input is {}", rgba.height());
    }
    // Preserve RGBA including alpha, exactly like ImageData. No implicit flattening.
    render::build(wasm, rgba.width(), rgba.height(), rgba.as_raw(), opt.copies, opt.dither)
}
fn run() -> Result<()> {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::List => usb::list()?,
        Command::Render { image, output } => {
            let bytes = generate(&cli.wasm, &image)?;
            // Avoid accidentally overwriting existing data.
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&output)
                .with_context(|| format!("create {} (must not exist)", output.display()))?;
            file.write_all(&bytes)?;
            json!({"ok":true,"output":output,"bytes":bytes.len(),"copies":image.copies,"dither":image.dither})
        }
        Command::Print { image } => {
            let bytes = generate(&cli.wasm, &image)?;
            let session = cli.device.open(false)?;
            session.write(&bytes)?;
            json!({"sent":true,"print_completion_confirmed":false,"bytes":bytes.len(),"copies":image.copies,"dither":image.dither})
        }
        Command::Info => cli.device.open(true)?.information()?,
        Command::Status => cli.device.open(true)?.status()?,
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod golden_tests {
    use super::*;
    #[test]
    fn sdk_golden_vectors() -> Result<()> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cases: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join("tests/fixtures/cases.json"))?)?;
        for c in cases.as_array().unwrap() {
            let name = c["name"].as_str().unwrap();
            let rgba = std::fs::read(root.join(format!("tests/fixtures/{name}.rgba")))?;
            let expected = std::fs::read(root.join(format!("tests/fixtures/{name}.bin")))?;
            let actual = render::build(&root.join("assets/Nemonic.wasm"), c["width"].as_u64().unwrap() as u32,
                c["height"].as_u64().unwrap() as u32, &rgba, c["copies"].as_u64().unwrap() as u8,
                c["dither"].as_bool().unwrap())?;
            assert_eq!(actual, expected, "SDK byte mismatch: {name}");
        }
        Ok(())
    }
    #[test]
    fn rejects_invalid_input() {
        let wasm = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/Nemonic.wasm");
        assert!(render::build(&wasm, 0, 1, &[], 1, false).is_err());
        assert!(render::build(&wasm, 1, 1, &[0; 3], 1, false).is_err());
        assert!(render::build(&wasm, 1, 1, &[0; 4], 0, false).is_err());
    }
}
