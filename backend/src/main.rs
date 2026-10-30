use std::path::PathBuf;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    routing::{get, patch, post, put},
};
use serde_json::{Value, json};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod error;
mod files;
mod folders;
mod models;
mod tags;
mod thumbnail;

/// Shared application state handed to every request handler.
#[derive(Clone)]
pub struct AppState {
    db: SqlitePool,
    uploads_dir: PathBuf,
    #[allow(dead_code)] // used starting in phase 2 (thumbnails)
    thumbnails_dir: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    // All persistent state lives under DATA_DIR (defaults to the repo's ./data).
    let data_dir = PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "../data".into()));
    let uploads_dir = data_dir.join("uploads");
    let thumbnails_dir = data_dir.join("thumbnails");
    tokio::fs::create_dir_all(&uploads_dir).await?;
    tokio::fs::create_dir_all(&thumbnails_dir).await?;

    let db = init_db(&data_dir.join("app.db")).await?;

    let state = AppState {
        db,
        uploads_dir,
        thumbnails_dir,
    };

    let app = build_router(state);

    let addr = "127.0.0.1:3000";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("backend listening on http://{addr}");
    axum::serve(listener, app).await?;

    Ok(())
}

/// Assemble the application router. Extracted so tests can drive it in-process.
fn build_router(state: AppState) -> Router {
    // Permissive CORS for local dev (Vite runs on a different port).
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/health", get(health))
        // The 2 GiB body limit is scoped to this route only. Applied globally it
        // would also let the JSON endpoints below buffer 2 GiB in memory before
        // deserializing; they keep axum's 2 MB default.
        .route(
            "/api/files",
            get(files::list)
                .post(files::upload)
                .layer(DefaultBodyLimit::max(files::MAX_UPLOAD_BYTES as usize)),
        )
        // Static segments must be declared before the `{id}` param route.
        .route("/api/files/move", post(files::move_files))
        .route("/api/files/delete", post(files::delete_many))
        .route(
            "/api/files/{id}",
            get(files::get_one)
                .delete(files::delete)
                .patch(files::rename),
        )
        .route("/api/files/{id}/raw", get(files::serve_raw))
        .route("/api/files/{id}/thumb", get(files::serve_thumb))
        .route("/api/files/{id}/tags", put(tags::set_for_file))
        .route("/api/folders", get(folders::list).post(folders::create))
        .route(
            "/api/folders/{id}",
            patch(folders::rename).delete(folders::delete),
        )
        .route("/api/tags", get(tags::list))
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "global_viewer_backend=debug,tower_http=info,info".into());
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Open (creating if needed) the SQLite database in WAL mode and run migrations.
async fn init_db(path: &std::path::Path) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);

    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations").run(&db).await?;
    tracing::info!("database ready at {}", path.display());

    Ok(db)
}

/// Liveness + DB connectivity check.
async fn health(State(state): State<AppState>) -> Json<Value> {
    let db_ok = sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.db)
        .await
        .is_ok();
    Json(json!({ "status": "ok", "db": db_ok }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use tower::ServiceExt;

    /// Build an AppState backed by a throwaway temp directory + SQLite file.
    async fn test_state() -> (AppState, PathBuf) {
        let base = std::env::temp_dir().join(format!("gv-test-{}", uuid::Uuid::new_v4()));
        let uploads_dir = base.join("uploads");
        let thumbnails_dir = base.join("thumbnails");
        tokio::fs::create_dir_all(&uploads_dir).await.unwrap();
        tokio::fs::create_dir_all(&thumbnails_dir).await.unwrap();
        let db = init_db(&base.join("app.db")).await.unwrap();
        (
            AppState {
                db,
                uploads_dir,
                thumbnails_dir,
            },
            base,
        )
    }

    fn multipart_body(
        boundary: &str,
        filename: &str,
        content_type: &str,
        content: &[u8],
    ) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
        body.extend_from_slice(content);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        body
    }

    #[tokio::test]
    async fn upload_list_serve_range_delete_cycle() {
        let (state, base) = test_state().await;
        let app = build_router(state);

        let boundary = "TESTBOUNDARY";
        let content = b"hello global viewer";
        let body = multipart_body(boundary, "hello.txt", "text/plain", content);

        // --- Upload ---
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/files")
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let uploaded: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(uploaded[0]["original_name"], "hello.txt");
        assert_eq!(uploaded[0]["size_bytes"], content.len() as i64);
        let id = uploaded[0]["id"].as_str().unwrap().to_string();

        // --- List ---
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/files")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let listed: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(listed.len(), 1);

        // --- Serve raw (full) ---
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/files/{id}/raw"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&bytes[..], content);

        // --- Serve raw (Range request → 206 Partial Content) ---
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/files/{id}/raw"))
                    .header(header::RANGE, "bytes=0-4")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&bytes[..], &content[0..5]);

        // --- Delete ---
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/files/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);

        // --- Gone now ---
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/files/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        let _ = tokio::fs::remove_dir_all(base).await;
    }

    #[tokio::test]
    async fn image_upload_generates_and_serves_thumbnail() {
        let (state, base) = test_state().await;
        let app = build_router(state);

        // Encode a small real PNG so the image crate can actually decode it.
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::new(8, 8))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let body = multipart_body("B", "pic.png", "image/png", &png);

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/files")
                    .header(header::CONTENT_TYPE, "multipart/form-data; boundary=B")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let uploaded: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(uploaded[0]["has_thumbnail"], true);
        let id = uploaded[0]["id"].as_str().unwrap().to_string();

        // Thumbnail is served and is itself a valid PNG.
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/files/{id}/thumb"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let thumb_bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            image::guess_format(&thumb_bytes).unwrap(),
            image::ImageFormat::Png
        );

        let _ = tokio::fs::remove_dir_all(base).await;
    }

    /// Helper: JSON POST/PUT/PATCH request.
    fn json_request(method: &str, uri: String, value: Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&value).unwrap()))
            .unwrap()
    }

    #[tokio::test]
    async fn folder_move_tags_and_bulk_delete_flow() {
        let (state, base) = test_state().await;
        let app = build_router(state);

        // Create a folder.
        let res = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/api/folders".into(),
                serde_json::json!({ "name": "Trips" }),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let folder: Value = serde_json::from_slice(&bytes).unwrap();
        let folder_id = folder["id"].as_str().unwrap().to_string();

        // Upload a file (lands at the root).
        let body = multipart_body("B", "note.txt", "text/plain", b"hi");
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/files")
                    .header(header::CONTENT_TYPE, "multipart/form-data; boundary=B")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let uploaded: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
        let file_id = uploaded[0]["id"].as_str().unwrap().to_string();

        // Move it into the folder.
        let res = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/api/files/move".into(),
                serde_json::json!({ "ids": [file_id], "folder_id": folder_id }),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);

        // It now appears under the folder, and not at the root.
        let in_folder = list_files(&app, &format!("/api/files?folder_id={folder_id}")).await;
        assert_eq!(in_folder.len(), 1);
        assert_eq!(in_folder[0]["folder_id"], folder_id);
        let at_root = list_files(&app, "/api/files?folder_id=root").await;
        assert_eq!(at_root.len(), 0);

        // Set tags, then confirm they come back on the file and in the global list.
        let res = app
            .clone()
            .oneshot(json_request(
                "PUT",
                format!("/api/files/{file_id}/tags"),
                serde_json::json!({ "tags": ["beach", "2026"] }),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/files/{file_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let file: Value = serde_json::from_slice(&bytes).unwrap();
        let mut tags: Vec<String> = file["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_string())
            .collect();
        tags.sort();
        assert_eq!(tags, vec!["2026".to_string(), "beach".to_string()]);

        // Bulk delete.
        let res = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/api/files/delete".into(),
                serde_json::json!({ "ids": [file_id] }),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert_eq!(list_files(&app, "/api/files").await.len(), 0);

        let _ = tokio::fs::remove_dir_all(base).await;
    }

    async fn list_files(app: &Router, uri: &str) -> Vec<Value> {
        let res = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn upload_named(
        app: &Router,
        filename: &str,
        content_type: &str,
        content: &[u8],
    ) -> String {
        let body = multipart_body("B", filename, content_type, content);
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/files")
                    .header(header::CONTENT_TYPE, "multipart/form-data; boundary=B")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let uploaded: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
        uploaded[0]["id"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn search_and_filter_by_name_type_and_tag() {
        let (state, base) = test_state().await;
        let app = build_router(state);

        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::new(4, 4))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        upload_named(&app, "vacation.png", "image/png", &png).await;
        let note_id = upload_named(&app, "notes.txt", "text/plain", b"todo").await;

        // Name search (case-insensitive substring).
        let hits = list_files(&app, "/api/files?q=vacat").await;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["original_name"], "vacation.png");

        // Type filter spans all folders.
        assert_eq!(list_files(&app, "/api/files?type=image").await.len(), 1);
        let text_hits = list_files(&app, "/api/files?type=text").await;
        assert_eq!(text_hits.len(), 1);
        assert_eq!(text_hits[0]["original_name"], "notes.txt");

        // Tag filter.
        let res = app
            .clone()
            .oneshot(json_request(
                "PUT",
                format!("/api/files/{note_id}/tags"),
                serde_json::json!({ "tags": ["work"] }),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let tagged = list_files(&app, "/api/files?tag=work").await;
        assert_eq!(tagged.len(), 1);
        assert_eq!(tagged[0]["id"], note_id);
        assert_eq!(list_files(&app, "/api/files?tag=missing").await.len(), 0);

        let _ = tokio::fs::remove_dir_all(base).await;
    }

    async fn create_folder(app: &Router, name: &str, parent_id: Option<&str>) -> String {
        let payload = match parent_id {
            Some(p) => serde_json::json!({ "name": name, "parent_id": p }),
            None => serde_json::json!({ "name": name }),
        };
        let res = app
            .clone()
            .oneshot(json_request("POST", "/api/folders".into(), payload))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let folder: Value = serde_json::from_slice(&bytes).unwrap();
        folder["id"].as_str().unwrap().to_string()
    }

    async fn status_of(app: &Router, method: &str, uri: &str) -> StatusCode {
        app.clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    /// The schema's two FK actions on folder delete: subfolders cascade away, but
    /// contained files survive and fall back to the root.
    #[tokio::test]
    async fn folder_delete_cascades_subfolders_and_reparents_files_to_root() {
        let (state, base) = test_state().await;
        let app = build_router(state);

        let parent = create_folder(&app, "Parent", None).await;
        let child = create_folder(&app, "Child", Some(&parent)).await;
        let grandchild = create_folder(&app, "Grandchild", Some(&child)).await;

        // A file in the doomed subtree.
        let file_id = upload_named(&app, "keep.txt", "text/plain", b"keep me").await;
        let res = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/api/files/move".into(),
                serde_json::json!({ "ids": [file_id], "folder_id": grandchild }),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);

        // Delete the top of the subtree.
        assert_eq!(
            status_of(&app, "DELETE", &format!("/api/folders/{parent}")).await,
            StatusCode::NO_CONTENT
        );

        // Every descendant folder is gone.
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/folders")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let folders: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(folders.len(), 0, "subfolders should cascade-delete");

        // The file survives, reparented to the root rather than deleted.
        let at_root = list_files(&app, "/api/files?folder_id=root").await;
        assert_eq!(at_root.len(), 1, "contained files must not be deleted");
        assert_eq!(at_root[0]["id"], file_id);
        assert!(at_root[0]["folder_id"].is_null());

        let _ = tokio::fs::remove_dir_all(base).await;
    }

    /// Any of q/type/tag drops the folder predicate, so search spans every folder.
    #[tokio::test]
    async fn search_spans_all_folders_ignoring_folder_scope() {
        let (state, base) = test_state().await;
        let app = build_router(state);

        let folder = create_folder(&app, "Deep", None).await;
        let buried = upload_named(&app, "buried-report.txt", "text/plain", b"x").await;
        let root_file = upload_named(&app, "root-report.txt", "text/plain", b"y").await;
        app.clone()
            .oneshot(json_request(
                "POST",
                "/api/files/move".into(),
                serde_json::json!({ "ids": [buried], "folder_id": folder }),
            ))
            .await
            .unwrap();

        // Unfiltered, the two files are in different scopes.
        assert_eq!(list_files(&app, "/api/files?folder_id=root").await.len(), 1);
        assert_eq!(
            list_files(&app, &format!("/api/files?folder_id={folder}"))
                .await
                .len(),
            1
        );

        // Searching finds both, even with a folder_id pinned to one of them.
        assert_eq!(list_files(&app, "/api/files?q=report").await.len(), 2);
        assert_eq!(
            list_files(&app, "/api/files?q=report&folder_id=root")
                .await
                .len(),
            2,
            "folder_id must be ignored while searching"
        );
        assert_eq!(list_files(&app, "/api/files?type=text").await.len(), 2);

        // Combining all three predicates still builds valid SQL (WHERE/AND joins).
        app.clone()
            .oneshot(json_request(
                "PUT",
                format!("/api/files/{root_file}/tags"),
                serde_json::json!({ "tags": ["urgent"] }),
            ))
            .await
            .unwrap();
        let combined = list_files(&app, "/api/files?q=report&type=text&tag=urgent").await;
        assert_eq!(combined.len(), 1);
        assert_eq!(combined[0]["id"], root_file);

        let _ = tokio::fs::remove_dir_all(base).await;
    }

    /// `%` and `_` in a search term must match literally, not as LIKE wildcards.
    #[tokio::test]
    async fn like_metacharacters_in_search_are_matched_literally() {
        let (state, base) = test_state().await;
        let app = build_router(state);

        upload_named(&app, "notes.txt", "text/plain", b"a").await;
        upload_named(&app, "100%-done.txt", "text/plain", b"b").await;
        upload_named(&app, "a_b.txt", "text/plain", b"c").await;

        // Bare `%` used to match every file in the database.
        let pct = list_files(&app, "/api/files?q=%25").await;
        assert_eq!(pct.len(), 1, "`%` must not act as a wildcard");
        assert_eq!(pct[0]["original_name"], "100%-done.txt");

        // `_` used to match any single character, so `n_tes` matched `notes.txt`.
        assert_eq!(list_files(&app, "/api/files?q=n_tes").await.len(), 0);
        let underscore = list_files(&app, "/api/files?q=a_b").await;
        assert_eq!(underscore.len(), 1);
        assert_eq!(underscore[0]["original_name"], "a_b.txt");

        let _ = tokio::fs::remove_dir_all(base).await;
    }

    /// A file uploaded as text/html must never be served in a way the browser will
    /// execute on this origin.
    #[tokio::test]
    async fn raw_never_serves_client_declared_html_inline() {
        let (state, base) = test_state().await;
        let app = build_router(state);

        let evil = upload_named(
            &app,
            "pwn.html",
            "text/html",
            b"<script>alert(document.domain)</script>",
        )
        .await;
        let png_id = upload_named(&app, "pic.png", "image/png", b"not-really-a-png").await;

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/files/{evil}/raw"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get(header::CONTENT_DISPOSITION).unwrap(),
            "attachment",
            "html must be forced to download"
        );
        assert_eq!(
            res.headers().get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff"
        );

        // Images stay inline so the viewer keeps working, but still get nosniff.
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/files/{png_id}/raw"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(res.headers().get(header::CONTENT_DISPOSITION).is_none());
        assert_eq!(
            res.headers().get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff"
        );

        let _ = tokio::fs::remove_dir_all(base).await;
    }

    /// Client errors must surface as 4xx, not as the blanket 500 from `AppError`.
    #[tokio::test]
    async fn client_errors_return_4xx_not_500() {
        let (state, base) = test_state().await;
        let app = build_router(state);

        // Unknown ids → 404 on every per-file route.
        for uri in [
            "/api/files/nope",
            "/api/files/nope/raw",
            "/api/files/nope/thumb",
        ] {
            assert_eq!(
                status_of(&app, "GET", uri).await,
                StatusCode::NOT_FOUND,
                "{uri}"
            );
        }
        assert_eq!(
            status_of(&app, "DELETE", "/api/files/nope").await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status_of(&app, "DELETE", "/api/folders/nope").await,
            StatusCode::NOT_FOUND
        );

        // A file with no thumbnail 404s rather than erroring.
        let txt = upload_named(&app, "plain.txt", "text/plain", b"no thumb").await;
        assert_eq!(
            status_of(&app, "GET", &format!("/api/files/{txt}/thumb")).await,
            StatusCode::NOT_FOUND
        );

        // Empty folder name, missing parent, and comma-bearing tags are all 400.
        let cases = [
            (
                "POST",
                "/api/folders".to_string(),
                serde_json::json!({ "name": "   " }),
            ),
            (
                "POST",
                "/api/folders".to_string(),
                serde_json::json!({ "name": "x", "parent_id": "ghost" }),
            ),
            (
                "POST",
                "/api/files/move".to_string(),
                serde_json::json!({ "ids": [txt], "folder_id": "ghost" }),
            ),
            (
                "PUT",
                format!("/api/files/{txt}/tags"),
                serde_json::json!({ "tags": ["a,b"] }),
            ),
        ];
        for (method, uri, payload) in cases {
            let res = app
                .clone()
                .oneshot(json_request(method, uri.clone(), payload))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{method} {uri}");
        }

        // Renaming a missing folder is a 404; renaming to blank is a 400.
        let res = app
            .clone()
            .oneshot(json_request(
                "PATCH",
                "/api/folders/ghost".into(),
                serde_json::json!({ "name": "new" }),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        // A multipart body with no file part is a 400, not an empty 200.
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/files")
                    .header(header::CONTENT_TYPE, "multipart/form-data; boundary=B")
                    .body(Body::from("--B--\r\n"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);

        let _ = tokio::fs::remove_dir_all(base).await;
    }
}
