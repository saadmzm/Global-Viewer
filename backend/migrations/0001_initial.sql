-- Global Viewer initial schema
-- Single-user for now; owner_id columns are nullable and ready for auth later.

CREATE TABLE folders (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    parent_id  TEXT REFERENCES folders (id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    owner_id   TEXT
);

CREATE TABLE files (
    id             TEXT PRIMARY KEY,           -- UUID; also the on-disk filename
    original_name  TEXT NOT NULL,              -- name shown to the user
    mime_type      TEXT NOT NULL,
    size_bytes     INTEGER NOT NULL,
    storage_path   TEXT NOT NULL,              -- absolute/relative path to bytes on disk
    thumbnail_path TEXT,                        -- NULL when no thumbnail generated
    folder_id      TEXT REFERENCES folders (id) ON DELETE SET NULL,
    created_at     TEXT NOT NULL,
    owner_id       TEXT
);

CREATE TABLE tags (
    id   TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE file_tags (
    file_id TEXT NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    tag_id  TEXT NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
    PRIMARY KEY (file_id, tag_id)
);

CREATE INDEX idx_files_name      ON files (original_name);
CREATE INDEX idx_files_mime      ON files (mime_type);
CREATE INDEX idx_files_folder    ON files (folder_id);
CREATE INDEX idx_files_created   ON files (created_at);
CREATE INDEX idx_folders_parent  ON folders (parent_id);
