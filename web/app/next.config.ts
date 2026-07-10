import type { NextConfig } from 'next'

const nextConfig: NextConfig = {
  // The oRPC API routes call `roco run-input <file>` via exec.
  // Allow the backend to install the binary at build time.
  output: 'standalone',
  experimental: {
    // Turbopack is fine; we keep default settings.
  },
}

export default nextConfig
