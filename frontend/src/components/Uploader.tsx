import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { moveFiles, uploadFiles } from '../api'

/** Drag-and-drop / click uploader. Uploads land in the current folder. */
export default function Uploader({ currentFolderId }: { currentFolderId: string | null }) {
  const queryClient = useQueryClient()
  const [dragOver, setDragOver] = useState(false)
  const [progress, setProgress] = useState<number | null>(null)
  const [strandedAtRoot, setStrandedAtRoot] = useState(false)

  const mutation = useMutation({
    mutationFn: async (files: File[]) => {
      setStrandedAtRoot(false)
      const uploaded = await uploadFiles(files, setProgress)
      // Uploads always land at the root; move them into the open folder. The
      // upload itself has already committed at this point, so a failed move must
      // not be reported as a failed upload — the files exist, just at the root.
      if (currentFolderId) {
        try {
          await moveFiles(
            uploaded.map((f) => f.id),
            currentFolderId,
          )
        } catch {
          setStrandedAtRoot(true)
        }
      }
      return uploaded
    },
    // Invalidate on settle, not just success: if the move failed the files are
    // still on the server, and without this they stay invisible until reload.
    onSettled: () => {
      setProgress(null)
      queryClient.invalidateQueries({ queryKey: ['files'] })
    },
  })

  function handleFiles(list: FileList | null) {
    if (!list || list.length === 0) return
    mutation.mutate(Array.from(list))
  }

  return (
    <div
      className={`uploader${dragOver ? ' drag-over' : ''}`}
      onDragOver={(e) => {
        e.preventDefault()
        setDragOver(true)
      }}
      onDragLeave={() => setDragOver(false)}
      onDrop={(e) => {
        e.preventDefault()
        setDragOver(false)
        handleFiles(e.dataTransfer.files)
      }}
    >
      {/* A <label> wrapping the input keeps the whole zone clickable while leaving
          the input in the tab order. The input must not be `hidden`/display:none
          or it stops being focusable, which leaves no keyboard path to upload. */}
      <label className="uploader-label">
        <input
          className="visually-hidden"
          type="file"
          multiple
          onChange={(e) => {
            handleFiles(e.target.files)
            e.target.value = '' // allow re-uploading the same file
          }}
        />
        <span>
          {mutation.isPending ? `Uploading… ${progress ?? 0}%` : 'Drag files here, or click to upload'}
        </span>
      </label>
      {mutation.isError && <span className="error"> — upload failed</span>}
      {strandedAtRoot && (
        <span className="error"> — uploaded, but couldn't move here; check Home</span>
      )}
    </div>
  )
}
