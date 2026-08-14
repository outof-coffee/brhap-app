import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// In development Vite serves the UI and proxies the API to the rarma server.
// In production the server serves dist/ itself and no proxy is involved.
export default defineConfig({
  plugins: [svelte()],
  server: {
    proxy: {
      '/api': 'http://localhost:8787',
    },
  },
})
