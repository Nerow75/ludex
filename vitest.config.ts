import { defineConfig, mergeConfig } from "vitest/config";
// @ts-expect-error type error without @types/node package
import process from "node:process";
import viteConfig from "./vite.config.ts";

// Fuseau fixé avant le lancement des processus de test, qui en héritent : dates et heures stables.
process.env.TZ = "Europe/Paris";

// Réutilise `vite.config.ts` (plugin React) ; fonctions pures seulement, donc pas de DOM.
export default defineConfig((env) =>
  mergeConfig(viteConfig(env), {
    test: {
      environment: "node",
      include: ["src/**/*.test.{ts,tsx}"],
    },
  }),
);
