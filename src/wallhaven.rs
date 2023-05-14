use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use indicatif::ProgressBar;
use lazy_static::lazy_static;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use walkdir::{DirEntry, WalkDir};

use crate::config::WallhavenConfig;

const WALLHAVEN_BASE_URL: &str = "https://wallhaven.cc/api/v1";

#[derive(Serialize, Deserialize, Debug)]
pub struct Collections {
    pub data: Vec<Collection>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Collection {
    pub id: i32,
    pub label: String,
    pub views: i32,
    pub public: i32,
    pub count: u64,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Wallpapers {
    pub meta: Page,
    pub data: Vec<Wallpaper>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Page {
    pub current_page: i32,
    pub last_page: i32,
    pub per_page: i32,
    pub total: u64,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Wallpaper {
    pub id: String,
    pub path: String,
}

lazy_static! {
    pub static ref WALLPAPERS: Mutex<HashSet<String>> = {
        let m = HashSet::new();
        Mutex::new(m)
    };
}

pub struct Wallhaven {
    pub wallhaven_config: WallhavenConfig,
    pub client: Client,
}

impl Wallhaven {
    pub fn new(wallhaven_config: WallhavenConfig) -> Self {
        Wallhaven {
            wallhaven_config,
            client: Client::new(),
        }
    }

    pub async fn get_wallhaven_collections(&self) -> Result<Collections, anyhow::Error> {
        let mut url = String::from(WALLHAVEN_BASE_URL);
        if !self.wallhaven_config.apikey.is_empty() {
            url = url + "/collections?apikey=" + &self.wallhaven_config.apikey
        } else {
            url = url + "/collections/" + &self.wallhaven_config.username;
        }
        let data = self.client.get(url).send().await?.text().await?;
        let collections: Collections = serde_json::from_str(&data).unwrap();
        anyhow::Ok(collections)
    }

    pub async fn get_collection_wallpapers(
        &self,
        collection: i32,
        page: i32,
    ) -> Result<Wallpapers, anyhow::Error> {
        let mut url = String::from(WALLHAVEN_BASE_URL);
        url = url
            + "/collections/"
            + &self.wallhaven_config.username
            + "/"
            + collection.to_string().as_str()
            + "?page="
            + page.to_string().as_str();
        if !self.wallhaven_config.apikey.is_empty() {
            url = url + "&apikey=" + &self.wallhaven_config.apikey
        }
        let data = self.client.get(url).send().await?.text().await?;
        let wallpapers = serde_json::from_str(&data).unwrap();
        anyhow::Ok(wallpapers)
    }

    pub async fn download(&self) -> Result<(), anyhow::Error> {
        let collection_names: Vec<&str> = self
            .wallhaven_config
            .collections
            .split(',')
            .map(|s| s.trim())
            .collect();
        let collections = self.get_wallhaven_collections().await?;

        for collection in collections.data {
            if !collection_names.is_empty()
                && !collection_names.contains(&collection.label.as_str())
            {
                continue;
            }
            println!("***** download collection {} ...", collection.label);
            self.download_wallpaper_from_collection(collection).await?;
        }

        clear_file_not_in_wallpapers(&self.wallhaven_config).await?;

        Ok(())
    }

    pub async fn download_wallpaper_from_collection(
        &self,
        collection: Collection,
    ) -> Result<(), anyhow::Error> {
        // Initialize page to 1 and random number generator
        let mut page = 1;
        let mut rng = rand::thread_rng();
        // Create progress bar with the total count of wallpapers in the collection
        let bar = ProgressBar::new(collection.count);
        // Loop until all pages are downloaded
        loop {
            // Get wallpapers from the current page
            let wallpapers = self.get_collection_wallpapers(collection.id, page).await?;
            // Create tasks for downloading each wallpaper
            let tasks = wallpapers
                .data
                .into_iter()
                .map(|wallpaper| download(&self.client, &self.wallhaven_config, wallpaper, &bar));
            // Wait for all tasks to complete
            futures::future::join_all(tasks).await;
            // Sleep for a random duration between 1000 and 2000 milliseconds
            sleep(Duration::from_millis(rng.gen_range(1000..2000))).await;
            // Increment page
            page = page + 1;
            // Break if we have reached the last page
            if page > wallpapers.meta.last_page {
                break;
            }
        }
        // Finish progress bar
        bar.finish();
        anyhow::Ok(())
    }
}

pub async fn download_and_save_file(
    client: &Client,
    wallpaper: &Wallpaper,
    path: &Path,
) -> Result<(), anyhow::Error> {
    let data = client.get(&wallpaper.path).send().await?.bytes().await?;
    let mut file = match File::create(&path) {
        Err(why) => panic!("couldn't create {}", why),
        Ok(file) => file,
    };
    let content = data.bytes();
    let data: Result<Vec<_>, _> = content.collect();
    file.write_all(&data.unwrap())?;
    anyhow::Ok(())
}

async fn download(
    client: &Client,
    cfg: &WallhavenConfig,
    wallpaper: Wallpaper,
    bar: &ProgressBar,
) -> Result<(), anyhow::Error> {
    let file_name = wallpaper.path.split("/").last().unwrap();
    WALLPAPERS.lock().unwrap().insert(file_name.to_string());
    let file_path = cfg.dir.to_string() + "/" + file_name;
    let file = Path::new(&file_path);
    if file.exists() && is_download_completed(&file_path) {
        bar.inc(1);
    } else {
        download_and_save_file(&client, &wallpaper, &file).await?;
        bar.inc(1);
    }

    anyhow::Ok(())
}

pub async fn clear_file_not_in_wallpapers(cfg: &WallhavenConfig) -> Result<(), anyhow::Error> {
    for entry in WalkDir::new(&cfg.dir)
        .follow_links(false)
        .max_depth(1)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
        .filter_map(|e| e.ok())
    {
        let f_name = entry.file_name().to_string_lossy();
        if f_name.to_string() != cfg.dir {
            if !WALLPAPERS.lock().unwrap().contains(&f_name.to_string()) {
                let file_path = cfg.dir.to_string() + "/" + &f_name.to_string();
                let file = Path::new(&file_path);
                if file.exists() && !file.is_dir() {
                    fs::remove_file(file)?;
                }
            }
        }
    }
    anyhow::Ok(())
}

fn get_eof_marker(file_type: &str) -> Option<Vec<u8>> {
    match file_type {
        "jpg" | "jpeg" => Some(vec![0xFF, 0xD9]),
        "png" => Some(vec![0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82]),
        "gif" => Some(vec![0x3B]),
        "bmp" => Some(vec![0x42, 0x4D]),
        "tiff" | "tif" => Some(vec![0x00, 0x00, 0x00, 0x00]),
        _ => None,
    }
}

fn is_download_completed(path: &str) -> bool {
    let file = Path::new(&path);
    let ext = file.extension().unwrap().to_str().unwrap();
    let eof_marker = get_eof_marker(ext);
    if eof_marker.is_some() {
        let eof_marker = eof_marker.unwrap();
        let file = File::open(path).expect("Failed to open file");
        let mut reader = std::io::BufReader::new(file);

        // Seek to the end of the file
        let _eof_offset = reader
            .seek(SeekFrom::End(-(eof_marker.len() as i64)))
            .unwrap();

        // Read the last few bytes and check if they match the expected EOF marker for the given type
        let mut buffer = vec![0u8; eof_marker.len()];
        reader.read_exact(&mut buffer).unwrap();
        buffer == eof_marker
    } else {
        false
    }
}

// This function checks if a directory entry is hidden or not
fn is_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with(".") || s.starts_with("$RECYCLE"))
        .unwrap_or(false)
}
