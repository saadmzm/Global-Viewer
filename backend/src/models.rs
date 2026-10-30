use serde::Serialize;

/// File metadata returned to the frontend. Deliberately omits internal fields
/// like `storage_path` and `owner_id`.
#[derive(Debug, Serialize)]
pub struct FileDto {
    pub id: String,
    pub original_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub folder_id: Option<String>,
    pub created_at: String,
    pub has_thumbnail: bool,
    pub tags: Vec<String>,
}

/// Raw row shape from the listing query. `tags` is a comma-joined string from
/// SQLite's `group_concat`, which we split into a `Vec` for the DTO.
#[derive(sqlx::FromRow)]
pub struct FileRow {
    pub id: String,
    pub original_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub folder_id: Option<String>,
    pub created_at: String,
    pub has_thumbnail: bool,
    pub tags: Option<String>,
}

impl From<FileRow> for FileDto {
    fn from(row: FileRow) -> Self {
        let tags = row
            .tags
            .map(|s| {
                s.split(',')
                    .filter(|t| !t.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        FileDto {
            id: row.id,
            original_name: row.original_name,
            mime_type: row.mime_type,
            size_bytes: row.size_bytes,
            folder_id: row.folder_id,
            created_at: row.created_at,
            has_thumbnail: row.has_thumbnail,
            tags,
        }
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FolderDto {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TagDto {
    pub id: String,
    pub name: String,
}
