

use anyhow::Ok;
use clap::Parser;


use wallhaven::config::{Cli, SubCommand, WallhavenConfig};
use wallhaven::wallhaven::Wallhaven;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli: Cli = Cli::parse();

    let result = match cli.subcommand {
        SubCommand::Download(args) => {
            println!("Start download wallpaper ...");
            let config = WallhavenConfig::load(&cli.config_path, &args);
            let wallhaven = Wallhaven::new(config);
            wallhaven.download().await?;
        }
    };

    println!("Done!");

    Ok(result)
}

