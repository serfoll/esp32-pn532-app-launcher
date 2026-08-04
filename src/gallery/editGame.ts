import type { Binding } from "../types";

/** Lists the tags bound to one specific game, each with an Unbind button --
 * the per-game counterpart to binding.ts's renderBindingsList, which shows
 * every binding across the whole catalog. */
export function renderGameTagsList(
  container: HTMLElement,
  gameId: string,
  bindings: Binding[],
  onUnbind: (tagUid: string) => void,
): void {
  container.innerHTML = "";
  const gameBindings = bindings.filter((b) => b.gameId === gameId);

  if (gameBindings.length === 0) {
    const empty = document.createElement("p");
    empty.className = "empty-state";
    empty.textContent = "No tags bound to this game yet.";
    container.appendChild(empty);
    return;
  }

  const list = document.createElement("ul");
  list.className = "bindings-list";
  for (const binding of gameBindings) {
    const item = document.createElement("li");

    const label = document.createElement("span");
    label.textContent = binding.tagUid;
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
