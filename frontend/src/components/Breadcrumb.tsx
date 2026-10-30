import type { Folder } from '../api'

interface Props {
  folders: Folder[]
  currentId: string | null
  onNavigate: (id: string | null) => void
}

/** Path from Home to the current folder, built by walking parent links. */
export default function Breadcrumb({ folders, currentId, onNavigate }: Props) {
  const byId = new Map(folders.map((f) => [f.id, f]))
  const path: Folder[] = []
  let cursor = currentId
  while (cursor) {
    const folder = byId.get(cursor)
    if (!folder) break
    path.unshift(folder)
    cursor = folder.parent_id
  }

  return (
    <nav className="breadcrumb">
      <button className="crumb" onClick={() => onNavigate(null)}>
        Home
      </button>
      {path.map((folder) => (
        <span key={folder.id}>
          <span className="sep">/</span>
          <button className="crumb" onClick={() => onNavigate(folder.id)}>
            {folder.name}
          </button>
        </span>
      ))}
    </nav>
  )
}
