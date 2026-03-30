use axum::{Router, extract::{Json, Query}, response::{Html, IntoResponse}, routing::{get, post}};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::{ItemKey, Tag};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::thread;

#[derive(Deserialize)] struct PreviewQuery { id: String }
#[derive(Deserialize)] struct SearchQuery { term: String }
#[derive(Deserialize)] struct FilePathQuery { path: String }
#[derive(Deserialize)] struct ItunesResponse { results: Vec<TrackInfo> }

#[derive(Deserialize)] 
#[allow(non_snake_case)] 
struct TrackInfo { 
    trackId: Option<u64>,
    trackName: String, 
    artistName: String, 
    collectionName: Option<String>,
    primaryGenreName: Option<String>,
    releaseDate: Option<String>,
    trackNumber: Option<u32>,
    trackCount: Option<u32>,
    discNumber: Option<u32>,
    discCount: Option<u32>,
    copyright: Option<String>,
    artworkUrl100: String 
}

#[derive(Serialize)] 
struct PreviewResponse { 
    trackName: String, artistName: String, album: String, genre: String,
    year: String, fullDate: String, label: String,
    trackNumber: u32, trackTotal: u32, discNumber: u32, discTotal: u32, artworkUrl: String 
}

#[derive(Serialize)]
struct ExistingTags { title: String, artist: String }

#[derive(Deserialize)] 
struct ApplyPayload { 
    file_path: String, title: String, artist: String, album: String,
    genre: String, year: String, label: String, track_num: String,
    track_total: String, disc_num: String, disc_total: String, artwork_url: String 
}

fn main() {
    thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let app = Router::new()
                .route("/", get(serve_ui))
                .route("/api/preview", get(api_preview))
                .route("/api/search", get(api_search))
                .route("/api/read_tags", get(api_read_tags))
                .route("/api/apply", post(api_apply))
                .route("/api/browse", get(api_browse));
            let l = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
            axum::serve(l, app).await.unwrap();
        });
    });

    tauri::Builder::default()
        .setup(|app| {
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::TrayIconBuilder;
            let quit_i = MenuItem::with_id(app, "quit", "Fechar", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_i])?;
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| { if event.id == "quit" { app.exit(0); } })
                .build(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Falha nativa");
}

async fn serve_ui() -> Html<&'static str> { Html(include_str!("../../src/index.html")) }

async fn api_read_tags(Query(q): Query<FilePathQuery>) -> impl IntoResponse {
    if let Ok(f) = Probe::open(&q.path).unwrap().read() {
        if let Some(tag) = f.primary_tag() {
            return Json(ExistingTags {
                title: tag.get_string(&ItemKey::TrackTitle).unwrap_or("").to_string(),
                artist: tag.get_string(&ItemKey::TrackArtist).unwrap_or("").to_string(),
            }).into_response();
        }
    }
    Json(ExistingTags { title: "".into(), artist: "".into() }).into_response()
}

async fn api_search(Query(q): Query<SearchQuery>) -> impl IntoResponse {
    let c = Client::new();
    let url = format!("https://itunes.apple.com/search?term={}&entity=song&limit=1", q.term);
    if let Ok(res) = c.get(&url).send().await {
        if let Ok(js) = res.json::<ItunesResponse>().await {
            if let Some(t) = js.results.first() {
                return t.trackId.map(|id| id.to_string()).unwrap_or_default();
            }
        }
    }
    String::new()
}

async fn api_preview(Query(q): Query<PreviewQuery>) -> impl IntoResponse {
    let c = Client::new();
    let url = format!("https://itunes.apple.com/lookup?id={}", q.id);
    if let Ok(res) = c.get(&url).send().await {
        if let Ok(js) = res.json::<ItunesResponse>().await {
            if let Some(t) = js.results.first() {
                let full_date = t.releaseDate.clone().unwrap_or_default().split('T').next().unwrap_or("").to_string();
                return Json(PreviewResponse {
                    trackName: t.trackName.clone(),
                    artistName: t.artistName.clone(),
                    album: t.collectionName.clone().unwrap_or_default(),
                    genre: t.primaryGenreName.clone().unwrap_or_default(),
                    year: full_date.chars().take(4).collect(),
                    fullDate: full_date,
                    label: t.copyright.clone().unwrap_or_default(),
                    trackNumber: t.trackNumber.unwrap_or(0),
                    trackTotal: t.trackCount.unwrap_or(0),
                    discNumber: t.discNumber.unwrap_or(1),
                    discTotal: t.discCount.unwrap_or(1),
                    artworkUrl: t.artworkUrl100.replace("100x100bb", "1000x1000bb"),
                }).into_response();
            }
        }
    }
    axum::http::StatusCode::NOT_FOUND.into_response()
}

async fn api_apply(Json(p): Json<ApplyPayload>) -> impl IntoResponse {
    let c = Client::new();
    let img = c.get(&p.artwork_url).send().await.unwrap().bytes().await.unwrap();
    let mut f = Probe::open(&p.file_path).unwrap().read().unwrap();
    f.clear();
    let mut tag = Tag::new(f.file_type().primary_tag_type());
    tag.insert_text(ItemKey::TrackTitle, p.title);
    tag.insert_text(ItemKey::TrackArtist, p.artist);
    tag.insert_text(ItemKey::AlbumTitle, p.album);
    tag.insert_text(ItemKey::Genre, p.genre);
    tag.insert_text(ItemKey::RecordingDate, p.year);
    tag.insert_text(ItemKey::Publisher, p.label);
    tag.insert_text(ItemKey::TrackNumber, p.track_num);
    tag.insert_text(ItemKey::TrackTotal, p.track_total);
    tag.insert_text(ItemKey::DiscNumber, p.disc_num);
    tag.insert_text(ItemKey::DiscTotal, p.disc_total);
    tag.push_picture(Picture::new_unchecked(PictureType::CoverFront, Some(MimeType::Jpeg), None, img.to_vec()));
    f.insert_tag(tag);
    f.save_to_path(&p.file_path, WriteOptions::new()).unwrap();
    axum::http::StatusCode::OK
}

async fn api_browse() -> impl IntoResponse {
    let out = Command::new("osascript").arg("-e").arg("POSIX path of (choose file)").output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}