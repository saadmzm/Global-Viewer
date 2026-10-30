import type { Folder } from '../api'

interface Props {
  folders: Folder[]
  currentId: string | null
  onOpen: (id: string) => void
  onRename: (folder: Folder) => void
  onDelete: (folder: Folder) => void
}

/** Subfolders of the current folder, shown as a row of cards above the files. */
export default function FolderList({ folders, currentId, onOpen, onRename, onDelete }: Props) {
  const children = folders.filter((f) => f.parent_id === currentId)
  if (children.length === 0) return null

  return (
    <div className="folder-row">
      {children.map((folder) => (
        <div key={folder.id} className="folder-card">
          {/* Opening is its own button so the rename/delete buttons aren't nested
              inside a clickable parent — that was invalid, and it meant a keyboard
              user could reach delete but had no way to open the folder. */}
          <button type="button" className="folder-open" onClick={() => onOpen(folder.id)}>
            <span className="folder-icon" aria-hidden="true">
              📁
            </span>
            <span className="folder-name" title={folder.name}>
              {folder.name}
            </span>
          </button>
          <button
            className="folder-action"
            title="Rename folder"
            aria-label={`Rename folder ${folder.name}`}
            onClick={() => onRename(folder)}
          >
            ✎
          </button>
          <button
            className="folder-action"
            title="Delete folder"
            aria-label={`Delete folder ${folder.name}`}
            onClick={() => onDelete(folder)}
          >
            🗑
          </button>
        </div>
      ))}
    </div>
  )
}
