import axios from 'axios'

/**
 * Shared axios instance. Base URL is just "/api" because Vite proxies it to
 * the Rust backend in dev (see vite.config.ts), and in production the frontend
 * is expected to be served behind the same origin as the API.
 */
export const api = axios.create({
  baseURL: '/api',
})

export interface HealthResponse {
  status: string
  db: boolean
}

export async function getHealth(): Promise<HealthResponse> {
  const { data } = await api.get<HealthResponse>('/health')
  return data
}

export interface FileItem {
  id: string
  original_name: string
  mime_type: string
  size_bytes: number
  folder_id: string | null
  created_at: string
  has_thumbnail: boolean
  tags: string[]
}

export interface FileFilters {
  q?: string
  type?: string
  tag?: string
}

export function isFiltering(filters: FileFilters): boolean {
  return Boolean(filters.q || filters.type || filters.tag)
}

export async function listFiles(
  folderId: string | null,
  filters: FileFilters = {},
): Promise<FileItem[]> {
  const params: Record<string, string> = {}
  if (isFiltering(filters)) {
    // Filtering searches across all folders.
    if (filters.q) params.q = filters.q
    if (filters.type) params.type = filters.type
    if (filters.tag) params.tag = filters.tag
  } else {
    params.folder_id = folderId === null ? 'root' : folderId
  }
  const { data } = await api.get<FileItem[]>('/files', { params })
  return data
}

export async function uploadFiles(
  files: File[],
  onProgress?: (percent: number) => void,
): Promise<FileItem[]> {
  const form = new FormData()
  for (const file of files) form.append('file', file)

  const { data } = await api.post<FileItem[]>('/files', form, {
    onUploadProgress: (e) => {
      if (onProgress && e.total) onProgress(Math.round((e.loaded / e.total) * 100))
    },
  })
  return data
}

export async function deleteFiles(ids: string[]): Promise<void> {
  await api.post('/files/delete', { ids })
}

export async function moveFiles(ids: string[], folderId: string | null): Promise<void> {
  await api.post('/files/move', { ids, folder_id: folderId })
}

export async function renameFile(id: string, originalName: string): Promise<void> {
  await api.patch(`/files/${id}`, { original_name: originalName })
}

export async function setFileTags(id: string, tags: string[]): Promise<void> {
  await api.put(`/files/${id}/tags`, { tags })
}

// --- Folders ---

export interface Folder {
  id: string
  name: string
  parent_id: string | null
  created_at: string
}

export async function listFolders(): Promise<Folder[]> {
  const { data } = await api.get<Folder[]>('/folders')
  return data
}

export async function createFolder(name: string, parentId: string | null): Promise<Folder> {
  const { data } = await api.post<Folder>('/folders', { name, parent_id: parentId })
  return data
}

export async function renameFolder(id: string, name: string): Promise<void> {
  await api.patch(`/folders/${id}`, { name })
}

export async function deleteFolder(id: string): Promise<void> {
  await api.delete(`/folders/${id}`)
}

// --- Tags ---

export interface Tag {
  id: string
  name: string
}

export async function listTags(): Promise<Tag[]> {
  const { data } = await api.get<Tag[]>('/tags')
  return data
}

/** Direct URL to the raw bytes — use in <img>/<video>/src attributes, not via axios. */
export function fileRawUrl(id: string): string {
  return `/api/files/${id}/raw`
}

/** Direct URL to the generated thumbnail (only valid when has_thumbnail is true). */
export function fileThumbUrl(id: string): string {
  return `/api/files/${id}/thumb`
}
