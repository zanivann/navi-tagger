// Previne a abertura do console no Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::{ItemKey, Tag};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Serialize, Deserialize)]
pub struct FullMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub year: String,
    pub full_date: String,
    pub label: String,
    pub track_num: String,
    pub track_total: String,
    pub disc_num: String,
    pub disc_total: String,
    pub artwork_url: String,
}

#[derive(Serialize)]
pub struct ExistingTags {
    pub title: String,
    pub artist: String,
}

#[derive(Deserialize)]
struct ItunesResponse { results: Vec<TrackInfo> }

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct TrackInfo {
    trackId: Option<u64>,
    trackName: Option<String>,
    artistName: Option<String>,
    collectionName: Option<String>,
    primaryGenreName: Option<String>,
    releaseDate: Option<String>,
    trackNumber: Option<u32>,
    trackCount: Option<u32>,
    discNumber: Option<u32>,
    discCount: Option<u32>,
    copyright: Option<String>,
    artworkUrl100: Option<String>,
}

#[tauri::command]
async fn browse_file() -> Result<String, String> {
    let out = Command::new("osascript")
        .arg("-e")
        .arg("POSIX path of (choose file)")
        .output()
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[tauri::command]
async fn read_existing_tags(path: String) -> Result<ExistingTags, String> {
    let f = Probe::open(&path).map_err(|e| e.to_string())?.read().map_err(|e| e.to_string())?;
    if let Some(tag) = f.primary_tag() {
        return Ok(ExistingTags {
            title: tag.get_string(&ItemKey::TrackTitle).unwrap_or("").to_string(),
            artist: tag.get_string(&ItemKey::TrackArtist).unwrap_or("").to_string(),
        });
    }
    Ok(ExistingTags { title: "".into(), artist: "".into() })
}

#[tauri::command]
async fn search_itunes(term: String) -> Result<String, String> {
    let c = Client::new();
    let url = format!("https://itunes.apple.com/search?term={}&entity=song&limit=1", term);
    let res = c.get(&url).send().await.map_err(|e| e.to_string())?
        .json::<ItunesResponse>().await.map_err(|e| e.to_string())?;
    
    if let Some(t) = res.results.first() {
        return Ok(t.trackId.map(|id| id.to_string()).unwrap_or_default());
    }
    Err("Nenhum resultado encontrado".into())
}

#[tauri::command]
async fn get_preview(id: String) -> Result<FullMetadata, String> {
    let c = Client::new();
    let url = format!("https://itunes.apple.com/lookup?id={}", id);
    let res = c.get(&url).send().await.map_err(|e| e.to_string())?
        .json::<ItunesResponse>().await.map_err(|e| e.to_string())?;
    
    if let Some(t) = res.results.first() {
        let full_date = t.releaseDate.as_deref().unwrap_or("").split('T').next().unwrap_or("").to_string();
        return Ok(FullMetadata {
            title: t.trackName.clone().unwrap_or_default(),
            artist: t.artistName.clone().unwrap_or_default(),
            album: t.collectionName.clone().unwrap_or_default(),
            genre: t.primaryGenreName.clone().unwrap_or_default(),
            year: full_date.chars().take(4).collect(),
            full_date,
            label: t.copyright.clone().unwrap_or_default(),
            track_num: t.trackNumber.unwrap_or(0).to_string(),
            track_total: t.trackCount.unwrap_or(0).to_string(),
            disc_num: t.discNumber.unwrap_or(1).to_string(),
            disc_total: t.discCount.unwrap_or(1).to_string(),
            artwork_url: t.artworkUrl100.clone().unwrap_or_default().replace("100x100bb", "1000x1000bb"),
        });
    }
    Err("ID inválido".into())
}

#[tauri::command]
async fn apply_tags(path: String, data: FullMetadata) -> Result<(), String> {
    let c = Client::new();
    let img = c.get(&data.artwork_url).send().await.map_err(|e| e.to_string())?
        .bytes().await.map_err(|e| e.to_string())?;
    
    let mut f = Probe::open(&path).map_err(|e| e.to_string())?.read().map_err(|e| e.to_string())?;
    f.clear();
    
    let mut tag = Tag::new(f.file_type().primary_tag_type());
    tag.insert_text(ItemKey::TrackTitle, data.title);
    tag.insert_text(ItemKey::TrackArtist, data.artist);
    tag.insert_text(ItemKey::AlbumTitle, data.album);
    tag.insert_text(ItemKey::Genre, data.genre);
    tag.insert_text(ItemKey::RecordingDate, data.year);
    tag.insert_text(ItemKey::Publisher, data.label);
    tag.insert_text(ItemKey::TrackNumber, data.track_num);
    tag.insert_text(ItemKey::TrackTotal, data.track_total);
    tag.insert_text(ItemKey::DiscNumber, data.disc_num);
    tag.insert_text(ItemKey::DiscTotal, data.disc_total);
    
    tag.push_picture(Picture::new_unchecked(PictureType::CoverFront, Some(MimeType::Jpeg), None, img.to_vec()));
    
    f.insert_tag(tag);
    f.save_to_path(&path, WriteOptions::new()).map_err(|e| e.to_string())?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{TrayIconBuilder, TrayIconEvent};
            use tauri::Manager;

            let show_i = MenuItem::with_id(app, "show", "Mostrar Interface", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Fechar", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "quit" => { app.exit(0); }
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Se a janela existir, mostre. Se não, não morra.
            if let Some(main_window) = app.get_webview_window("main") {
                let _ = main_window.show();
            }

            Ok(())
        })
        })
        .invoke_handler(tauri::generate_handler![
            browse_file, 
            read_existing_tags, 
            search_itunes, 
            get_preview, 
            apply_tags
        ])
        .run(tauri::generate_context!())
        .expect("Erro na fundição nativa");
}