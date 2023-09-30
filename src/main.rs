use anyhow::Result;
use clap::Parser;
use clap::ValueEnum;
use std::io::Write;
use std::thread;
use std::time::Duration;

#[derive(Parser, Debug)]
#[clap(version)]
struct Opts {
    /// Specify file output location (default: out.wav)
    #[clap(short, long)]
    output: Option<String>,
    /// Default device to listen to
    #[clap(short, long)]
    listen: Listen,
    /// Use an output stream as input
    /// (No effect while listening to input devices)
    #[clap(long, verbatim_doc_comment)]
    loopback: bool,
    /// Delay recording (seconds)
    #[clap(short, long)]
    delay: Option<usize>,
    /// Don't play sound during delay countdown
    #[clap(short, long)]
    quiet: bool,
}

#[derive(ValueEnum, Debug, Clone, PartialEq)]
enum Listen {
    In,
    Out,
}

const BACKSPACE: &str = "\x08";
const ALERT: &str = "\x07";

fn main() -> Result<()> {
    let options = Opts::parse();
    let mut stdout = std::io::stdout();

    let device = if options.listen == Listen::In {
        audiort::DeviceBuilder::new_default_input()?
    } else {
        audiort::DeviceBuilder::new_default_output()?
    };

    if let Ok(name) = device.name() {
        eprintln!("Listening to {name}");
    }

    let device_kind = device.kind();
    let mut stream = audiort::StreamBuilder::from(device)?;

    if device_kind == audiort::Device::Output && options.loopback {
        stream.as_input();
    }

    let writer = stream.write_wav(options.output.unwrap_or_else(|| "out.wav".into()))?;

    if let Some(delay) = options.delay {
        write!(&stdout, "Recording in  ")?;
        stdout.flush()?;
        let alert = if options.quiet { "" } else { ALERT };

        for i in (1..=delay).rev() {
            write!(&stdout, "{BACKSPACE}{i}{alert}")?;
            stdout.flush()?;
            thread::sleep(Duration::from_secs(1));
        }

        print!("\r");
        thread::sleep(Duration::from_millis(500)); // Don't record the last ding
    }

    stream.play()?;

    write!(&stdout, "Press `Enter` to stop recording... ")?;

    stdout.flush()?;

    if std::io::stdin().read_line(&mut String::new()).is_ok() {
        if let Ok(mut wlock) = writer.lock() {
            if let Some(writer) = wlock.take() {
                writer.finalize()?;
                eprintln!("Written to out.wav");
            }
        }
    }

    Ok(())
}
