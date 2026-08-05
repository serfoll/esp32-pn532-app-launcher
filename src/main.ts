import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open } from '@tauri-apps/plugin-dialog'
import type { Catalog, Game, ScanCandidate } from './types'
import { renderGallery } from './gallery/gallery'
import { renderGameTagsList } from './gallery/editGame'
import { renderSettings } from './settings/settings'
import {
  renderScanReview,
  collectConfirmedGames,
  renderBindDialog,
  renderBindingsList,
} from './binding/binding'
import { appendLog } from './log/log'

let catalog: Catalog

const galleryEl = document.querySelector<HTMLElement>('#gallery-grid')!
const settingsEl = document.querySelector<HTMLElement>('#settings-content')!
const bindingsListEl = document.querySelector<HTMLElement>('#bindings-list')!
const outputLogEl = document.querySelector<HTMLElement>('#output-log')!
const viewGallery = document.querySelector<HTMLElement>('#view-gallery')!
const viewBindings = document.querySelector<HTMLElement>('#view-bindings')!
const navGallery = document.querySelector<HTMLAnchorElement>('#nav-gallery')!
const navBindings = document.querySelector<HTMLAnchorElement>('#nav-bindings')!
const toastContainerEl = document.querySelector<HTMLElement>('#toast-container')!
const readerStatusEl = document.querySelector<HTMLElement>('#reader-status')!

const appMenuToggle =
  document.querySelector<HTMLButtonElement>('#app-menu-toggle')!
const appMenuEl = document.querySelector<HTMLElement>('#app-menu')!
const menuSettingsBtn =
  document.querySelector<HTMLButtonElement>('#menu-settings')!
const menuLogsCheckbox = document.querySelector<HTMLInputElement>(
  '#menu-logs-checkbox',
)!
const menuExitBtn = document.querySelector<HTMLButtonElement>('#menu-exit')!

const titlebarMinimizeBtn =
  document.querySelector<HTMLButtonElement>('#titlebar-minimize')!
const titlebarMaximizeBtn =
  document.querySelector<HTMLButtonElement>('#titlebar-maximize')!
const titlebarCloseBtn =
  document.querySelector<HTMLButtonElement>('#titlebar-close')!
const appWindow = getCurrentWindow()

const settingsDialog =
  document.querySelector<HTMLDialogElement>('#settings-dialog')!
let settingsNavItems = document.querySelectorAll<HTMLButtonElement>(
  '.settings-nav-item',
)
const settingsNavDev =
  document.querySelector<HTMLButtonElement>('#settings-nav-dev')!
const settingsPanelDev =
  document.querySelector<HTMLElement>('#settings-panel-dev')!
const closeBehaviorSelect = document.querySelector<HTMLSelectElement>(
  '#close-behavior-select',
)!
const confirmBeforeLaunchCheckbox = document.querySelector<HTMLInputElement>(
  '#confirm-before-launch-checkbox',
)!
const simulateInput = document.querySelector<HTMLInputElement>(
  '#simulate-tag-input',
)!
const simulateBtn = document.querySelector<HTMLButtonElement>(
  '#simulate-tag-button',
)!

const scanReviewDialog = document.querySelector<HTMLDialogElement>(
  '#scan-review-dialog',
)!
const scanReviewList = document.querySelector<HTMLElement>('#scan-review-list')!
const scanReviewConfirmBtn = document.querySelector<HTMLButtonElement>(
  '#scan-review-confirm',
)!
let scanReviewFolderPath = ''

const bindDialog = document.querySelector<HTMLDialogElement>('#bind-dialog')!
const bindTagLabel = document.querySelector<HTMLElement>('#bind-tag-uid')!
const bindSelect =
  document.querySelector<HTMLSelectElement>('#bind-game-select')!
const bindConfirmBtn =
  document.querySelector<HTMLButtonElement>('#bind-confirm')!
let bindTagUid = ''

const contextMenuEl = document.querySelector<HTMLElement>('#game-context-menu')!
const contextMenuEditBtn =
  document.querySelector<HTMLButtonElement>('#context-menu-edit')!
let contextMenuGameId = ''

const editGameDialog =
  document.querySelector<HTMLDialogElement>('#edit-game-dialog')!
const editGameNameInput =
  document.querySelector<HTMLInputElement>('#edit-game-name')!
const editGameSaveNameBtn = document.querySelector<HTMLButtonElement>(
  '#edit-game-save-name',
)!
const editGameChangeArtBtn = document.querySelector<HTMLButtonElement>(
  '#edit-game-change-art',
)!
const editGameTagsListEl = document.querySelector<HTMLElement>(
  '#edit-game-tags-list',
)!
let editGameId = ''

const closePromptDialog = document.querySelector<HTMLDialogElement>(
  '#close-prompt-dialog',
)!
const closePromptRemember = document.querySelector<HTMLInputElement>(
  '#close-prompt-remember',
)!
const closePromptMinimizeBtn = document.querySelector<HTMLButtonElement>(
  '#close-prompt-minimize',
)!
const closePromptQuitBtn =
  document.querySelector<HTMLButtonElement>('#close-prompt-quit')!

const progressDialog = document.querySelector<HTMLDialogElement>(
  '#progress-dialog',
)!
const progressDialogMessageEl = document.querySelector<HTMLElement>(
  '#progress-dialog-message',
)!
const launchErrorDialog = document.querySelector<HTMLDialogElement>(
  '#launch-error-dialog',
)!
const launchErrorMessageEl = document.querySelector<HTMLElement>(
  '#launch-error-message',
)!

const READER_STATUS_LABEL: Record<string, string> = {
  disconnected: 'Reader: disconnected',
  connectedUnknownFirmware: 'Reader: connected, needs firmware update',
  connectedReady: 'Reader: ready',
}

function updateReaderStatus(state: string): void {
  readerStatusEl.textContent = READER_STATUS_LABEL[state] ?? `Reader: ${state}`
  readerStatusEl.className = `reader-status reader-status--${state}`
}

const TOAST_AUTO_DISMISS_MS = 5000

function showAlert(message: string): void {
  const toast = document.createElement('div')
  toast.className = 'toast'

  const text = document.createElement('span')
  text.className = 'toast-message'
  text.textContent = message
  toast.appendChild(text)

  const dismissBtn = document.createElement('button')
  dismissBtn.type = 'button'
  dismissBtn.className = 'toast-dismiss'
  dismissBtn.setAttribute('aria-label', 'Dismiss')
  dismissBtn.textContent = '×'
  // .remove() is a no-op if the toast is already gone, so this and the
  // auto-dismiss timeout below can't double-fire into an error.
  dismissBtn.addEventListener('click', () => toast.remove())
  toast.appendChild(dismissBtn)

  toastContainerEl.appendChild(toast)
  setTimeout(() => toast.remove(), TOAST_AUTO_DISMISS_MS)
}

// Shared by any long-ish backend call (launching a game, refreshing
// artwork) that wants a modal "this is in progress" indicator instead of
// silently blocking the UI. Closes any dialog already open under this id
// first -- showModal() throws if called on a dialog that's already open,
// which two overlapping progress operations could otherwise hit.
function showProgressDialog(message: string): void {
  if (progressDialog.open) progressDialog.close()
  progressDialogMessageEl.textContent = message
  progressDialog.showModal()
}

function log(message: string): void {
  appendLog(outputLogEl, message)
}

/** Every Tauri command can reject (bad path, disk full, permission denied)
 * — routing all of them through here means a backend error always reaches
 * the user instead of dying as a silent unhandled rejection. */
async function invokeOrAlert<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T | undefined> {
  try {
    return await invoke<T>(command, args)
  } catch (e) {
    showAlert(`${command} failed: ${e}`)
    return undefined
  }
}

function showView(name: 'gallery' | 'bindings'): void {
  viewGallery.hidden = name !== 'gallery'
  viewBindings.hidden = name !== 'bindings'
  navGallery.classList.toggle('active', name === 'gallery')
  navBindings.classList.toggle('active', name === 'bindings')
}

function showSettingsSection(name: 'general' | 'game' | 'dev'): void {
  for (const item of settingsNavItems) {
    item.classList.toggle('active', item.dataset.section === name)
  }
  for (const panel of document.querySelectorAll<HTMLElement>('.settings-panel')) {
    panel.hidden = panel.id !== `settings-panel-${name}`
  }
}

function refresh(): void {
  renderGalleryView()
  renderSettings(settingsEl, catalog.settings, {
    onAddFolder: handleAddFolder,
    onRemoveFolder: handleRemoveFolder,
    onRefreshArtwork: handleRefreshArtwork,
  })
  confirmBeforeLaunchCheckbox.checked = catalog.settings.confirmBeforeLaunch
  renderBindingsList(
    bindingsListEl,
    catalog.bindings,
    catalog.games,
    handleUnbindTag,
  )
  outputLogEl.hidden = !catalog.settings.showOutputLog
  menuLogsCheckbox.checked = catalog.settings.showOutputLog
  closeBehaviorSelect.value = catalog.settings.closeBehavior
}

async function updateSettings(
  overrides: Partial<{
    rootFolders: string[]
    confirmBeforeLaunch: boolean
    showOutputLog: boolean
    closeBehavior: 'ask' | 'minimize' | 'quit'
  }>,
): Promise<void> {
  const result = await invokeOrAlert<Catalog>('update_settings', {
    rootFolders: catalog.settings.rootFolders,
    confirmBeforeLaunch: catalog.settings.confirmBeforeLaunch,
    showOutputLog: catalog.settings.showOutputLog,
    closeBehavior: catalog.settings.closeBehavior,
    ...overrides,
  })
  if (!result) return
  catalog = result
  refresh()
}

async function handleUnbindTag(tagUid: string): Promise<void> {
  const result = await invokeOrAlert<Catalog>('unbind_tag', { tagUid })
  if (!result) return
  catalog = result
  refresh()
  log(`Unbound tag ${tagUid}`)
}

async function loadCatalog(): Promise<void> {
  const result = await invokeOrAlert<Catalog>('get_catalog')
  if (!result) return
  catalog = result
  refresh()
}

async function loadReaderState(): Promise<void> {
  const state = await invokeOrAlert<string>('get_reader_state')
  if (state) updateReaderStatus(state)
}

async function handleAddFolder(path: string): Promise<void> {
  const candidates = await invokeOrAlert<ScanCandidate[]>('scan_folder', {
    path,
  })
  if (!candidates) return
  scanReviewFolderPath = path
  renderScanReview(scanReviewList, candidates)
  scanReviewDialog.showModal()
}

async function handleRefreshArtwork(): Promise<void> {
  settingsDialog.close()
  showProgressDialog('Refreshing artwork...')
  const result = await invokeOrAlert<Catalog>('refresh_all_artwork')
  progressDialog.close()
  if (!result) return
  catalog = result
  refresh()
  showAlert('Artwork refreshed.')
}

async function handleRemoveFolder(path: string): Promise<void> {
  const result = await invokeOrAlert<Catalog>('remove_root_folder', { path })
  if (!result) return
  catalog = result
  refresh()
}

async function handleToggleConfirmBeforeLaunch(value: boolean): Promise<void> {
  await updateSettings({ confirmBeforeLaunch: value })
}

async function handleToggleShowOutputLog(value: boolean): Promise<void> {
  await updateSettings({ showOutputLog: value })
}

async function handleChangeCloseBehavior(
  value: 'ask' | 'minimize' | 'quit',
): Promise<void> {
  await updateSettings({ closeBehavior: value })
}

function openBindDialog(tagUid: string): void {
  bindTagUid = tagUid
  bindTagLabel.textContent = tagUid
  renderBindDialog(bindSelect, catalog.games)
  bindDialog.showModal()
}

function showContextMenu(event: MouseEvent, gameId: string): void {
  contextMenuGameId = gameId
  contextMenuEl.style.left = `${event.clientX}px`
  contextMenuEl.style.top = `${event.clientY}px`
  contextMenuEl.hidden = false
}

function hideContextMenu(): void {
  contextMenuEl.hidden = true
}

function hideAppMenu(): void {
  appMenuEl.hidden = true
}

function refreshEditGameTagsList(): void {
  renderGameTagsList(
    editGameTagsListEl,
    editGameId,
    catalog.bindings,
    handleUnbindFromEditDialog,
  )
}

function openEditDialog(gameId: string): void {
  const game = catalog.games.find((g) => g.id === gameId)
  if (!game) return
  editGameId = gameId
  editGameNameInput.value = game.name
  refreshEditGameTagsList()
  editGameDialog.showModal()
}

async function handleSaveGameName(): Promise<void> {
  const name = editGameNameInput.value.trim()
  if (!name) return
  const result = await invokeOrAlert<Catalog>('rename_game', {
    gameId: editGameId,
    name,
  })
  if (!result) return
  catalog = result
  refresh()
  showAlert(`Renamed to "${name}".`)
  log(`Renamed game ${editGameId} -> "${name}"`)
}

async function handleChangeGameArt(): Promise<void> {
  const selected = await open({
    filters: [
      {
        name: 'Images',
        extensions: ['png', 'jpg', 'jpeg', 'bmp', 'webp', 'ico'],
      },
    ],
  })
  if (typeof selected !== 'string') return

  const result = await invokeOrAlert<Catalog>('set_custom_artwork', {
    gameId: editGameId,
    sourcePath: selected,
  })
  if (!result) return
  catalog = result
  refresh()
  const game = catalog.games.find((g) => g.id === editGameId)
  log(`Set custom artwork for "${game?.name ?? editGameId}"`)
}

async function handleUnbindFromEditDialog(tagUid: string): Promise<void> {
  const result = await invokeOrAlert<Catalog>('unbind_tag', { tagUid })
  if (!result) return
  catalog = result
  refresh()
  refreshEditGameTagsList()
  log(`Unbound tag ${tagUid}`)
}

// A tag held too close to the reader can make it flap inserted/removed
// repeatedly even though it never moved (the firmware also guards against
// this, but this is a second, independent line of defense so a repeat
// insert of the same tag can never fire a fresh launch/dialog-open faster
// than this).
const RECENT_TAG_EVENT_COOLDOWN_MS = 3000
const lastHandledTagEventAt = new Map<string, number>()

// Polled instead of pushed: the backend has no way to know when a
// launched process (or its launcher's real hand-off target) exits, so the
// frontend asks periodically rather than the backend trying to watch every
// possible child process tree.
const RUNNING_GAMES_POLL_INTERVAL_MS = 3000
// A launcher hand-off (Steam, EA App, etc.) can legitimately take a while
// to actually spawn its real process -- this is a "stop watching" timeout
// for the launching dialog, not a failure threshold, matching launchGame's
// existing under-report-rather-than-false-fail approach.
const LAUNCH_RUNNING_TIMEOUT_MS = 30000
let runningGameIds = new Set<string>()
const runningWaiters = new Map<string, () => void>()

function renderGalleryView(): void {
  renderGallery(galleryEl, catalog.games, catalog.bindings, runningGameIds, {
    onContextMenu: showContextMenu,
    onLaunch: handleLaunchFromGallery,
  })
}

async function pollRunningGames(): Promise<void> {
  if (!catalog) return // can race the initial catalog load, same as tag events
  const ids = await invokeOrAlert<string[]>('get_running_games')
  if (!ids) return
  runningGameIds = new Set(ids)
  for (const [gameId, resolve] of runningWaiters) {
    if (runningGameIds.has(gameId)) {
      runningWaiters.delete(gameId)
      resolve()
    }
  }
  renderGalleryView()
}

/** Resolves once gameId shows up as running on a poll tick, or after
 * timeoutMs regardless. */
function waitUntilRunning(gameId: string, timeoutMs: number): Promise<void> {
  if (runningGameIds.has(gameId)) return Promise.resolve()
  return new Promise((resolve) => {
    const timeout = setTimeout(() => {
      runningWaiters.delete(gameId)
      resolve()
    }, timeoutMs)
    runningWaiters.set(gameId, () => {
      clearTimeout(timeout)
      resolve()
    })
  })
}

/** Availability check, confirm-before-launch prompt, then the actual
 * launch_game invoke -- shared by tag-triggered launches and clicking a
 * game's icon directly in the gallery. Feedback is dialog-based: a
 * "Launching..." dialog with a progress bar stays open until the game
 * shows up as running (or waitUntilRunning gives up waiting), success
 * itself has no dialog since the gallery's running badge is the ongoing
 * signal, and any failure opens a dismissible error dialog. */
async function launchGame(game: Game): Promise<void> {
  if (!game.available) {
    launchErrorMessageEl.textContent = `"${game.name}" isn't installed/found right now.`
    launchErrorDialog.showModal()
    log(`"${game.name}" is unavailable, not launching`)
    return
  }

  if (
    catalog.settings.confirmBeforeLaunch &&
    !window.confirm(`Launch "${game.name}"?`)
  ) {
    log(`Launch of "${game.name}" cancelled at confirm prompt`)
    return
  }

  showProgressDialog(`Launching "${game.name}"...`)
  try {
    const launched = await invoke<boolean>('launch_game', {
      exePath: game.exePath,
      folderPath: game.folderPath,
    })
    if (launched) {
      log(`Launched "${game.name}"`)
      await waitUntilRunning(game.id, LAUNCH_RUNNING_TIMEOUT_MS)
    } else {
      log(`"${game.name}" is already running, not relaunching`)
    }
  } catch (e) {
    log(`Failed to launch "${game.name}": ${e}`)
    launchErrorMessageEl.textContent = `Couldn't launch "${game.name}": ${e}`
    launchErrorDialog.showModal()
  } finally {
    progressDialog.close()
  }
}

function handleLaunchFromGallery(gameId: string): void {
  const game = catalog.games.find((g) => g.id === gameId)
  if (game) launchGame(game)
}

/** Looks up a tag UID against the catalog and reacts: alerts on an
 * unavailable game, opens the bind dialog for an unbound tag, or launches
 * the bound game via launchGame. Shared by both the real serial
 * "tag-inserted" event and the dev simulate-insert control below. */
async function handleTagEvent(tagUid: string): Promise<void> {
  if (!catalog) return // a real insert can in principle race the initial catalog load

  const now = Date.now()
  const lastHandledAt = lastHandledTagEventAt.get(tagUid)
  if (
    lastHandledAt !== undefined &&
    now - lastHandledAt < RECENT_TAG_EVENT_COOLDOWN_MS
  ) {
    return
  }
  lastHandledTagEventAt.set(tagUid, now)

  log(`Tag inserted: ${tagUid}`)

  const binding = catalog.bindings.find((b) => b.tagUid === tagUid)
  if (!binding) {
    log(`Tag ${tagUid} is not bound to a game — opening bind dialog`)
    openBindDialog(tagUid)
    return
  }

  const game = catalog.games.find((g) => g.id === binding.gameId)
  if (!game) {
    showAlert(
      `Tag ${tagUid} is bound to a game that's no longer in the catalog.`,
    )
    log(`Tag ${tagUid} is bound to a game that's no longer in the catalog`)
    return
  }

  await launchGame(game)
}

function triggerSimulatedTagEvent(): void {
  const uid = simulateInput.value.trim()
  if (uid) {
    handleTagEvent(uid)
    simulateInput.value = ''
  }
}

scanReviewConfirmBtn.addEventListener('click', async () => {
  const games = collectConfirmedGames(scanReviewList)
  if (!(await invokeOrAlert<Catalog>('confirm_games', { games }))) return
  const result = await invokeOrAlert<Catalog>('add_root_folder', {
    path: scanReviewFolderPath,
  })
  if (!result) return
  catalog = result
  scanReviewDialog.close()
  refresh()
})

bindConfirmBtn.addEventListener('click', async () => {
  const gameId = bindSelect.value
  if (!gameId) return
  const result = await invokeOrAlert<Catalog>('bind_tag', {
    tagUid: bindTagUid,
    gameId,
  })
  if (!result) return
  catalog = result
  bindDialog.close()
  refresh()
  showAlert(`Bound tag ${bindTagUid}.`)
  log(
    `Bound tag ${bindTagUid} -> ${bindSelect.selectedOptions[0]?.textContent ?? gameId}`,
  )
})

simulateBtn.addEventListener('click', triggerSimulatedTagEvent)
simulateInput.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') {
    e.preventDefault()
    triggerSimulatedTagEvent()
  }
})

contextMenuEditBtn.addEventListener('click', () => {
  hideContextMenu()
  openEditDialog(contextMenuGameId)
})

editGameSaveNameBtn.addEventListener('click', handleSaveGameName)
editGameChangeArtBtn.addEventListener('click', handleChangeGameArt)

navGallery.addEventListener('click', (e) => {
  e.preventDefault()
  showView('gallery')
})
navBindings.addEventListener('click', (e) => {
  e.preventDefault()
  showView('bindings')
})

appMenuToggle.addEventListener('click', () => {
  appMenuEl.hidden = !appMenuEl.hidden
})
menuSettingsBtn.addEventListener('click', () => {
  hideAppMenu()
  showSettingsSection('general')
  settingsDialog.showModal()
})
menuLogsCheckbox.addEventListener('change', () =>
  handleToggleShowOutputLog(menuLogsCheckbox.checked),
)
menuExitBtn.addEventListener('click', () => {
  hideAppMenu()
  invokeOrAlert('resolve_close_prompt', { minimize: false, remember: false })
})
closeBehaviorSelect.addEventListener('change', () =>
  handleChangeCloseBehavior(
    closeBehaviorSelect.value as 'ask' | 'minimize' | 'quit',
  ),
)
confirmBeforeLaunchCheckbox.addEventListener('change', () =>
  handleToggleConfirmBeforeLaunch(confirmBeforeLaunchCheckbox.checked),
)

// The Dev section only makes sense against a running dev server -- a
// production build's simulate-tag control would have nothing real to
// compare against, so it's gated on Vite's own dev/prod distinction
// rather than a setting anyone has to remember to turn off.
if (import.meta.env.DEV) {
  settingsNavDev.hidden = false
} else {
  settingsNavDev.remove()
  settingsPanelDev.remove()
  settingsNavItems = document.querySelectorAll<HTMLButtonElement>(
    '.settings-nav-item',
  )
}

for (const item of settingsNavItems) {
  item.addEventListener('click', () => {
    showSettingsSection(item.dataset.section as 'general' | 'game' | 'dev')
  })
}

// Clicking the backdrop (i.e. the dialog element itself, outside its
// content box) dismisses Settings -- unlike the close-prompt dialog,
// there's no choice here that needs to resolve to something explicit.
settingsDialog.addEventListener('click', (e) => {
  if (e.target === settingsDialog) settingsDialog.close()
})

titlebarMinimizeBtn.addEventListener('click', () => {
  appWindow.minimize()
})
titlebarMaximizeBtn.addEventListener('click', () => {
  appWindow.toggleMaximize()
})
// close() emits closeRequested, which the Rust side already intercepts for
// the minimize-to-tray prompt -- same path as the OS close button used to
// take before decorations were disabled for the custom titlebar.
titlebarCloseBtn.addEventListener('click', () => {
  appWindow.close()
})

document.addEventListener('click', (e) => {
  if (!contextMenuEl.hidden && !contextMenuEl.contains(e.target as Node)) {
    hideContextMenu()
  }
  if (
    !appMenuEl.hidden &&
    !appMenuEl.contains(e.target as Node) &&
    e.target !== appMenuToggle
  ) {
    hideAppMenu()
  }
})

listen<string>('reader-state', (event) => {
  updateReaderStatus(event.payload)
  log(`Reader state: ${event.payload}`)
})
listen<string>('tag-inserted', (event) => handleTagEvent(event.payload))
listen<string>('tag-removed', (event) => log(`Tag removed: ${event.payload}`))
listen<string>('reader-error', (event) => {
  showAlert(`Reader: ${event.payload}`)
  log(`Reader error: ${event.payload}`)
})
listen('close-requested', () => closePromptDialog.showModal())

// Escape fires the dialog's native 'cancel' event, which would dismiss it
// without ever calling resolve_close_prompt -- the window's close request
// is still pending on the Rust side at that point, so the prompt has to
// resolve to an actual choice rather than being silently dismissable.
closePromptDialog.addEventListener('cancel', (e) => e.preventDefault())

async function resolveClosePrompt(minimize: boolean): Promise<void> {
  // Quitting exits the process before a response comes back, but minimizing
  // doesn't -- and a remembered choice made here needs to show up in the
  // Settings panel without waiting for a restart, so apply the returned
  // catalog the same way every other mutating command does.
  const result = await invokeOrAlert<Catalog>('resolve_close_prompt', {
    minimize,
    remember: closePromptRemember.checked,
  })
  if (result) {
    catalog = result
    refresh()
  }
  closePromptDialog.close()
}
closePromptMinimizeBtn.addEventListener('click', () => resolveClosePrompt(true))
closePromptQuitBtn.addEventListener('click', () => resolveClosePrompt(false))

window.addEventListener('DOMContentLoaded', () => {
  showView('gallery')
  loadCatalog()
  loadReaderState()
  pollRunningGames()
  setInterval(pollRunningGames, RUNNING_GAMES_POLL_INTERVAL_MS)
})
