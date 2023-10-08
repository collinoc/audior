use anyhow::Result;

pub mod cli;

fn main() -> Result<()> {
    cli::cli_main()?;

    Ok(())
}
