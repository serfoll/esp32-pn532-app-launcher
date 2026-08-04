import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Catalog, ScanCandidate } from "./types";
import { renderGallery } from "./gallery/gallery";
import { renderSettings } from "./settings/settings";
import {
  renderScanReview,
  collectConfirmedGames,
  renderBindDialog,
  renderBindingsList,
} from "./binding/binding";

let catalog: Catalog;

const galleryEl = document.querySelector<HTMLElement>("#gallery-grid")!;
const settingsEl = document.querySelector<HTMLElement>("#settings-content")!;
const bindingsListEl = document.querySelector<HTMLElement>("#bindings-list")!;
const viewGallery = document.querySelector<HTMLElement>("#view-gallery")!;
const viewSettings = document.querySelector<HTMLElement>("#view-settings")!;
const navGallery = document.querySelector<HTMLButtonElement>("#nav-gallery")!;
const navSettings = document.querySelector<HTMLButtonElement>("#nav-settings")!;

const scanReviewDialog = document.querySelector<HTMLDialogElement>("#scan-review-dialog")!;
const scanReviewList = document.querySelector<HTMLElement>("#scan-review-list")!;
const scanReviewConfirmBtn = document.querySelector<HTMLButtonElement>("#scan-review-confirm")!;
let scanReviewFolderPath = "";

const bindDialog = document.querySelector<HTMLDialogElement>("#bind-dialog")!;
const bindTagLabel = document.querySelector<HTMLElement>("#bind-tag-uid")!;
const bindSelect = document.querySelector<HTMLSelectElement>("#bind-game-select")!;
const bindConfirmBtn = document.querySelector<HTMLButtonElement>("#bind-confirm")!;
let bindTagUid = "";

const simulateForm = document.querySelector<HTMLFormElement>("#simulate-tag-form")!;
const simulateInput = document.querySelector<HTMLInputElement>("#simulate-tag-input")!;
const alertBanner = document.querySelector<HTMLElement>("#alert-banner")!;
const readerStatusEl = document.querySelector<HTMLElement>("#reader-status")!;

const READER_STATUS_LABEL: Record<string, string> = {
  disconnected: "Reader: disconnected",
  connectedUnknownFirmware: "Reader: connected, needs firmware update",
  connectedReady: "Reader: ready",
};

function updateReaderStatus(state: string): void {
  readerStatusEl.textContent = READER_STATUS_LABEL[state] ?? `Reader: ${state}`;
  readerStatusEl.className = `reader-status reader-status--${state}`;
}

function showAlert(message: string): void {
  alertBanner.textContent = message;
  alertBanner.hidden = false;
}

/** Every Tauri command can reject (bad path, disk full, permission denied)
 * — routing all of them through here means a backend error always reaches
 * the user instead of dying as a silent unhandled rejection. */
async function invokeOrAlert<T>(command: string, args?: Record<string, unknown>): Promise<T | undefined> {
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    showAlert(`${command} failed: ${e}`);
    return undefined;
  }
}

function showView(name: "gallery" | "settings"): void {
  viewGallery.hidden = name !== "gallery";
  viewSettings.hidden = name !== "settings";
}

function refresh(): void {
  renderGallery(galleryEl, catalog.games);
  renderSettings(settingsEl, catalog.settings, {
    onAddFolder: handleAddFolder,
    onRemoveFolder: handleRemoveFolder,
    onToggleConfirmBeforeLaunch: handleToggleConfirmBeforeLaunch,
    onRefreshArtwork: handleRefreshArtwork,
  });
  renderBindingsList(bindingsListEl, catalog.bindings, catalog.games, handleUnbindTag);
}

async function handleUnbindTag(tagUid: string): Promise<void> {
  const result = await invokeOrAlert<Catalog>("unbind_tag", { tagUid });
  if (!result) return;
  catalog = result;
  refresh();
}

async function loadCatalog(): Promise<void> {
  const result = await invokeOrAlert<Catalog>("get_catalog");
  if (!result) return;
  catalog = result;
  refresh();
}

async function loadReaderState(): Promise<void> {
  const state = await invokeOrAlert<string>("get_reader_state");
  if (state) updateReaderStatus(state);
}

async function handleAddFolder(path: string): Promise<void> {
  const candidates = await invokeOrAlert<ScanCandidate[]>("scan_folder", { path });
  if (!candidates) return;
  scanReviewFolderPath = path;
  renderScanReview(scanReviewList, candidates);
  scanReviewDialog.showModal();
}

async function handleRefreshArtwork(): Promise<void> {
  showAlert("Refreshing artwork...");
  const result = await invokeOrAlert<Catalog>("refresh_all_artwork");
  if (!result) return;
  catalog = result;
  refresh();
  showAlert("Artwork refreshed.");
}

async function handleRemoveFolder(path: string): Promise<void> {
  const result = await invokeOrAlert<Catalog>("remove_root_folder", { path });
  if (!result) return;
  catalog = result;
  refresh();
}

async function handleToggleConfirmBeforeLaunch(value: boolean): Promise<void> {
  const result = await invokeOrAlert<Catalog>("update_settings", {
    rootFolders: catalog.settings.rootFolders,
    confirmBeforeLaunch: value,
  });
  if (!result) return;
  catalog = result;
  refresh();
}

function openBindDialog(tagUid: string): void {
  bindTagUid = tagUid;
  bindTagLabel.textContent = tagUid;
  renderBindDialog(bindSelect, catalog.games);
  bindDialog.showModal();
}

/** Looks up a tag UID against the catalog and reacts: alerts on an
 * unavailable game, opens the bind dialog for an unbound tag, or launches
 * the bound game (with a confirm prompt first if settings.confirmBeforeLaunch
 * is on). Shared by both the real serial "tag-inserted" event and the dev
 * simulate-insert form below. */
async function handleTagEvent(tagUid: string): Promise<void> {
  if (!catalog) return; // a real insert can in principle race the initial catalog load

  const binding = catalog.bindings.find((b) => b.tagUid === tagUid);
  if (!binding) {
    openBindDialog(tagUid);
    return;
  }

  const game = catalog.games.find((g) => g.id === binding.gameId);
  if (!game) {
    showAlert(`Tag ${tagUid} is bound to a game that's no longer in the catalog.`);
    return;
  }
  if (!game.available) {
    showAlert(`"${game.name}" is bound to this tag but isn't installed/found right now.`);
    return;
  }

  if (catalog.settings.confirmBeforeLaunch && !window.confirm(`Launch "${game.name}"?`)) {
    return;
  }

  try {
    await invoke("launch_game", { exePath: game.exePath });
    showAlert(`Launched "${game.name}".`);
  } catch (e) {
    showAlert(`Couldn't launch "${game.name}": ${e}`);
  }
}

scanReviewConfirmBtn.addEventListener("click", async () => {
  const games = collectConfirmedGames(scanReviewList);
  if (!(await invokeOrAlert<Catalog>("confirm_games", { games }))) return;
  const result = await invokeOrAlert<Catalog>("add_root_folder", { path: scanReviewFolderPath });
  if (!result) return;
  catalog = result;
  scanReviewDialog.close();
  refresh();
});

bindConfirmBtn.addEventListener("click", async () => {
  const gameId = bindSelect.value;
  if (!gameId) return;
  const result = await invokeOrAlert<Catalog>("bind_tag", { tagUid: bindTagUid, gameId });
  if (!result) return;
  catalog = result;
  bindDialog.close();
  refresh();
  showAlert(`Bound tag ${bindTagUid}.`);
});

simulateForm.addEventListener("submit", (e) => {
  e.preventDefault();
  const uid = simulateInput.value.trim();
  if (uid) {
    handleTagEvent(uid);
    simulateInput.value = "";
  }
});

navGallery.addEventListener("click", () => showView("gallery"));
navSettings.addEventListener("click", () => showView("settings"));

listen<string>("reader-state", (event) => updateReaderStatus(event.payload));
listen<string>("tag-inserted", (event) => handleTagEvent(event.payload));
listen<string>("reader-error", (event) => showAlert(`Reader: ${event.payload}`));
// tag-removed has no UI reaction yet — nothing in the spec asks for one.

window.addEventListener("DOMContentLoaded", () => {
  showView("gallery");
  loadCatalog();
  loadReaderState();
});
