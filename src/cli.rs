use anyhow::{Context, Result};
use audiort::WavExt;
use clap::{Parser, Subcommand, ValueEnum};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Parser, Debug)]
#[clap(version)]
pub struct Opts {
    #[command(subcommand)]
    command: Command,
    /// Delay recording (seconds)
    #[clap(short, long)]
    delay: Option<usize>,
    /// Don't play sound during delay countdown
    #[clap(short, long)]
    quiet: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Redirect audio stream to another stream
    Loopback {
        /// Type of device to use as input stream
        kind: ListenKind,
    },
    /// Record audio stream to a wav file
    Record {
        /// Type of device to use as input stream
        kind: ListenKind,
        /// Specify file output location (default: ~/Music/audiort/out.wav)
        #[clap(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq)]
pub enum ListenKind {
    /// Listen to input device
    Input,
    /// Listen to output device
    Output,
}

pub fn cli_main() -> Result<()> {
    let options = Opts::parse();

    match options.command {
        Command::Loopback { kind } => {
            let device = match kind {
                ListenKind::Input => audiort::DeviceBuilder::new_default_input()?,
                ListenKind::Output => audiort::DeviceBuilder::new_default_output()?,
            };

            if let Ok(name) = device.name() {
                eprintln!("Listening to {name}");
            }

            let mut stream = audiort::StreamBuilder::from(device)?;

            // Trick it into using the device kind as inverse
            match kind {
                ListenKind::Input => stream.as_input(),
                ListenKind::Output => stream.as_output(),
            };

            do_delay(options.delay, options.quiet)?;

            // stream.play()?;

            // println!("Press `Enter` to stop recording...");
        }
        Command::Record { kind, output } => {
            let path = output.unwrap_or_else(|| "out.wav".into());

            let device = match kind {
                ListenKind::Input => audiort::DeviceBuilder::new_default_input()?,
                ListenKind::Output => audiort::DeviceBuilder::new_default_output()?,
            };

            if let Ok(name) = device.name() {
                eprintln!("Listening to {name}");
            }

            let writer = hound::WavWriter::create(path.clone(), device.config().as_wav_spec())
                .context("failed to creat wav writer")?;

            let mut stream = audiort::StreamBuilder::from(device)?;

            do_delay(options.delay, options.quiet)?;

            let writer = Arc::new(Mutex::new(Some(writer)));

            let wav_writer = Arc::clone(&writer);

            stream
                .with_reader(move |data| {
                    if let Ok(mut wlock) = wav_writer.lock() {
                        if let Some(writer) = wlock.as_mut() {
                            for d in data.bytes() {
                                writer
                                    .write_sample(*d as i8)
                                    .expect("failed to write sample");
                            }
                        }
                    }
                })
                .context("stream creation failed")?;

            stream.play()?;

            if std::io::stdin().read_line(&mut String::new()).is_ok() {
                if let Ok(mut wlock) = writer.lock() {
                    if let Some(writer) = wlock.take() {
                        writer.finalize()?;
                        eprintln!("Written to {}", path.display());
                    }
                }
            }
        }
    };

    Ok(())
}

const BACKSPACE: &str = "\x08";
const ALERT: &str = "\x07";

fn do_delay(delay: Option<usize>, quiet: bool) -> std::io::Result<()> {
    if let Some(delay) = delay {
        let mut stdout = std::io::stdout();
        let alert = if quiet { "" } else { ALERT };

        write!(&stdout, "Recording in  ")?;
        stdout.flush()?;

        for count in (1..=delay).rev() {
            write!(&stdout, "{BACKSPACE}{count}{alert}")?;
            stdout.flush()?;
            thread::sleep(Duration::from_secs(1));
        }

        print!("\r");
        // Don't record the last ding
        thread::sleep(Duration::from_millis(500));
    }

    Ok(())
}

type WavWriter = Arc<Mutex<Option<hound::WavWriter<BufWriter<File>>>>>;

fn write_wav_data(data: &[u8], writer: &WavWriter) {
    if let Ok(mut wlock) = writer.lock() {
        if let Some(writer) = wlock.as_mut() {
            for &d in data {
                writer
                    .write_sample(d as i8)
                    .expect("failed to write sample");
            }
        }
    }
}
