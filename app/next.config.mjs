/** Static export for GitHub Pages (PRD §5). NEXT_PUBLIC_BASE_PATH = /<repo> on Pages. */
const basePath = process.env.NEXT_PUBLIC_BASE_PATH || "";
export default {
  output: "export",
  basePath,
  trailingSlash: true,
  images: { unoptimized: true },
  reactStrictMode: true,
};
