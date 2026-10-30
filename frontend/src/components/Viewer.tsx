import { lazy, Suspense, useEffect, useRef } from 'react'
import type { FileItem } from '../api'
import { fileRawUrl } from '../api'
import { viewerKind } from '../fileType'
import TextViewer from './viewers/TextViewer'
import TagEditor from './TagEditor'
import ErrorBoundary from './ErrorBoundary'

// three.js is heavy — only load it when a 3D model is actually opened.
const ModelViewer = lazy(() => import('./viewers/ModelViewer'))

interface Props {
  file: FileItem
  onClose: () => void
}

/** Full-screen modal that dispatches to the right viewer based on file type. */
export default function Viewer({ file, onClose }: Props) {
  const url = fileRawUrl(file.id)
  const kind = viewerKind(file)
  const dialogRef = useRef<HTMLDivElement>(null)

  // Escape to close, and hand focus back to whatever opened the modal so keyboard
  // users aren't dumped at the top of the document on close.
  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null
    dialogRef.current?.focus()

    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', onKeyDown)
    return () => {
      document.removeEventListener('keydown', onKeyDown)
      opener?.focus?.()
    }
  }, [onClose])

  return (
    <div
      className="modal-backdrop"
      // Only close on a click that both started and ended on the backdrop.
      // Without the target check, drag-selecting text in the viewer and
      // releasing past the modal edge closes it (the click event's target is
      // the common ancestor of mousedown and mouseup).
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose()
      }}
    >
      <div
        className="modal"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label={file.original_name}
        tabIndex={-1}
      >
        <header className="modal-header">
          <span className="modal-title" title={file.original_name}>
            {file.original_name}
          </span>
          <a className="modal-download" href={url} download={file.original_name}>
            Download
          </a>
          <button className="modal-close" onClick={onClose} title="Close">
            ✕
          </button>
        </header>

        <div className="modal-body">
          {kind === 'image' && <img className="fit" src={url} alt={file.original_name} />}
          {kind === 'video' && <video className="fit" src={url} controls autoPlay />}
          {kind === 'audio' && <audio src={url} controls />}
          {kind === 'pdf' && <iframe className="fill" src={url} title={file.original_name} />}
          {kind === 'text' && (
            <ErrorBoundary resetKey={url} fallback="Failed to load this text file.">
              <TextViewer url={url} />
            </ErrorBoundary>
          )}
          {kind === 'model' && (
            <div className="canvas-wrap">
              {/* Suspense covers the pending chunk/model; the boundary catches a
                  corrupt glTF or a failed lazy import, which would otherwise
                  unmount the whole app. */}
              <ErrorBoundary
                resetKey={url}
                fallback="This 3D model could not be loaded — it may be corrupt or use an unsupported extension."
              >
                <Suspense fallback={<p>Loading 3D viewer…</p>}>
                  <ModelViewer url={url} />
                </Suspense>
              </ErrorBoundary>
            </div>
          )}
          {kind === 'other' && (
            <div className="fallback">
              <p>No preview for this file type ({file.mime_type}).</p>
              <a href={url} download={file.original_name}>
                Download {file.original_name}
              </a>
            </div>
          )}
        </div>

        <footer className="modal-footer">
          <TagEditor key={file.id} file={file} />
        </footer>
      </div>
    </div>
  )
}
