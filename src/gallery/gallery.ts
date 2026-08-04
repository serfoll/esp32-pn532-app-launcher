import { convertFileSrc } from "@tauri-apps/api/core";
import type { Game } from "../types";

export interface GalleryHandlers {
  onContextMenu: (event: MouseEvent, gameId: string) => void;
}

export function renderGallery(container: HTMLElement, games: Game[], handlers: GalleryHandlers): void {
  container.innerHTML = "";

  if (games.length === 0) {
    const empty = document.createElement("p");
    empty.className = "empty-state";
    empty.textContent = "No games yet. Add a folder in Settings to scan for games.";
    container.appendChild(empty);
    return;
  }

  for (const game of games) {
    const card = document.createElement("div");
    card.className = "game-card" + (game.available ? "" : " unavailable");
    card.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      handlers.onContextMenu(e, game.id);
    });

    const img = document.createElement("img");
    img.className = "game-art";
    img.alt = game.name;
    if (game.artworkPath) {
      // Cache-bust: the artwork file at this path can be overwritten in
      // place (e.g. "Refresh artwork"), but the URL alone doesn't change,
      // so the webview would otherwise keep serving the stale cached image.
      img.src = `${convertFileSrc(game.artworkPath)}?t=${Date.now()}`;
    }
    card.appendChild(img);

    const label = document.createElement("div");
    label.className = "game-name";
    label.textContent = game.available ? game.name : `${game.name} (unavailable)`;
    card.appendChild(label);

    container.appendChild(card);
  }
}
