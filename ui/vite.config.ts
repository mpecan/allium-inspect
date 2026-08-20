import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import { defineConfig } from "vitest/config";

// The bundle is embedded in the Rust binary by rust-embed, which reads it from
// the server crate's `assets` directory. Building straight there means `just
// build` is two commands with no copy step between them that could go stale.
const OUT_DIR = "../crates/inspect-server/assets";

export default defineConfig({
  // `svelteTesting` resolves the browser condition and unmounts components
  // between tests. Its config hook returns early unless VITEST is set, so the
  // shipped bundle is byte-identical with and without it.
  plugins: [svelte(), svelteTesting()],
  // Do not obscure Rust errors when this runs as part of `just build`.
  clearScreen: false,
  build: {
    outDir: OUT_DIR,
    emptyOutDir: true,
    // The tool is served from localhost by a binary the user already trusts;
    // a source map costs nothing here and makes a stack trace in the browser
    // point at real code.
    sourcemap: true,
  },
  server: {
    // The dev server proxies to whatever port `allium-inspect --no-open`
    // bound, so the UI can hot-reload against a real graph. The port is
    // printed by the binary on startup.
    proxy: {
      "/api": {
        target: process.env.INSPECT_API ?? "http://127.0.0.1:7171",
        changeOrigin: true,
      },
    },
  },
  test: {
    environment: "happy-dom",
    globals: true,
    include: ["src/**/*.test.ts"],
    coverage: {
      provider: "v8",
      include: ["src/**/*.ts", "src/**/*.svelte"],
      // The generated bindings are types only: they emit no runtime code, so
      // including them would divide by a zero-statement denominator.
      exclude: ["src/lib/api/*.ts", "src/main.ts", "**/*.test.ts"],
      thresholds: { lines: 85, functions: 85, branches: 85, statements: 85 },
    },
  },
});
