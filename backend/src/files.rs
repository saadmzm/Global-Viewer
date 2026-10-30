use std::path::Path as FsPath;

use axum::{
    Json,
    extract::{Multipart, Path, Query, Request, State, multipart::Field},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sqlx::{QueryBuilder, Sqlite};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::io::AsyncWriteExt;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    models::{FileDto, FileRow},
    thumbnail,
};

/// Hard cap per uploaded file (2 GiB). Mirrored by the request body limit layer.
pub const MAX_UPLOAD_BYTES: i64 = 2 * 1024 * 1024 * 1024;

// Common SELECT + joins for file listings. Tags are aggregated via group_concat;
// the dynamic WHERE clause is appended with QueryBuilder so filter values bind safely.
const FILE_SELECT: &str = "SELECT f.id, f.original_name, f.mime_type, f.size_bytes, f.folder_id, f.created_at, \
    (f.thumbnail_path IS NOT NULL) AS has_thumbnail, group_concat(t.name) AS tags \
    FROM files f LEFT JOIN file_tags ft ON ft.file_id = f.id LEFT JOIN tags t ON t.id = ft.tag_id ";

const GET_ONE: &str = "SELECT f.id, f.original_name, f.mime_type, f.size_bytes, f.folder_id, f.created_at, \
    (f.thumbnail_path IS NOT NULL) AS has_thumbnail, group_concat(t.name) AS tags \
    FROM files f LEFT JOIN file_tags ft ON ft.file_id = f.id LEFT JOIN tags t ON t.id = ft.tag_id \
    WHERE f.id = ? GROUP BY f.id";

#[derive(Deserialize)]
pub struct ListQuery {
    /// `None`/`"root"`/empty → root level; otherwise a folder id. Ignored while searching.
    pub folder_id: Option<String>,
    /// Case-insensitive substring match on the file name.
    pub q: Option<String>,
    /// MIME category: image | video | audio | model | pdf | text | other.
    #[serde(rename = "type")]
    pub file_type: Option<String>,
    /// Exact tag name.
    pub tag: Option<String>,
}

fn push_separator(qb: &mut QueryBuilder<Sqlite>, has_where: &mut bool) {
    qb.push(if *has_where { " AND " } else { " WHERE " });
    *has_where = true;
}

/// Escape SQL `LIKE` wildcards so user input matches literally. Without this a
/// search for `%` matches every file and `n_tes` matches `notes.txt`. Pair with
/// an `ESCAPE '\'` clause.
fn escape_like(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Append the MIME-category predicate. The patterns are constant literals, so no
/// binding is needed.
fn push_type_predicate(qb: &mut QueryBuilder<Sqlite>, file_type: &str) {
    match file_type {
        "image" => qb.push("f.mime_type LIKE 'image/%'"),
        "video" => qb.push("f.mime_type LIKE 'video/%'"),
        "audio" => qb.push("f.mime_type LIKE 'audio/%'"),
        "pdf" => qb.push("f.mime_type = 'application/pdf'"),
        "model" => qb.push(
            "(lower(f.original_name) LIKE '%.glb' OR lower(f.original_name) LIKE '%.gltf' OR f.mime_type LIKE '%gltf%')",
        ),
        "text" => qb.push(
            "(f.mime_type LIKE 'text/%' OR f.mime_type IN ('application/json', 'application/xml'))",
        ),
        "other" => qb.push(
            "NOT (f.mime_type LIKE 'image/%' OR f.mime_type LIKE 'video/%' OR f.mime_type LIKE 'audio/%' \
             OR f.mime_type = 'application/pdf' OR f.mime_type LIKE 'text/%' \
             OR f.mime_type IN ('application/json', 'application/xml') \
             OR lower(f.original_name) LIKE '%.glb' OR lower(f.original_name) LIKE '%.gltf' OR f.mime_type LIKE '%gltf%')",
        ),
        _ => qb.push("1 = 1"),
    };
}

/// `GET /api/files` — list files with optional filters. When any of `q`/`type`/`tag`
/// is set the search spans all folders; otherwise results are scoped to `folder_id`
/// (`"root"`/absent → top level), newest first.
pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<FileDto>>, AppError> {
    let name = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let file_type = query
        .file_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let tag = query
        .tag
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let searching = name.is_some() || file_type.is_some() || tag.is_some();

    let mut qb = QueryBuilder::<Sqlite>::new(FILE_SELECT);
    let mut has_where = false;

    if let Some(name) = name {
        push_separator(&mut qb, &mut has_where);
        qb.push("f.original_name LIKE ")
            .push_bind(format!("%{}%", escape_like(name)))
            .push(" ESCAPE '\\'");
    }
    if let Some(tag) = tag {
        // Filter via a subquery so group_concat still returns the file's full tag set.
        push_separator(&mut qb, &mut has_where);
        qb.push("f.id IN (SELECT ft2.file_id FROM file_tags ft2 JOIN tags t2 ON t2.id = ft2.tag_id WHERE t2.name = ")
            .push_bind(tag.to_string())
            .push(")");
    }
    if let Some(file_type) = file_type {
        push_separator(&mut qb, &mut has_where);
        push_type_predicate(&mut qb, file_type);
    }
    if !searching {
        push_separator(&mut qb, &mut has_where);
        match query.folder_id.as_deref() {
            Some(id) if id != "root" && !id.is_empty() => {
                qb.push("f.folder_id = ").push_bind(id.to_string());
            }
            _ => {
                qb.push("f.folder_id IS NULL");
            }
        }
    }

    qb.push(" GROUP BY f.id ORDER BY f.created_at DESC");

    let rows = qb.build_query_as::<FileRow>().fetch_all(&state.db).await?;
    Ok(Json(rows.into_iter().map(FileDto::from).collect()))
}

/// `GET /api/files/{id}` — metadata for one file.
pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<FileDto>, AppError> {
    let row = sqlx::query_as::<_, FileRow>(GET_ONE)
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(row.into()))
}

/// Stream one multipart field to `path`, returning the bytes written.
///
/// Enforces the per-file cap. In practice the `DefaultBodyLimit` layer on the
/// upload route rejects an oversized *request body* before this check can fire —
/// this guards the per-file case and keeps the limit honest if the layer changes.
async fn stream_to_disk(field: &mut Field<'_>, path: &FsPath) -> Result<i64, AppError> {
    let mut file = tokio::fs::File::create(path).await?;
    let mut size: i64 = 0;

    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|e| AppError::BadRequest(format!("upload stream error: {e}")))?
    {
        size += chunk.len() as i64;
        if size > MAX_UPLOAD_BYTES {
            return Err(AppError::PayloadTooLarge);
        }
        file.write_all(&chunk).await?;
    }
    file.flush().await?;

    Ok(size)
}

/// `POST /api/files` — multipart upload. Each file part is streamed to disk
/// (never fully buffered) and recorded in the database.
pub async fn upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Vec<FileDto>>, AppError> {
    let mut uploaded = Vec::new();

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("invalid multipart body: {e}")))?
    {
        // Only handle parts that carry a filename (i.e. actual file uploads).
        let Some(original_name) = field.file_name().map(str::to_owned) else {
            continue;
        };
        let declared_mime = field.content_type().map(str::to_owned);

        // Store under a UUID (keeping the extension so MIME detection still works
        // and the path can never escape the uploads dir).
        let id = Uuid::new_v4().to_string();
        let stored_name = match FsPath::new(&original_name)
            .extension()
            .and_then(|e| e.to_str())
        {
            Some(ext) => format!("{id}.{ext}"),
            None => id.clone(),
        };
        let path = state.uploads_dir.join(&stored_name);

        // On any streaming failure, delete the partial file rather than leaving
        // orphaned bytes on disk with no database row pointing at them.
        let size = match stream_to_disk(&mut field, &path).await {
            Ok(size) => size,
            Err(e) => {
                let _ = tokio::fs::remove_file(&path).await;
                return Err(e);
            }
        };

        // Prefer the browser-declared type; fall back to guessing from extension.
        let mime_type = declared_mime.unwrap_or_else(|| {
            mime_guess::from_path(&original_name)
                .first_or_octet_stream()
                .to_string()
        });
        let created_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_default();
        let storage_path = path.to_string_lossy().to_string();

        sqlx::query(
            "INSERT INTO files (id, original_name, mime_type, size_bytes, storage_path, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&original_name)
        .bind(&mime_type)
        .bind(size)
        .bind(&storage_path)
        .bind(&created_at)
        .execute(&state.db)
        .await?;

        // Best-effort thumbnail for images. Failure (unsupported/corrupt image)
        // just leaves thumbnail_path NULL — it never fails the upload.
        let mut has_thumbnail = false;
        if mime_type.starts_with("image/") {
            let thumb_path = state.thumbnails_dir.join(format!("{id}.png"));
            let src = path.clone();
            let dest = thumb_path.clone();
            match tokio::task::spawn_blocking(move || thumbnail::generate(&src, &dest)).await {
                Ok(Ok(())) => {
                    let thumb_str = thumb_path.to_string_lossy().to_string();
                    sqlx::query("UPDATE files SET thumbnail_path = ? WHERE id = ?")
                        .bind(&thumb_str)
                        .bind(&id)
                        .execute(&state.db)
                        .await?;
                    has_thumbnail = true;
                }
                Ok(Err(e)) => tracing::warn!("thumbnail generation failed for {id}: {e}"),
                Err(e) => tracing::warn!("thumbnail task panicked for {id}: {e}"),
            }
        }

        uploaded.push(FileDto {
            id,
            original_name,
            mime_type,
            size_bytes: size,
            folder_id: None,
            created_at,
            has_thumbnail,
            tags: Vec::new(),
        });
    }

    if uploaded.is_empty() {
        return Err(AppError::BadRequest("no file parts in request".into()));
    }

    Ok(Json(uploaded))
}

/// Whether we're willing to let the browser render this type inline.
///
/// `mime_type` comes from the *client-declared* multipart content type, so an
/// attacker fully controls it. Anything not on this list is forced to download,
/// which stops `text/html` (or a script-carrying `image/svg+xml`) uploaded to
/// this origin from executing against the unauthenticated API.
fn is_inline_safe(mime_type: &str) -> bool {
    let essence = mime_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    // SVG is deliberately carved out of image/* — it can carry <script>.
    if essence == "image/svg+xml" || essence == "image/svg" {
        return false;
    }
    essence.starts_with("image/")
        || essence.starts_with("video/")
        || essence.starts_with("audio/")
        || essence == "application/pdf"
}

/// `GET /api/files/{id}/raw` — stream the file bytes. Delegates to tower-http's
/// `ServeFile`, which handles HTTP Range requests (needed for video seeking and
/// large 3D models) and sets the content type we recorded at upload time.
///
/// Text and model files are fetched by the frontend via `fetch()`/three.js rather
/// than rendered by the browser, so forcing them to `attachment` costs nothing.
pub async fn serve_raw(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: Request,
) -> Result<Response, AppError> {
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT storage_path, mime_type FROM files WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let (storage_path, mime_type) = row;
    let mime = mime_type
        .parse::<mime::Mime>()
        .unwrap_or(mime::APPLICATION_OCTET_STREAM);

    let response = ServeFile::new_with_mime(&storage_path, &mime)
        .oneshot(request)
        .await // ServeFile's error type is Infallible
        .into_response();

    let mut response = response;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if !is_inline_safe(&mime_type) {
        headers.insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment"),
        );
    }

    Ok(response)
}

/// `GET /api/files/{id}/thumb` — serve the generated thumbnail (PNG), or 404.
pub async fn serve_thumb(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: Request,
) -> Result<Response, AppError> {
    let thumbnail_path =
        sqlx::query_as::<_, (Option<String>,)>("SELECT thumbnail_path FROM files WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound)? // no such file
            .0
            .ok_or(AppError::NotFound)?; // file exists but has no thumbnail

    let response = ServeFile::new(&thumbnail_path)
        .oneshot(request)
        .await
        .into_response();

    Ok(response)
}

#[derive(Deserialize)]
pub struct RenameRequest {
    pub original_name: String,
}

/// `PATCH /api/files/{id}` — rename a file (display name only).
pub async fn rename(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<RenameRequest>,
) -> Result<StatusCode, AppError> {
    let name = req.original_name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name must not be empty".into()));
    }
    let result = sqlx::query("UPDATE files SET original_name = ? WHERE id = ?")
        .bind(name)
        .bind(&id)
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct MoveRequest {
    pub ids: Vec<String>,
    /// Target folder, or `null` to move to the root.
    pub folder_id: Option<String>,
}

/// `POST /api/files/move` — move one or more files into a folder (or the root).
pub async fn move_files(
    State(state): State<AppState>,
    Json(req): Json<MoveRequest>,
) -> Result<StatusCode, AppError> {
    if let Some(folder_id) = &req.folder_id {
        let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM folders WHERE id = ?")
            .bind(folder_id)
            .fetch_optional(&state.db)
            .await?
            .is_some();
        if !exists {
            return Err(AppError::BadRequest("target folder does not exist".into()));
        }
    }

    let mut tx = state.db.begin().await?;
    for id in &req.ids {
        sqlx::query("UPDATE files SET folder_id = ? WHERE id = ?")
            .bind(&req.folder_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct DeleteManyRequest {
    pub ids: Vec<String>,
}

/// `POST /api/files/delete` — delete several files at once.
pub async fn delete_many(
    State(state): State<AppState>,
    Json(req): Json<DeleteManyRequest>,
) -> Result<StatusCode, AppError> {
    for id in &req.ids {
        remove_file_record(&state, id).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/files/{id}` — remove a single file.
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    if remove_file_record(&state, &id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

/// Delete a file's DB row and its bytes/thumbnail on disk. Returns whether a row
/// existed. Disk cleanup is best-effort.
async fn remove_file_record(state: &AppState, id: &str) -> Result<bool, AppError> {
    let row = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT storage_path, thumbnail_path FROM files WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;

    let Some((storage_path, thumbnail_path)) = row else {
        return Ok(false);
    };

    sqlx::query("DELETE FROM files WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;

    let _ = tokio::fs::remove_file(&storage_path).await;
    if let Some(thumb) = thumbnail_path {
        let _ = tokio::fs::remove_file(&thumb).await;
    }
    Ok(true)
}
