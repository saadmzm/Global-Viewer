use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{AppState, error::AppError, models::TagDto};

/// `GET /api/tags` — all tags, alphabetical.
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<TagDto>>, AppError> {
    let tags = sqlx::query_as::<_, TagDto>("SELECT id, name FROM tags ORDER BY name")
        .fetch_all(&state.db)
        .await?;
    Ok(Json(tags))
}

#[derive(Deserialize)]
pub struct SetTagsRequest {
    pub tags: Vec<String>,
}

/// `PUT /api/files/{id}/tags` — replace a file's tags with the given set (by name).
/// Unknown tag names are created; the global tag list is the union of all used names.
pub async fn set_for_file(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
    Json(req): Json<SetTagsRequest>,
) -> Result<StatusCode, AppError> {
    // Normalize: trim, drop empties, dedupe (case-sensitive). Commas are reserved
    // because the listing query joins tag names with `group_concat`.
    let mut names: Vec<String> = Vec::new();
    for raw in req.tags {
        let name = raw.trim().to_string();
        if name.is_empty() {
            continue;
        }
        if name.contains(',') {
            return Err(AppError::BadRequest(
                "tag names may not contain commas".into(),
            ));
        }
        if !names.contains(&name) {
            names.push(name);
        }
    }

    let file_exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM files WHERE id = ?")
        .bind(&file_id)
        .fetch_optional(&state.db)
        .await?
        .is_some();
    if !file_exists {
        return Err(AppError::NotFound);
    }

    let mut tx = state.db.begin().await?;

    // Replace the file's existing tag links.
    sqlx::query("DELETE FROM file_tags WHERE file_id = ?")
        .bind(&file_id)
        .execute(&mut *tx)
        .await?;

    for name in &names {
        // Create the tag if new; either way fetch its id.
        let new_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT OR IGNORE INTO tags (id, name) VALUES (?, ?)")
            .bind(&new_id)
            .bind(name)
            .execute(&mut *tx)
            .await?;
        let tag_id = sqlx::query_scalar::<_, String>("SELECT id FROM tags WHERE name = ?")
            .bind(name)
            .fetch_one(&mut *tx)
            .await?;

        sqlx::query("INSERT INTO file_tags (file_id, tag_id) VALUES (?, ?)")
            .bind(&file_id)
            .bind(&tag_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
