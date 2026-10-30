import type { Folder } from '../api'

interface Props {
  count: number
  folders: Folder[]
  onMove: (folderId: string | null) => void
  onDelete: () => void
  onClear: () => void
}

/** Action bar shown when one or more files are selected. */
export default function SelectionToolbar({ count, folders, onMove, onDelete, onClear }: Props) {
  return (
    <div className="selection-bar">
      <span className="sel-count">{count} selected</span>
      <select
        className="move-select"
        defaultValue=""
        onChange={(e) => {
          const value = e.target.value
          if (value === '') return
          onMove(value === 'root' ? null : value)
          e.target.value = ''
        }}
      >
        <option value="">Move to…</option>
        <option value="root">Home</option>
        {folders.map((f) => (
          <option key={f.id} value={f.id}>
            {f.name}
          </option>
        ))}
      </select>
      <button className="btn danger" onClick={onDelete}>
        Delete
      </button>
      <button className="btn" onClick={onClear}>
        Clear
      </button>
    </div>
  )
}
