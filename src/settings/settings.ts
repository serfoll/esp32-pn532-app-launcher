import { open } from "@tauri-apps/plugin-dialog";
import type { Settings } from "../types";

export interface SettingsHandlers {
  onAddFolder: (path: string) => void;
  onRemoveFolder: (path: string) => void;
  onToggleConfirmBeforeLaunch: (value: boolean) => void;
  onRefreshArtwork: () => void;
}

export function renderSettings(
  container: HTMLElement,
  settings: Settings,
  handlers: SettingsHandlers,
): void {
  container.innerHTML = "";

  const folderSection = document.createElement("section");
  const heading = document.createElement("h2");
  heading.textContent = "Root folders";
  folderSection.appendChild(heading);

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
  folderSection.appendChild(list);

  const form = document.createElement("form");
  form.className = "add-folder-form";
  const input = document.createElement("input");
  input.placeholder = "C:\\Games";
  input.required = true;
  const scanButton = document.createElement("button");
  scanButton.type = "submit";
  scanButton.textContent = "Scan folder";
  const browseButton = document.createElement("button");
  browseButton.type = "button";
  browseButton.textContent = "Browse...";
  browseButton.addEventListener("click", async () => {
    const selected = await open({ directory: true });
    if (typeof selected === "string") {
      handlers.onAddFolder(selected);
    }
  });
  form.appendChild(input);
  form.appendChild(browseButton);
  form.appendChild(scanButton);
  form.addEventListener("submit", (e) => {
    e.preventDefault();
    const path = input.value.trim();
    if (path) {
      handlers.onAddFolder(path);
      input.value = "";
    }
  });
  folderSection.appendChild(form);
  container.appendChild(folderSection);

  const launchSection = document.createElement("section");
  const label = document.createElement("label");
  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.checked = settings.confirmBeforeLaunch;
  checkbox.addEventListener("change", () =>
    handlers.onToggleConfirmBeforeLaunch(checkbox.checked),
  );
  label.appendChild(checkbox);
  label.append(" Confirm before launch");
  launchSection.appendChild(label);
  container.appendChild(launchSection);

  const artworkSection = document.createElement("section");
  const refreshButton = document.createElement("button");
  refreshButton.type = "button";
  refreshButton.textContent = "Refresh artwork";
  refreshButton.title = "Re-fetch artwork for every game, including ones added before an artwork source was set up";
  refreshButton.addEventListener("click", () => handlers.onRefreshArtwork());
  artworkSection.appendChild(refreshButton);
  container.appendChild(artworkSection);
}
