/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "export",
  trailingSlash: true,
  images: {
    unoptimized: true,
  },
  reactStrictMode: false,
  // Tauri uses fixed port 3000 in dev
  env: {
    TAURI: "true",
  },
};

export default nextConfig;
