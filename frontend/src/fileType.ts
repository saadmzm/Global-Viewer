import type { FileItem } from './api'

export type ViewerKind = 'image' | 'video' | 'audio' | 'model' | 'pdf' | 'text' | 'other'

/** Decide which viewer to use, based on MIME type with a filename-extension fallback. */
export function viewerKind(file: FileItem): ViewerKind {
  const mime = file.mime_type.toLowerCase()
  const name = file.original_name.toLowerCase()

  // glTF/GLB often arrives as application/octet-stream, so check the extension too.
  if (name.endsWith('.glb') || name.endsWith('.gltf') || mime.includes('gltf')) return 'model'
  if (mime.startsWith('image/')) return 'image'
  if (mime.startsWith('video/')) return 'video'
  if (mime.startsWith('audio/')) return 'audio'
  if (mime === 'application/pdf') return 'pdf'
  if (mime.startsWith('text/') || mime === 'application/json' || mime === 'application/xml') {
    return 'text'
  }
  return 'other'
}

/** Human-readable file size. */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes / 1024
  let i = 0
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024
    i++
  }
  return `${value.toFixed(1)} ${units[i]}`
}
