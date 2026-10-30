use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{AppState, error::AppError, models::FolderDto};

/// `GET /api/folders` — all folders (flat; the client builds the tree).
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<FolderDto>>, AppError> {
    let folders = sqlx::query_as::<_, FolderDto>(
        "SELECT id, name, parent_id, created_at FROM folders ORDER BY name",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(folders))
}

#[derive(Deserialize)]
pub struct CreateFolder {
    pub name: String,
    pub parent_id: Option<String>,
}

/// `POST /api/folders` — create a folder, optionally nested under a parent.
pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateFolder>,
) -> Result<Json<FolderDto>, AppError> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("folder name must not be empty".into()));
    }

    if let Some(parent_id) = &req.parent_id {
        let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM folders WHERE id = ?")
            .bind(parent_id)
            .fetch_optional(&state.db)
            .await?
            .is_some();
        if !exists {
            return Err(AppError::BadRequest("parent folder does not exist".into()));
        }
    }

    let id = Uuid::new_v4().to_string();
    let created_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default();

    sqlx::query("INSERT INTO folders (id, name, parent_id, created_at) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&name)
        .bind(&req.parent_id)
        .bind(&created_at)
        .execute(&state.db)
        .await?;

    Ok(Json(FolderDto {
        id,
        name,
        parent_id: req.parent_id,
        created_at,
    }))
}

#[derive(Deserialize)]
pub struct RenameFolder {
    pub name: String,
}

/// `PATCH /api/folders/{id}` — rename a folder.
pub async fn rename(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<RenameFolder>,
) -> Result<StatusCode, AppError> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("folder name must not be empty".into()));
    }
    let result = sqlx::query("UPDATE folders SET name = ? WHERE id = ?")
        .bind(name)
        .bind(&id)
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/folders/{id}` — delete a folder. Subfolders cascade-delete; files
/// inside move to the root (folder_id set NULL), per the schema's FK actions.
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM folders WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
