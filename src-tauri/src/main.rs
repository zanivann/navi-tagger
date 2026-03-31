#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::{ItemKey, Tag, Accessor};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::process::Command;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use walkdir::WalkDir;

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
    pub album: String,
    pub genre: String,
    pub year: String,
    pub track_num: String,
    pub track_total: String,
    pub disc_num: String,
    pub disc_total: String,
    pub cover_base64: Option<String>,
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
async fn browse_folder() -> Result<String, String> {
    let out = Command::new("osascript")
        .arg("-e")
        .arg("POSIX path of (choose folder)")
        .output()
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[tauri::command]
async fn scan_directory(path: String) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    let valid_extensions = ["flac", "mp3", "m4a", "wav", "aac"];

    for entry in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_file() {
            if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                if valid_extensions.contains(&ext.to_lowercase().as_str()) {
                    files.push(p.to_string_lossy().to_string());
                }
            }
        }
    }
    
    if files.is_empty() {
        return Err("Nenhum arquivo de áudio encontrado nesta pasta ou subpastas.".into());
    }
    
    Ok(files)
}

#[tauri::command]
async fn read_existing_tags(path: String) -> Result<ExistingTags, String> {
    let mut f = Probe::open(&path).map_err(|e| e.to_string())?.read().map_err(|e| e.to_string())?;
    
    let mut tags = ExistingTags {
        title: "".into(), artist: "".into(), album: "".into(), genre: "".into(),
        year: "".into(), track_num: "".into(), track_total: "".into(),
        disc_num: "".into(), disc_total: "".into(), cover_base64: None,
    };

    if let Some(tag) = f.primary_tag() {
        tags.title = tag.title().as_deref().unwrap_or("").to_string();
        tags.artist = tag.artist().as_deref().unwrap_or("").to_string();
        tags.album = tag.album().as_deref().unwrap_or("").to_string();
        tags.genre = tag.genre().as_deref().unwrap_or("").to_string();
        tags.year = tag.get_string(&ItemKey::RecordingDate).unwrap_or("").to_string();
        tags.track_num = tag.track().map(|v| v.to_string()).unwrap_or_default();
        tags.track_total = tag.track_total().map(|v| v.to_string()).unwrap_or_default();
        tags.disc_num = tag.disk().map(|v| v.to_string()).unwrap_or_default();
        tags.disc_total = tag.disk_total().map(|v| v.to_string()).unwrap_or_default();

        if let Some(pic) = tag.pictures().first() {
            let b64 = STANDARD.encode(pic.data());
            let mime = pic.mime_type().unwrap_or(&MimeType::Jpeg).as_str();
            tags.cover_base64 = Some(format!("data:{};base64,{}", mime, b64));
        }
    }
    Ok(tags)
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
    let mut f = Probe::open(&path).map_err(|e| e.to_string())?.read().map_err(|e| e.to_string())?;
    
    let tag_type = f.file_type().primary_tag_type();
    let mut tag = f.primary_tag_mut().cloned().unwrap_or_else(|| Tag::new(tag_type));
    
    if !data.title.is_empty() { tag.insert_text(ItemKey::TrackTitle, data.title); }
    if !data.artist.is_empty() { tag.insert_text(ItemKey::TrackArtist, data.artist); }
    if !data.album.is_empty() { tag.insert_text(ItemKey::AlbumTitle, data.album); }
    if !data.genre.is_empty() { tag.insert_text(ItemKey::Genre, data.genre); }
    if !data.year.is_empty() { tag.insert_text(ItemKey::RecordingDate, data.year); }
    if !data.label.is_empty() { tag.insert_text(ItemKey::Publisher, data.label); }
    if !data.track_num.is_empty() { tag.insert_text(ItemKey::TrackNumber, data.track_num); }
    if !data.track_total.is_empty() { tag.insert_text(ItemKey::TrackTotal, data.track_total); }
    if !data.disc_num.is_empty() { tag.insert_text(ItemKey::DiscNumber, data.disc_num); }
    if !data.disc_total.is_empty() { tag.insert_text(ItemKey::DiscTotal, data.disc_total); }
    
    if !data.artwork_url.is_empty() {
        let c = Client::new();
        let img = c.get(&data.artwork_url).send().await.map_err(|e| e.to_string())?
            .bytes().await.map_err(|e| e.to_string())?;
        
        tag.remove_picture_type(PictureType::CoverFront);
        tag.push_picture(Picture::new_unchecked(PictureType::CoverFront, Some(MimeType::Jpeg), None, img.to_vec()));
    }
    
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

            if let Some(main_window) = app.get_webview_window("main") {
                let _ = main_window.show();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            browse_file, 
            browse_folder,
            scan_directory,
            read_existing_tags, 
            search_itunes, 
            get_preview, 
            apply_tags
        ])
        .run(tauri::generate_context!())
        .expect("Erro na fundição nativa");
}