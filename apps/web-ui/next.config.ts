import type { NextConfig } from "next"

const nextConfig: NextConfig = {
  output: "export",
  reactStrictMode: true,
  experimental: {
    optimizePackageImports: ["lucide-react"],
  },
  transpilePackages: [
    "@agent-factory/runtime-client",
    "@agent-factory/theme",
    "@agent-factory/ui",
  ],
}

export default nextConfig
