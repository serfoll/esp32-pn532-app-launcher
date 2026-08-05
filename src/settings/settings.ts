import { open } from "@tauri-apps/plugin-dialog";
import type { Settings } from "../types";

export interface SettingsHandlers {
  onAddFolder: (path: string) => void;
  onRemoveFolder: (path: string) => void;
  onRefreshArtwork: () => void;
}

export function renderSettings(
  container: HTMLElement,
  settings: Settings,
  handlers: SettingsHandlers,
): void {
  container.innerHTML = "";

  const list = document.createElement("ul");
  list.className = "root-folder-list";
  for (const folder of settings.rootFolders) {
    const item = document.createElement("li");

    const label = document.createElement("span");
    label.textContent = folder;
    item.appendChild(label);

    const removeButton = document.createElement("button");
    removeButton.type = "button";
    removeButton.textContent = "Remove";
    removeButton.addEventListener("click", () => handlers.onRemoveFolder(folder));
    item.appendChild(removeButton);

    list.appendChild(item);
  }
  container.appendChild(list);

  const browseButton = document.createElement("button");
  browseButton.type = "button";
  browseButton.textContent = "+ Add Collection";
  browseButton.addEventListener("click", async () => {
    const selected = await open({ directory: true });
    if (typeof selected === "string") {
      handlers.onAddFolder(selected);
    }
  });
  container.appendChild(browseButton);

  const refreshButton = document.createElement("button");
  refreshButton.type = "button";
  refreshButton.textContent = "Refresh artwork";
  refreshButton.title = "Re-fetch artwork for every game, including ones added before an artwork source was set up";
  refreshButton.addEventListener("click", () => handlers.onRefreshArtwork());
  container.appendChild(refreshButton);
}
