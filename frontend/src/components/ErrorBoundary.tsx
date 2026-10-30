import { Component, type ErrorInfo, type ReactNode } from 'react'

interface Props {
  /** Remounts the boundary (clearing the error) whenever this changes. */
  resetKey?: string
  fallback?: string
  children: ReactNode
}

/**
 * Catches render-time errors from its subtree.
 *
 * `<Suspense>` only handles pending promises — a rejected one (a corrupt glTF, a
 * failed lazy chunk fetch) propagates to the root and unmounts the whole app,
 * blanking the page until reload. Every viewer that loads remote bytes needs one
 * of these above it.
 */
export default class ErrorBoundary extends Component<Props, { error: Error | null }> {
  state = { error: null as Error | null }

  static getDerivedStateFromError(error: Error) {
    return { error }
  }

  componentDidUpdate(prev: Props) {
    if (prev.resetKey !== this.props.resetKey && this.state.error) {
      this.setState({ error: null })
    }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('Viewer failed to render:', error, info.componentStack)
  }

  render() {
    if (this.state.error) {
      return (
        <div className="fallback">
          <p className="error">{this.props.fallback ?? 'Something went wrong.'}</p>
          <p className="error-detail">{this.state.error.message}</p>
        </div>
      )
    }
    return this.props.children
  }
}
