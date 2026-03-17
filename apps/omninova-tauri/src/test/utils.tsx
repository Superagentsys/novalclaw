import type { ReactElement } from 'react'
import { render } from '@testing-library/react'
import type { RenderOptions } from '@testing-library/react'

/**
 * 自定义 render 函数
 *
 * 可以在这里添加 providers（如 ThemeProvider, Router 等）
 * 以便在测试中自动包装组件
 */
function customRender(
  ui: ReactElement,
  options?: Omit<RenderOptions, 'wrapper'>
) {
  return render(ui, { ...options })
}

// 重新导出所有 testing-library 方法
export * from '@testing-library/react'
export { customRender as render }