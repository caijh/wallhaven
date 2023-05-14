use std::path::Path;


use clap::{arg, Parser};

use serde::{Deserialize, Serialize};

use config::Config;

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct WallhavenConfig {
    pub apikey: String,
    pub username: String,
    pub collections: String,
    pub dir: String,
}

#[derive(clap::Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(long, required = false)]
    pub config_path: Option<String>,
    #[command(subcommand)]
    pub subcommand: SubCommand,
}

#[derive(clap::Subcommand, Debug)]
pub enum SubCommand {
    Download(DownloadArgs),
}

#[derive(Parser, Debug)]
pub struct DownloadArgs {
    #[arg(long)]
    pub apikey: Option<String>,
    #[arg(long)]
    pub username: Option<String>,
    #[arg(long)]
    pub collections: Option<String>,
    #[arg(long)]
    pub dir: Option<String>,
}

impl WallhavenConfig {
    pub fn load(config_path: &Option<String>, download_args: &DownloadArgs) -> WallhavenConfig {
        let config_path = if let Some(path) = config_path {
            path
        } else {
            "./config.toml"
        };
        let mut config = WallhavenConfig::default();
        let config_path = Path::new(config_path);
        if config_path.exists() {
            let settings = Config::builder()
                .add_source(config::File::from(Path::new(config_path)))
                .build()
                .unwrap_or_else(|_| panic!("[!] Fail to load config file {}", config_path.display()));
            let cfg = settings.try_deserialize::<WallhavenConfig>().unwrap();
            config = cfg;
        }
        if let Some(apikey) = &download_args.apikey {
            config.apikey = apikey.trim().to_string();
        }
        if let Some(username) = &download_args.username {
            config.username = username.clone();
        }
        if let Some(collections) = &download_args.collections {
            config.collections = collections.clone();
        }
        if let Some(dir) = &download_args.dir {
            config.dir = dir.clone();
        }
        if config.dir.is_empty() {
            config.dir = "./".to_string();
        }
        config
    }
}
