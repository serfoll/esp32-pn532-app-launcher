import type { Binding, Game, ScanCandidate } from "../types";

export interface ConfirmedGameInput {
  folderPath: string;
  name: string;
  exePath: string;
}

/** Renders one editable row per scanned candidate — the exe path is
 * pre-filled with the heuristic's guess but always user-editable, and the
 * include checkbox is unchecked by default when no exe was found. This is
 * the one required confirm step before a candidate ever becomes a Game. */
export function renderScanReview(container: HTMLElement, candidates: ScanCandidate[]): void {
  container.innerHTML = "";

  if (candidates.length === 0) {
    const empty = document.createElement("p");
    empty.textContent = "No subfolders found in that folder.";
    container.appendChild(empty);
    return;
  }

  for (const candidate of candidates) {
    const row = document.createElement("div");
    row.className = "scan-review-row";
    row.dataset.folderPath = candidate.folderPath;
    row.dataset.name = candidate.name;

    const include = document.createElement("input");
    include.type = "checkbox";
    include.className = "scan-review-include";
    include.checked = candidate.exePath !== null;
    row.appendChild(include);

    const label = document.createElement("span");
    label.className = "scan-review-name";
    label.textContent = candidate.name;
    row.appendChild(label);

    const exeInput = document.createElement("input");
    exeInput.className = "scan-review-exe";
    exeInput.value = candidate.exePath ?? "";
    exeInput.placeholder = candidate.exePath ? "" : "no exe found, enter path manually";
    row.appendChild(exeInput);

    container.appendChild(row);
  }
}

/** Reads back the rows a user confirmed (checked, with a non-empty exe
 * path) from a container previously rendered by renderScanReview. */
export function collectConfirmedGames(container: HTMLElement): ConfirmedGameInput[] {
  const games: ConfirmedGameInput[] = [];
  container.querySelectorAll<HTMLElement>(".scan-review-row").forEach((row) => {
    const include = row.querySelector<HTMLInputElement>(".scan-review-include");
    const exeInput = row.querySelector<HTMLInputElement>(".scan-review-exe");
    if (!include?.checked) return;

    const exePath = exeInput?.value.trim();
    if (!exePath) return;

    games.push({
      folderPath: row.dataset.folderPath!,
      name: row.dataset.name!,
      exePath,
    });
  });
  return games;
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
