import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  // Tauri drives this server, so the port must be fixed and a failure to bind
  // should be an error rather than a silent move to another port.
  server: { port: 5274, strictPort: true },
  clearScreen: false,
  build: {
    // The webviews this ships against: WKWebView on macOS, WebView2 on
    // Windows, WebKitGTK on Linux. es2021 is comfortably supported by all
    // three and avoids shipping transpiler helpers nobody needs.
    target: "es2021",
    // The installer is attached to a GitHub release and should stay a few
    // megabytes. No sourcemaps in the bundle; they more than double its size
    // and are no use once it is packaged.
    sourcemap: false,
    chunkSizeWarningLimit: 1200,
  },
});
