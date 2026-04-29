import { defineConfig } from "vite";

export default defineConfig({
  root: "src", // points to your frontend folder
  base: "./",  // makes paths relative for Tauri
  server: {
    strictPort: true
  }
});