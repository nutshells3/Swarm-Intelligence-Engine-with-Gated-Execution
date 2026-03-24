import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://127.0.0.1:8845',
      '/health': 'http://127.0.0.1:8845',
    }
  },
  build: {
    outDir: '../../services/orchestration-api/static',
    emptyOutDir: true,
  }
})
