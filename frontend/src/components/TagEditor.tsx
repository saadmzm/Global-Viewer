import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { setFileTags, type FileItem } from '../api'

/** Edit a file's tags: chips with remove buttons + an input to add new ones. */
export default function TagEditor({ file }: { file: FileItem }) {
  const queryClient = useQueryClient()
  const [input, setInput] = useState('')
  const [error, setError] = useState<string | null>(null)

  // The tag list the user sees is whatever was last successfully written, falling
  // back to the server's copy. Keeping a *mirror* in state was the bug: `add()`
  // and a chip's × both read it, and it only updated in onSuccess, so two
  // overlapping edits each computed their payload from the same stale array.
  const [saved, setSaved] = useState<string[] | null>(null)
  const tags = saved ?? file.tags

  const mutation = useMutation({
    mutationFn: (next: string[]) => setFileTags(file.id, next),
    onSuccess: (_data, next) => {
      setSaved(next)
      setError(null)
      queryClient.invalidateQueries({ queryKey: ['files'] })
      queryClient.invalidateQueries({ queryKey: ['tags'] })
    },
    onError: () => setError('Could not save tags.'),
  })

  function commit(next: string[]) {
    // One write at a time. Without this, blurring the input to click a chip's ×
    // fires both handlers against the same `tags`, and the two PUTs race — the
    // loser's change is silently discarded.
    if (mutation.isPending) return
    mutation.mutate(next)
  }

  function add() {
    const tag = input.trim()
    if (!tag) return
    if (tag.includes(',')) {
      setError('Tag names may not contain commas.')
      return
    }
    if (tags.includes(tag)) {
      setError(`"${tag}" is already on this file.`)
      return
    }
    setInput('')
    setError(null)
    commit([...tags, tag])
  }

  return (
    <div className="tag-editor">
      <span className="tag-label">Tags:</span>
      {tags.map((tag) => (
        <span key={tag} className="tag-chip">
          {tag}
          <button
            aria-label={`Remove tag ${tag}`}
            disabled={mutation.isPending}
            onClick={() => commit(tags.filter((t) => t !== tag))}
          >
            ×
          </button>
        </span>
      ))}
      <input
        className="tag-input"
        value={input}
        placeholder="add tag…"
        aria-label="Add a tag"
        onChange={(e) => setInput(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') add()
        }}
        onBlur={add}
      />
      {error && <span className="error tag-error">{error}</span>}
    </div>
  )
}
