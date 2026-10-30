import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  createFolder,
  deleteFiles,
  deleteFolder,
  getHealth,
  isFiltering,
  listFiles,
  listFolders,
  listTags,
  moveFiles,
  renameFolder,
  type FileFilters,
  type FileItem,
  type Folder,
} from './api'
import Uploader from './components/Uploader'
import FileGrid from './components/FileGrid'
import Viewer from './components/Viewer'
import Breadcrumb from './components/Breadcrumb'
import FolderList from './components/FolderList'
import SelectionToolbar from './components/SelectionToolbar'
import SearchBar from './components/SearchBar'
import './App.css'

function App() {
  const queryClient = useQueryClient()
  const [folderId, setFolderId] = useState<string | null>(null)
  const [selected, setSelected] = useState<FileItem | null>(null)
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [filters, setFilters] = useState<FileFilters>({})

  const searching = isFiltering(filters)

  const health = useQuery({ queryKey: ['health'], queryFn: getHealth, retry: false })
  const foldersQuery = useQuery({ queryKey: ['folders'], queryFn: listFolders })
  const tagsQuery = useQuery({ queryKey: ['tags'], queryFn: listTags })
  const filesQuery = useQuery({
    queryKey: ['files', folderId, filters],
    queryFn: () => listFiles(folderId, filters),
  })
  const folders = foldersQuery.data ?? []

  function clearSelection() {
    setSelectedIds(new Set())
  }
  function navigate(id: string | null) {
    setFolderId(id)
    setFilters({})
    clearSelection()
  }
  function toggleSelect(id: string) {
    setSelectedIds((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }
  function changeFilters(next: FileFilters) {
    setFilters(next)
    clearSelection()
  }

  const invalidateFiles = () => queryClient.invalidateQueries({ queryKey: ['files'] })

  const delMutation = useMutation({
    mutationFn: (ids: string[]) => deleteFiles(ids),
    onSuccess: () => {
      invalidateFiles()
      clearSelection()
    },
  })
  const moveMutation = useMutation({
    mutationFn: ({ ids, target }: { ids: string[]; target: string | null }) =>
      moveFiles(ids, target),
    onSuccess: () => {
      invalidateFiles()
      clearSelection()
    },
  })
  const createFolderMutation = useMutation({
    mutationFn: (name: string) => createFolder(name, folderId),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['folders'] }),
  })
  const renameFolderMutation = useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) => renameFolder(id, name),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['folders'] }),
  })
  const deleteFolderMutation = useMutation({
    mutationFn: (id: string) => deleteFolder(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['folders'] })
      invalidateFiles()
    },
  })

  function handleNewFolder() {
    const name = prompt('New folder name:')
    if (name && name.trim()) createFolderMutation.mutate(name.trim())
  }
  function handleDeleteFolder(folder: Folder) {
    if (confirm(`Delete folder "${folder.name}"? Files inside move to Home.`)) {
      deleteFolderMutation.mutate(folder.id)
    }
  }
  function handleRenameFolder(folder: Folder) {
    const name = prompt('Rename folder:', folder.name)
    const trimmed = name?.trim()
    if (trimmed && trimmed !== folder.name) {
      renameFolderMutation.mutate({ id: folder.id, name: trimmed })
    }
  }
  function handleBulkDelete() {
    const ids = [...selectedIds]
    if (ids.length === 0) return
    if (!confirm(`Delete ${ids.length} file(s)?`)) return
    if (selected && ids.includes(selected.id)) setSelected(null)
    delMutation.mutate(ids)
  }

  const dbOk = health.data?.status === 'ok' && health.data.db === true

  return (
    <main className="app">
      <header className="app-header">
        <div>
          <h1>Global Viewer</h1>
          <p className="tagline">Upload and view any file — images, video, 3D models, and more.</p>
        </div>
        <span
          className="status-dot"
          title={dbOk ? 'backend connected' : 'backend unreachable'}
          style={{ background: dbOk ? '#30a46c' : '#e5484d' }}
        />
      </header>

      <Uploader currentFolderId={folderId} />

      <SearchBar
        filters={filters}
        tags={tagsQuery.data ?? []}
        onChange={changeFilters}
        onClear={() => changeFilters({})}
      />

      {!searching && (
        <div className="toolbar">
          <Breadcrumb folders={folders} currentId={folderId} onNavigate={navigate} />
          <button className="btn" onClick={handleNewFolder}>
            + New folder
          </button>
        </div>
      )}
      {searching && <p className="search-note">Search results across all folders</p>}

      {selectedIds.size > 0 && (
        <SelectionToolbar
          count={selectedIds.size}
          folders={folders}
          onMove={(target) => moveMutation.mutate({ ids: [...selectedIds], target })}
          onDelete={handleBulkDelete}
          onClear={clearSelection}
        />
      )}

      {!searching && (
        <FolderList
          folders={folders}
          currentId={folderId}
          onOpen={navigate}
          onRename={handleRenameFolder}
          onDelete={handleDeleteFolder}
        />
      )}

      {filesQuery.isLoading && <p>Loading…</p>}
      {filesQuery.isError && (
        <p className="error">Could not load files. Is the backend running?</p>
      )}
      {filesQuery.data && (
        <FileGrid
          files={filesQuery.data}
          selectedIds={selectedIds}
          onOpen={setSelected}
          onToggleSelect={toggleSelect}
        />
      )}

      {selected && <Viewer file={selected} onClose={() => setSelected(null)} />}
    </main>
  )
}

export default App
