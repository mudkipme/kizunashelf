import { Component, type ErrorInfo, type ReactNode } from "react";

import { Button } from "@/components/ui/button";

type ErrorBoundaryProps = {
  children: ReactNode;
  /** When this value changes, the boundary resets (e.g. on route change). */
  resetKey?: unknown;
};

type ErrorBoundaryState = {
  error: Error | null;
  resetKey: unknown;
};

/**
 * Catches render-time throws — including a Zod validation error from API
 * contract drift — so a single failure degrades to a recoverable message
 * instead of a blank white screen. Resets automatically when `resetKey` changes
 * (the caller passes the route path) so navigating away recovers.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { error: null, resetKey: props.resetKey };
  }

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    return { error };
  }

  static getDerivedStateFromProps(
    props: ErrorBoundaryProps,
    state: ErrorBoundaryState,
  ): Partial<ErrorBoundaryState> | null {
    // Route changed (resetKey advanced): clear any caught error and track it.
    if (props.resetKey !== state.resetKey) {
      return { error: null, resetKey: props.resetKey };
    }
    return null;
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Surface the detail to the console for debugging; the UI stays friendly.
    console.error("Unhandled UI error:", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <main className="flex h-dvh flex-col items-center justify-center gap-4 bg-background p-8 text-center">
        <div className="max-w-md">
          <h1 className="text-base font-semibold">Something went wrong</h1>
          <p className="mt-2 text-sm text-muted-foreground">
            The page hit an unexpected error. You can try again, or reload the app.
          </p>
          <p className="mt-2 break-words text-xs text-muted-foreground/80">{this.state.error.message}</p>
        </div>
        <div className="flex gap-2">
          <Button type="button" variant="outline" onClick={() => this.setState({ error: null })}>
            Try again
          </Button>
          <Button type="button" onClick={() => window.location.reload()}>
            Reload
          </Button>
        </div>
      </main>
    );
  }
}
