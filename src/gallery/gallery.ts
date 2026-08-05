import { convertFileSrc } from "@tauri-apps/api/core";
import steamIcon from "../assets/steam.svg";
import type { Binding, Game, Store } from "../types";

const STORE_ICONS: Record<Store, string> = {
  steam: steamIcon,
};

const STORE_NAMES: Record<Store, string> = {
  steam: "Steam",
};

// games/bindings/runningGameIds always travel together as "the gallery's
// current state" -- bundled so renderGallery doesn't keep growing a
// positional parameter list every time it needs one more piece of it.
export interface GalleryState {
  games: Game[];
  bindings: Binding[];
  runningGameIds: ReadonlySet<string>;
  showStoreBadges: boolean;
}

export interface GalleryHandlers {
  onContextMenu: (event: MouseEvent, gameId: string) => void;
  onLaunch: (gameId: string) => void;
}

export function renderGallery(
  container: HTMLElement,
  state: GalleryState,
  handlers: GalleryHandlers,
): void {
  const { games, bindings, runningGameIds, showStoreBadges } = state;
  container.innerHTML = "";

  if (games.length === 0) {
    const empty = document.createElement("p");
    empty.className = "empty-state";
    empty.textContent = "No games yet. Add a folder in Settings to scan for games.";
    container.appendChild(empty);
    return;
  }

  const boundGameIds = new Set(bindings.map((b) => b.gameId));
  // Stable sort (guaranteed since ES2019): bound games float to the front,
  // otherwise games keep their original catalog order within each group.
  const sortedGames = [...games].sort(
    (a, b) => Number(boundGameIds.has(b.id)) - Number(boundGameIds.has(a.id)),
  );

  for (const game of sortedGames) {
    const card = document.createElement("div");
    card.className = "game-card" + (game.available ? "" : " unavailable");
    card.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      handlers.onContextMenu(e, game.id);
    });

    const isBound = boundGameIds.has(game.id);
    const iconButton = document.createElement("button");
    iconButton.type = "button";
    iconButton.className =
      "game-art-button " + (isBound ? "game-art-button--bound" : "game-art-button--unbound");
    // Not disabled when unavailable -- launchGame already alerts with a
    // reason in that case. A disabled button would swallow the click
    // silently, leaving no way to find out why a game won't launch (unlike
    // the tag-insert flow, which always explains).
    //
    // The border color is the only visual signal for binding status, which
    // fails on color alone for low-vision/color-blind users -- stating it
    // in the accessible name keeps it available to screen readers even
    // though sighted-but-color-blind users still only get the border.
    iconButton.setAttribute(
      "aria-label",
      `Launch ${game.name} (${isBound ? "tag bound" : "no tag bound"})`,
    );
    iconButton.addEventListener("click", () => handlers.onLaunch(game.id));

    const img = document.createElement("img");
    img.className = "game-art";
    // Decorative here -- the button's aria-label already names the game,
    // so a repeated alt text would just double up for screen readers.
    img.alt = "";
    if (game.artworkPath) {
      // Cache-bust: the artwork file at this path can be overwritten in
      // place (e.g. "Refresh artwork"), but the URL alone doesn't change,
      // so the webview would otherwise keep serving the stale cached image.
      img.src = `${convertFileSrc(game.artworkPath)}?t=${Date.now()}`;
    }
    iconButton.appendChild(img);

    const playIcon = document.createElement("span");
    playIcon.className = "game-play-icon";
    playIcon.setAttribute("aria-hidden", "true");
    playIcon.textContent = "▶";
    iconButton.appendChild(playIcon);

    if (showStoreBadges && game.store && STORE_ICONS[game.store]) {
      const storeBadge = document.createElement("span");
      storeBadge.className = "game-store-badge";
      storeBadge.title = `Installed via ${STORE_NAMES[game.store]}`;

      const storeIcon = document.createElement("img");
      storeIcon.className = "game-store-badge-icon";
      storeIcon.src = STORE_ICONS[game.store];
      storeIcon.alt = "";
      storeBadge.appendChild(storeIcon);

      const storeLabel = document.createElement("span");
      storeLabel.textContent = STORE_NAMES[game.store];
      storeBadge.appendChild(storeLabel);

      iconButton.appendChild(storeBadge);
    }

    if (runningGameIds.has(game.id)) {
      const runningBadge = document.createElement("span");
      runningBadge.className = "game-running-badge";
      runningBadge.title = `${game.name} is running`;
      iconButton.appendChild(runningBadge);
    }

    card.appendChild(iconButton);

    const label = document.createElement("div");
    label.className = "game-name";
    label.textContent = game.available ? game.name : `${game.name} (unavailable)`;
    card.appendChild(label);

    container.appendChild(card);
  }
}
