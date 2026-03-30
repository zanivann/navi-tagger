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
#[derive(Deserialize)] struct ItunesResponse { results: Vec<TrackInfo> }
#[derive(Deserialize)] #[allow(non_snake_case)] struct TrackInfo { trackName: String, artistName: String, artworkUrl100: String }
#[derive(Serialize)] struct PreviewResponse { trackName: String, artistName: String, artworkUrl: String }
#[derive(Deserialize)] struct ApplyPayload { file_path: String, title: String, artist: String, artwork_url: String }

fn main() {
    thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let app = Router::new()
                .route("/", get(serve_ui))
                .route("/api/preview", get(api_preview))
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
                .icon(app.default_window_icon().unwrap().clone()) // Força o uso do ícone padrão
                .menu(&menu)
                .show_menu_on_left_click(true) // Essencial para macOS
                .on_menu_event(|app, event| {
                    if event.id == "quit" { app.exit(0); }
                })
                .build(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Falha nativa");
}

async fn serve_ui() -> Html<&'static str> { Html(include_str!("../../src/index.html")) }

async fn api_preview(Query(q): Query<PreviewQuery>) -> impl IntoResponse {
    let c = Client::new();
    let url = format!("https://itunes.apple.com/lookup?id={}", q.id);
    if let Ok(res) = c.get(&url).send().await {
        if let Ok(js) = res.json::<ItunesResponse>().await {
            if let Some(t) = js.results.first() {
                return Json(PreviewResponse {
                    trackName: t.trackName.clone(),
                    artistName: t.artistName.clone(),
                    artworkUrl: t.artworkUrl100.replace("100x100bb", "600x600bb"),
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
    tag.push_picture(Picture::new_unchecked(PictureType::CoverFront, Some(MimeType::Jpeg), None, img.to_vec()));
    f.insert_tag(tag);
    f.save_to_path(&p.file_path, WriteOptions::new()).unwrap();
    axum::http::StatusCode::OK
}

async fn api_browse() -> impl IntoResponse {
    let out = Command::new("osascript").arg("-e").arg("POSIX path of (choose file)").output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}