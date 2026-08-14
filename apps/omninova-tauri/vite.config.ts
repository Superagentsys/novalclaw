import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  // Relative asset URLs work both in Tauri and when Gateway serves the SPA at /app.
  base: './',
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      '/api': { target: 'http://127.0.0.1:10809', changeOrigin: true },
      '/health': { target: 'http://127.0.0.1:10809', changeOrigin: true },
      '/chat': { target: 'http://127.0.0.1:10809', changeOrigin: true },
      '/ws': { target: 'http://127.0.0.1:10809', ws: true, changeOrigin: true },
    },
  },
})
