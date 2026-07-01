import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

// base '/admin/' so hashed asset URLs resolve under the admin mount, which the
// Worker serves from ASSETS by stripping the '/admin' prefix.
export default defineConfig({
  base: "/admin/",
  plugins: [preact()],
  build: { outDir: "dist", emptyOutDir: true },
});
