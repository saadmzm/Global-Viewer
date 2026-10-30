import type { FileFilters, Tag } from '../api'

const TYPE_OPTIONS: { value: string; label: string }[] = [
  { value: '', label: 'All types' },
  { value: 'image', label: 'Images' },
  { value: 'video', label: 'Videos' },
  { value: 'audio', label: 'Audio' },
  { value: 'model', label: '3D models' },
  { value: 'pdf', label: 'PDFs' },
  { value: 'text', label: 'Text' },
  { value: 'other', label: 'Other' },
]

interface Props {
  filters: FileFilters
  tags: Tag[]
  onChange: (filters: FileFilters) => void
  onClear: () => void
}

export default function SearchBar({ filters, tags, onChange, onClear }: Props) {
  const active = Boolean(filters.q || filters.type || filters.tag)

  return (
    <div className="search-bar">
      <input
        className="search-input"
        type="search"
        placeholder="Search by name…"
        value={filters.q ?? ''}
        onChange={(e) => onChange({ ...filters, q: e.target.value })}
      />
      <select
        className="search-select"
        value={filters.type ?? ''}
        onChange={(e) => onChange({ ...filters, type: e.target.value })}
      >
        {TYPE_OPTIONS.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <select
        className="search-select"
        value={filters.tag ?? ''}
        onChange={(e) => onChange({ ...filters, tag: e.target.value })}
      >
        <option value="">All tags</option>
        {tags.map((t) => (
          <option key={t.id} value={t.name}>
            {t.name}
          </option>
        ))}
      </select>
      {active && (
        <button className="btn" onClick={onClear}>
          Clear
        </button>
      )}
    </div>
  )
}
