import type { Binding, Game } from "../types";

export interface ConfirmedGameInput {
  folderPath: string;
  name: string;
  exePath: string;
}

export function renderBindDialog(select: HTMLSelectElement, games: Game[]): void {
  select.innerHTML = "";
  for (const game of games) {
    const option = document.createElement("option");
    option.value = game.id;
    option.textContent = game.name;
    select.appendChild(option);
  }
}

/** Lists every current tag<->game binding with an Unbind button, so a
 * binding made in error (or a cart the user wants to repurpose) can be
 * cleared without deleting the game itself. */
export function renderBindingsList(
  container: HTMLElement,
  bindings: Binding[],
  games: Game[],
  onUnbind: (tagUid: string) => void,
): void {
  container.innerHTML = "";

  if (bindings.length === 0) {
    const empty = document.createElement("p");
    empty.className = "empty-state";
    empty.textContent = "No tags bound yet.";
    container.appendChild(empty);
    return;
  }

  const list = document.createElement("ul");
  list.className = "bindings-list";
  for (const binding of bindings) {
    const game = games.find((g) => g.id === binding.gameId);

    const item = document.createElement("li");
    const label = document.createElement("span");
    label.textContent = `${binding.tagUid} -> ${game?.name ?? "(game no longer in catalog)"}`;
    item.appendChild(label);

    const unbindButton = document.createElement("button");
    unbindButton.type = "button";
    unbindButton.textContent = "Unbind";
    unbindButton.addEventListener("click", () => onUnbind(binding.tagUid));
    item.appendChild(unbindButton);

    list.appendChild(item);
  }
  container.appendChild(list);
}
