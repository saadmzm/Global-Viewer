import type { FileItem } from '../api'
import { fileThumbUrl } from '../api'
import { formatSize, viewerKind, type ViewerKind } from '../fileType'

const ICONS: Record<ViewerKind, string> = {
  image: '🖼️',
  video: '🎬',
  audio: '🎵',
  model: '🧊',
  pdf: '📄',
  text: '📝',
  other: '📁',
}

interface Props {
  files: FileItem[]
  selectedIds: Set<string>
  onOpen: (file: FileItem) => void
  onToggleSelect: (id: string) => void
}

export default function FileGrid({ files, selectedIds, onOpen, onToggleSelect }: Props) {
  if (files.length === 0) {
    return <p className="empty">No files here yet. Upload something to get started.</p>
  }

  return (
    <div className="grid">
      {files.map((file) => {
        const kind = viewerKind(file)
        const selected = selectedIds.has(file.id)
        return (
          <div key={file.id} className={`card${selected ? ' selected' : ''}`}>
            {/* Checkbox is a sibling of the open button, not nested inside it:
                interactive elements can't be nested, and this way both are
                natively focusable without any stopPropagation juggling. */}
            <input
              type="checkbox"
              className="select-box"
              aria-label={`Select ${file.original_name}`}
              checked={selected}
              onChange={() => onToggleSelect(file.id)}
            />
            <button type="button" className="card-open" onClick={() => onOpen(file)}>
              <div className="thumb">
                {file.has_thumbnail ? (
                  <img src={fileThumbUrl(file.id)} alt="" loading="lazy" />
                ) : (
                  <span className="icon" aria-hidden="true">
                    {ICONS[kind]}
                  </span>
                )}
              </div>
              <div className="meta">
                <span className="name" title={file.original_name}>
                  {file.original_name}
                </span>
                <span className="sub">{formatSize(file.size_bytes)}</span>
                {file.tags.length > 0 && (
                  <div className="card-tags">
                    {file.tags.map((tag) => (
                      <span key={tag} className="tag-chip small">
                        {tag}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            </button>
          </div>
        )
      })}
    </div>
  )
}
