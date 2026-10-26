import { appearance } from "#lib/shell/appearance.svelte.js";

// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
export const ssr = false;

// Тема и режим движения известны до первой отрисовки: иначе при «Быстро» успевает
// мелькнуть светлая тема и входная анимация, которую потом обрывает no-motion.
export async function load() {
  await appearance.load();
  return {};
}
