import { Component, StrictMode, type ErrorInfo, type ReactNode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { ThemeProvider } from './theme/ThemeProvider.tsx'
import { initializeTheme } from './theme/themeState.ts'

initializeTheme()
document.documentElement.dataset.terraFaction = 'rhine-lab'
document.documentElement.dataset.terraDepth = 'moderate'
document.documentElement.dataset.terraTemplate = 'dashboard'

interface StartupErrorBoundaryState {
  errorMessage: string | null
}

class StartupErrorBoundary extends Component<
  { children: ReactNode },
  StartupErrorBoundaryState
> {
  state: StartupErrorBoundaryState = { errorMessage: null }

  static getDerivedStateFromError(error: unknown): StartupErrorBoundaryState {
    return {
      errorMessage:
        error instanceof Error ? error.message : '桌面界面初始化时发生未知错误。',
    }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('[app-startup] render_failed', {
      message: error.message,
      componentStack: info.componentStack,
    })
  }

  render() {
    if (this.state.errorMessage) {
      return (
        <main className="startup-error" role="alert">
          <section className="startup-error__card">
            <h1>OmniNova 启动失败</h1>
            <p>桌面界面未能正常初始化，请重试。若问题持续，请查看启动日志。</p>
            <code>{this.state.errorMessage}</code>
            <button type="button" onClick={() => window.location.reload()}>
              重新加载
            </button>
          </section>
        </main>
      )
    }

    return this.props.children
  }
}

const rootElement = document.getElementById('root')

if (!rootElement) {
  console.error('[app-startup] root_element_missing')
  document.body.innerHTML =
    '<main class="startup-error" role="alert"><section class="startup-error__card"><h1>OmniNova 启动失败</h1><p>页面容器缺失，请重新启动应用。</p></section></main>'
} else {
  createRoot(rootElement).render(
    <StrictMode>
      <StartupErrorBoundary>
        <ThemeProvider>
          <App />
        </ThemeProvider>
      </StartupErrorBoundary>
    </StrictMode>,
  )
}
