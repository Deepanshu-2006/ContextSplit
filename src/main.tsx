import React, { Component, ReactNode, ErrorInfo } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

class RootErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("ContextSplit Root ErrorBoundary caught:", error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div style={{
          padding: '32px',
          fontFamily: 'system-ui, -apple-system, sans-serif',
          background: '#1c1917',
          color: '#f5f5f4',
          minHeight: '100vh',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          textAlign: 'center'
        }}>
          <div style={{ maxWidth: '600px', width: '100%' }}>
            <h1 style={{ fontSize: '20px', fontWeight: 600, color: '#f87171', marginBottom: '12px' }}>
              Something went wrong loading ContextSplit
            </h1>
            <p style={{ fontSize: '13px', color: '#a8a29e', marginBottom: '20px' }}>
              {this.state.error?.message || 'An unexpected runtime error occurred.'}
            </p>
            <pre style={{
              textAlign: 'left',
              padding: '16px',
              borderRadius: '8px',
              background: '#0c0a09',
              border: '1px solid #292524',
              color: '#d6d3d1',
              fontSize: '11px',
              fontFamily: 'monospace',
              overflow: 'auto',
              maxHeight: '240px',
              marginBottom: '20px'
            }}>
              {this.state.error?.stack}
            </pre>
            <button
              onClick={() => window.location.reload()}
              style={{
                padding: '8px 20px',
                background: '#ea580c',
                color: '#fff',
                border: 'none',
                borderRadius: '6px',
                fontSize: '13px',
                fontWeight: 500,
                cursor: 'pointer'
              }}
            >
              Reload ContextSplit
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <RootErrorBoundary>
      <App />
    </RootErrorBoundary>
  </React.StrictMode>,
);
