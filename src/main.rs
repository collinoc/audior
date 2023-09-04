use anyhow::Result;
use clap::Parser;
use clap::ValueEnum;
use std::io::Write;

#[derive(Parser)]
struct Opts {
    /// Specify file output location
    #[clap(short, long)]
    output: Option<String>,
    /// Default device to listen to
    #[clap(short, long)]
    listen: Listen,
    /// Use recorded audio as input
    #[clap(long)]
    loopback: bool,
    /// Delay recording (seconds)
    #[clap(short, long)]
    delay: Option<usize>,
}

#[derive(ValueEnum, Clone, PartialEq)]
enum Listen {
    In,
    Out,
}

fn main() -> Result<()> {
    let options = Opts::parse();
    let mut stdout = std::io::stdout();

    let device = if options.listen == Listen::In {
        audior::DeviceBuilder::new_default_input()?
    } else {
        audior::DeviceBuilder::new_default_output()?
    };

    if let Ok(name) = device.name() {
        eprintln!("Listening to {name}");
    }

    let mut stream = audior::StreamBuilder::new(device)?;

    if options.loopback {
        stream.from_input();
    }

    let writer = stream.write_wav(options.output.unwrap_or_else(|| "out.wav".into()))?;

    if let Some(delay) = options.delay {
        write!(&stdout, "Recording in ")?;
        stdout.flush()?;

        for i in (1..=delay).rev() {
            write!(&stdout, "{i} ")?;
            stdout.flush()?;
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        println!();
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
