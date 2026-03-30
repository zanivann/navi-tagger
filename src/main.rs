use axum::{
    Router,
    extract::{Json, Query},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::{ItemKey, Tag, TagExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::process::Command;

#[derive(Deserialize)]
struct PreviewQuery {
    id: String,
}

#[derive(Deserialize)]
struct ItunesResponse {
    results: Vec<TrackInfo>,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct TrackInfo {
    trackName: String,
    artistName: String,
    collectionName: String,
    artworkUrl100: String,
    primaryGenreName: String,
    releaseDate: String,
    trackNumber: u32,
    trackCount: u32,
    discNumber: u32,
    copyright: Option<String>,
    trackExplicitness: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
struct PreviewResponse {
    trackName: String,
    artistName: String,
    collectionName: String,
    artworkUrl: String,
    genre: String,
    year: String,
    trackNumber: u32,
    trackTotal: u32,
    discNumber: u32,
    copyright: String,
    isExplicit: bool,
}

#[derive(Deserialize)]
struct ApplyPayload {
    file_path: String,
    title: String,
    artist: String,
    album: String,
    artwork_url: String,
    genre: String,
    year: String,
    track_number: u32,
    track_total: u32,
    disc_number: u32,
    copyright: String,
    comment: String,
}

#[tokio::main]
async fn main() {
    println!("Jarvis: Iniciando Servidor Navi-Tagger na porta 3000...");
    println!("Acesse: http://localhost:3000");

    let app = Router::new()
        .route("/", get(serve_ui))
        .route("/api/preview", get(api_preview))
        .route("/api/apply", post(api_apply))
        .route("/api/browse", get(api_browse));

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn serve_ui() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

async fn api_preview(Query(params): Query<PreviewQuery>) -> impl IntoResponse {
    let client = Client::new();
    let url = format!("https://itunes.apple.com/lookup?id={}", params.id);

    if let Ok(res) = client.get(&url).send().await {
        if let Ok(json) = res.json::<ItunesResponse>().await {
            if let Some(track) = json.results.first() {
                let hires_url = track.artworkUrl100.replace("100x100bb", "600x600bb");
                let year = track.releaseDate.chars().take(4).collect::<String>();

                return Json(PreviewResponse {
                    trackName: track.trackName.clone(),
                    artistName: track.artistName.clone(),
                    collectionName: track.collectionName.clone(),
                    artworkUrl: hires_url,
                    genre: track.primaryGenreName.clone(),
                    year,
                    trackNumber: track.trackNumber,
                    trackTotal: track.trackCount,
                    discNumber: track.discNumber,
                    copyright: track.copyright.clone().unwrap_or_default(),
                    isExplicit: track.trackExplicitness == "explicit",
                })
                .into_response();
            }
        }
    }
    axum::http::StatusCode::NOT_FOUND.into_response()
}

async fn api_apply(Json(payload): Json<ApplyPayload>) -> impl IntoResponse {
    let client = Client::new();

    let img_bytes = match client.get(&payload.artwork_url).send().await {
        Ok(resp) => resp.bytes().await.unwrap_or_default(),
        Err(_) => return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let mut tagged_file = match Probe::open(&payload.file_path).and_then(|p| p.read()) {
        Ok(file) => file,
        Err(_) => return axum::http::StatusCode::BAD_REQUEST.into_response(),
    };

    tagged_file.clear();

    let native_tag_type = tagged_file.file_type().primary_tag_type();
    let mut tag = Tag::new(native_tag_type);

    tag.insert_text(ItemKey::TrackTitle, payload.title);
    tag.insert_text(ItemKey::TrackArtist, payload.artist);
    tag.insert_text(ItemKey::AlbumTitle, payload.album);
    tag.insert_text(ItemKey::Genre, payload.genre);
    tag.insert_text(ItemKey::RecordingDate, payload.year);
    tag.insert_text(ItemKey::TrackNumber, payload.track_number.to_string());
    tag.insert_text(ItemKey::TrackTotal, payload.track_total.to_string());
    tag.insert_text(ItemKey::DiscNumber, payload.disc_number.to_string());
    tag.insert_text(ItemKey::CopyrightMessage, payload.copyright);
    tag.insert_text(ItemKey::Comment, payload.comment);

    let picture = Picture::new_unchecked(
        PictureType::CoverFront,
        Some(MimeType::Jpeg),
        None,
        img_bytes.to_vec(),
    );
    tag.push_picture(picture);

    tagged_file.insert_tag(tag);

    match tagged_file.save_to_path(&payload.file_path, WriteOptions::new()) {
        Ok(_) => axum::http::StatusCode::OK.into_response(),
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn api_browse() -> impl IntoResponse {
    if cfg!(target_os = "macos") {
        let output = Command::new("osascript")
            .arg("-e")
            .arg("POSIX path of (choose file)")
            .output();

        if let Ok(cmd_res) = output {
            if cmd_res.status.success() {
                let path = String::from_utf8_lossy(&cmd_res.stdout).trim().to_string();
                return path.into_response();
            }
        }
    }
    String::new().into_response()
}
