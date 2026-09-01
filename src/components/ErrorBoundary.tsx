import { Component, type ReactNode } from "react";
import { AlertTriangle } from "lucide-react";

interface State {
  error: Error | null;
}

/// A render error used to unmount the whole tree, leaving the window a solid
/// dark rectangle with no way back except quitting the application. Whatever
/// else goes wrong, the window stays usable and says what happened.
export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: { componentStack?: string | null }) {
    // The webview console is where a report can be copied from.
    console.error("Interface error", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return <div className="crash-screen" role="alert">
      <AlertTriangle aria-hidden size={40} />
      <h1>The interface stopped responding</h1>
      <p>Nothing installed was changed by this. Reloading rebuilds the window from what is on disk.</p>
      <pre>{this.state.error.message}</pre>
      <div>
        <button className="primary" onClick={() => window.location.reload()}>Reload the interface</button>
        <button onClick={() => this.setState({ error: null })}>Try to continue</button>
      </div>
    </div>;
  }
}
