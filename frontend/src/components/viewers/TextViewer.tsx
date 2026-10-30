import { useQuery } from '@tanstack/react-query'

/**
 * Only ever pull down this much of a text file. The upload cap is 2 GiB, so
 * without a Range request opening a large log would buffer the whole thing into
 * memory and hang the tab.
 */
const MAX_BYTES = 256 * 1024

/** Fetches and displays text content (capped to keep huge files responsive). */
export default function TextViewer({ url }: { url: string }) {
  const { data, isPending, isError } = useQuery({
    // Keyed by url, so a response can never be written into another file's slot —
    // this replaces the hand-rolled `active` flag against stale responses.
    queryKey: ['text', url],
    queryFn: async ({ signal }) => {
      const res = await fetch(url, {
        signal,
        headers: { Range: `bytes=0-${MAX_BYTES - 1}` },
      })
      // 206 = server honoured the range, 200 = it ignored it (file may be larger).
      if (!res.ok) throw new Error(`Request failed with ${res.status}`)
      const text = await res.text()
      return { text, truncated: res.status === 206 && text.length >= MAX_BYTES / 2 }
    },
    staleTime: Infinity,
    retry: false,
  })

  if (isError) return <p className="error">Failed to load text.</p>
  if (isPending) return <p>Loading…</p>

  return (
    <>
      <pre className="text-viewer">{data.text}</pre>
      {data.truncated && <p className="text-truncated">Preview truncated — download to see the full file.</p>}
    </>
  )
}
