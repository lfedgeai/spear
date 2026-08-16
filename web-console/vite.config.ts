import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const rootDir = path.dirname(fileURLToPath(import.meta.url))

export default defineConfig({
  plugins: [react()],
  resolve: {
    dedupe: ['react', 'react-dom'],
    alias: {
      '@': path.resolve(rootDir, './src'),
      react: path.resolve(rootDir, './node_modules/react'),
      'react-dom': path.resolve(rootDir, './node_modules/react-dom'),
      'react/jsx-runtime': path.resolve(rootDir, './node_modules/react/jsx-runtime.js'),
      'react/jsx-dev-runtime': path.resolve(rootDir, './node_modules/react/jsx-dev-runtime.js'),
    },
  },
  server: {
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8080',
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: path.resolve(rootDir, '../assets/console'),
    emptyOutDir: true,
    assetsDir: '',
    sourcemap: false,
    cssCodeSplit: false,
    rollupOptions: {
      output: {
        entryFileNames: 'main.js',
        chunkFileNames: 'main.js',
        assetFileNames: (assetInfo: { name?: string }) => {
          if (assetInfo.name && assetInfo.name.endsWith('.css')) return 'main.css'
          return '[name][extname]'
        },
        inlineDynamicImports: true,
      },
    },
  },
  base: '/console/static/',
})
