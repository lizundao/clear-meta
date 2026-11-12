import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

/** 任何渲染异常都降级为可重试的提示页，而不是整窗白屏。 */
export class AppErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("界面渲染异常", error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;

    return (
      <div className="app-error-boundary" role="alert">
        <h2>界面遇到问题</h2>
        <p className="app-error-detail">{error.message}</p>
        <p>你的文件不会被修改，可点击重试；若反复出现请重启应用。</p>
        <button type="button" onClick={() => this.setState({ error: null })}>
          重试
        </button>
      </div>
    );
  }
}
