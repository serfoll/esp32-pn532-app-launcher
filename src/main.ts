import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open } from '@tauri-apps/plugin-dialog'
import {
  enable as enableAutostart,
  disable as disableAutostart,
  isEnabled as isAutostartEnabled,
} from '@tauri-apps/plugin-autostart'
import type {
  Catalog,
  ConfirmResult,
  FlashProgressPayload,
  Game,
  ScanCandidate,
  SyncResult,
} from './types'
import { renderGallery } from './gallery/gallery'
import { renderGameTagsList } from './gallery/editGame'
import { renderSettings } from './settings/settings'
import { renderBindDialog, renderBindingsList, type ConfirmedGameInput } from './binding/binding'
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
const flashFirmwareButton = document.querySelector<HTMLButtonElement>(
  '#flash-firmware-button',
)!

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
const showStoreBadgesCheckbox = document.querySelector<HTMLInputElement>(
  '#show-store-badges-checkbox',
)!
const syncOnStartupCheckbox = document.querySelector<HTMLInputElement>(
  '#sync-on-startup-checkbox',
)!
const launchOnStartupCheckbox = document.querySelector<HTMLInputElement>(
  '#launch-on-startup-checkbox',
)!
const simulateInput = document.querySelector<HTMLInputElement>(
  '#simulate-tag-input',
)!
const simulateBtn = document.querySelector<HTMLButtonElement>(
  '#simulate-tag-button',
)!
const devFlashFirmwareButton = document.querySelector<HTMLButtonElement>(
  '#dev-flash-firmware-button',
)!

const syncLibraryButton = document.querySelector<HTMLButtonElement>(
  '#sync-library-button',
)!

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
const progressDialogBarEl = document.querySelector<HTMLProgressElement>(
  '#progress-dialog-bar',
)!
// No close button (nothing to cancel -- the underlying command has no
// abort mechanism and keeps running either way) means Escape shouldn't be
// able to dismiss it either; a <dialog> still fires 'cancel' on Escape
// even with no close button in its markup.
progressDialog.addEventListener('cancel', (e) => e.preventDefault())
const launchErrorDialog = document.querySelector<HTMLDialogElement>(
  '#launch-error-dialog',
)!
const launchErrorMessageEl = document.querySelector<HTMLElement>(
  '#launch-error-message',
)!

const confirmDialog = document.querySelector<HTMLDialogElement>('#confirm-dialog')!
const confirmDialogTitleEl = document.querySelector<HTMLElement>('#confirm-dialog-title')!
const confirmDialogMessageEl = document.querySelector<HTMLElement>('#confirm-dialog-message')!
const confirmDialogConfirmBtn = document.querySelector<HTMLButtonElement>(
  '#confirm-dialog-confirm',
)!
confirmDialogConfirmBtn.addEventListener('click', () => confirmDialog.close('confirmed'))

/** Styled replacement for window.confirm(), matching the rest of the
 * app's dialog chrome. Resolves true only when the Confirm button was
 * clicked (dialog.close('confirmed')) -- the Cancel button's plain
 * form-submit and Escape both close with an empty returnValue, so both
 * read as "cancelled" the same way. */
function showConfirmDialog(options: {
  title: string
  message?: string
  confirmLabel: string
}): Promise<boolean> {
  confirmDialogTitleEl.textContent = options.title
  confirmDialogMessageEl.textContent = options.message ?? ''
  confirmDialogMessageEl.hidden = !options.message
  confirmDialogConfirmBtn.textContent = options.confirmLabel

  confirmDialog.returnValue = ''
  if (confirmDialog.open) confirmDialog.close()
  confirmDialog.showModal()

  return new Promise((resolve) => {
    confirmDialog.addEventListener(
      'close',
      () => resolve(confirmDialog.returnValue === 'confirmed'),
      { once: true },
    )
  })
}

const READER_STATUS_LABEL: Record<string, string> = {
  disconnected: 'Reader: disconnected',
  connectedUnknownFirmware: 'Reader: connected, needs firmware update',
  connectedReady: 'Reader: ready',
}

function updateReaderStatus(state: string): void {
  readerStatusEl.textContent = READER_STATUS_LABEL[state] ?? `Reader: ${state}`
  readerStatusEl.className = `reader-status reader-status--${state}`
  flashFirmwareButton.hidden = state !== 'connectedUnknownFirmware'
}

async function handleFlashFirmware(): Promise<void> {
  const proceed = await showConfirmDialog({
    title: 'Flash firmware?',
    message: "This writes the app's bundled firmware to the connected board. Don't unplug it during the process.",
    confirmLabel: 'Flash',
  })
  if (!proceed) {
    log('Firmware flash cancelled at confirm prompt')
    return
  }

  lastFlashStage = null
  showProgressDialog('Flashing firmware... this can take a couple of minutes.')
  const result = await invokeOrAlert<null>('flash_firmware')
  progressDialog.close()
  if (result === undefined) return // invokeOrAlert already surfaced the error

  log('Firmware flashed successfully')
  showToast('Firmware flashed.')
}

const TOAST_AUTO_DISMISS_MS = 5000

function showToast(message: string): void {
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
  // Indeterminate by default -- only flash-progress events (see below)
  // turn this into a real percentage bar, and a stale value from a
  // previous flash shouldn't leak into some other operation's dialog.
  progressDialogBarEl.removeAttribute('value')
  progressDialogBarEl.removeAttribute('max')
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
    showToast(`${command} failed: ${e}`)
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
  showStoreBadgesCheckbox.checked = catalog.settings.showStoreBadges
  syncOnStartupCheckbox.checked = catalog.settings.syncOnStartup
  renderBindingsList(
    bindingsListEl,
    catalog.bindings,
    catalog.games,
    handleRebindTag,
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
    showStoreBadges: boolean
    syncOnStartup: boolean
  }>,
): Promise<void> {
  const result = await invokeOrAlert<Catalog>('update_settings', {
    rootFolders: catalog.settings.rootFolders,
    confirmBeforeLaunch: catalog.settings.confirmBeforeLaunch,
    showOutputLog: catalog.settings.showOutputLog,
    closeBehavior: catalog.settings.closeBehavior,
    showStoreBadges: catalog.settings.showStoreBadges,
    syncOnStartup: catalog.settings.syncOnStartup,
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
  showProgressDialog('Adding folder...')
  const candidates = await invokeOrAlert<ScanCandidate[]>('scan_folder', {
    path,
  })
  if (!candidates) {
    progressDialog.close()
    return
  }

  const skipped = candidates.filter((c) => c.exePath === null)
  const games: ConfirmedGameInput[] = candidates
    .filter((c): c is ScanCandidate & { exePath: string } => c.exePath !== null)
    .map((c) => ({ folderPath: c.folderPath, name: c.name, exePath: c.exePath }))

  if (skipped.length > 0) {
    log(
      `Skipped ${skipped.length} folder(s) in "${path}" (couldn't detect an exe): ` +
        skipped.map((c) => c.name).join(', '),
    )
  }

  const confirmResult = await invokeOrAlert<ConfirmResult>('confirm_games', { games })
  if (!confirmResult) {
    progressDialog.close()
    return
  }

  const result = await invokeOrAlert<Catalog>('add_root_folder', { path })
  progressDialog.close()
  if (!result) return
  catalog = result
  refresh()

  // confirmResult.added, not games.length -- re-adding a previously
  // removed folder finds the same candidates again, but confirm_games
  // skips ones already in the catalog (they were never deleted, only
  // marked unavailable), so games.length can overstate what actually got
  // (re)confirmed.
  if (games.length === 0 && skipped.length === 0) {
    showToast(`No games found in "${path}".`)
  } else if (confirmResult.added === 0) {
    showToast(`"${path}" is already in your library.`)
  } else {
    showToast(
      `Added ${confirmResult.added} game(s) from "${path}".` +
        (skipped.length > 0 ? ` ${skipped.length} skipped: exe not detected (see Logs).` : ''),
    )
  }
}

async function handleSyncLibrary(): Promise<void> {
  showProgressDialog('Syncing library...')
  const result = await invokeOrAlert<SyncResult>('sync_library')
  progressDialog.close()
  if (!result) return
  catalog = result.catalog
  refresh()

  if (result.skippedNames.length > 0) {
    log(`Sync skipped ${result.skippedNames.length} folder(s) (couldn't detect an exe): ${result.skippedNames.join(', ')}`)
  }

  if (result.added === 0 && result.skippedNames.length === 0) {
    showToast('Library is up to date.')
  } else {
    showToast(
      `Added ${result.added} new game(s).` +
        (result.skippedNames.length > 0
          ? ` ${result.skippedNames.length} skipped: exe not detected (see Logs).`
          : ''),
    )
  }
}

async function handleRefreshArtwork(): Promise<void> {
  settingsDialog.close()
  showProgressDialog('Refreshing artwork...')
  const result = await invokeOrAlert<Catalog>('refresh_all_artwork')
  progressDialog.close()
  if (!result) return
  catalog = result
  refresh()
  showToast('Artwork refreshed.')
}

function isUnderFolder(path: string, root: string): boolean {
  const normalizedRoot = root.toLowerCase().replace(/\\+$/, '')
  const normalizedPath = path.toLowerCase()
  return normalizedPath === normalizedRoot || normalizedPath.startsWith(normalizedRoot + '\\')
}

async function handleRemoveFolder(path: string): Promise<void> {
  const affectedGameIds = new Set(
    catalog.games.filter((g) => isUnderFolder(g.folderPath, path)).map((g) => g.id),
  )
  const affectedBindings = catalog.bindings.filter((b) => affectedGameIds.has(b.gameId))

  if (affectedBindings.length > 0) {
    const lines = affectedBindings.map((b) => {
      const game = catalog.games.find((g) => g.id === b.gameId)
      return `${b.tagUid} -> ${game?.name ?? b.gameId}`
    })
    const proceed = await showConfirmDialog({
      title: `Remove "${path}"?`,
      message: `${affectedBindings.length} tag(s) are bound to games in this folder:\n\n${lines.join('\n')}\n\nRemoving it will make those games unavailable.`,
      confirmLabel: 'Remove',
    })
    if (!proceed) {
      log(`Removal of "${path}" cancelled at confirm prompt`)
      return
    }
  }

  const result = await invokeOrAlert<Catalog>('remove_root_folder', { path })
  if (!result) return
  catalog = result
  refresh()
}

async function handleToggleConfirmBeforeLaunch(value: boolean): Promise<void> {
  await updateSettings({ confirmBeforeLaunch: value })
}

async function handleToggleShowStoreBadges(value: boolean): Promise<void> {
  await updateSettings({ showStoreBadges: value })
}

async function handleToggleSyncOnStartup(value: boolean): Promise<void> {
  await updateSettings({ syncOnStartup: value })
}

// Not stored in catalog.json -- the OS registration (registry Run key)
// is its own persistent source of truth, and duplicating it as a
// settings field would risk drifting out of sync with it (e.g. if the
// user removes the startup entry via Windows' own Settings app).
async function handleToggleLaunchOnStartup(value: boolean): Promise<void> {
  try {
    if (value) {
      await enableAutostart()
    } else {
      await disableAutostart()
    }
  } catch (e) {
    log(`Failed to ${value ? 'enable' : 'disable'} launch on startup: ${e}`)
    showToast(`Couldn't ${value ? 'enable' : 'disable'} launch on startup.`)
    launchOnStartupCheckbox.checked = !value // reflects reality: the change didn't take
  }
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
  // Pre-selects the tag's current game when it already has a binding
  // (rebinding), leaves the browser's default (first option) otherwise.
  const currentGameId = catalog.bindings.find((b) => b.tagUid === tagUid)?.gameId
  renderBindDialog(bindSelect, catalog.games, currentGameId)
  bindDialog.showModal()
}

function handleRebindTag(tagUid: string): void {
  openBindDialog(tagUid)
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
  showToast(`Renamed to "${name}".`)
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
  renderGallery(
    galleryEl,
    {
      games: catalog.games,
      bindings: catalog.bindings,
      runningGameIds,
      showStoreBadges: catalog.settings.showStoreBadges,
    },
    { onContextMenu: showContextMenu, onLaunch: handleLaunchFromGallery, onStop: handleStopGame },
  )
}

async function pollRunningGames(): Promise<void> {
  if (!catalog) return // can race the initial catalog load, same as tag events
  const ids = await invokeOrAlert<string[]>('get_running_games')
  if (!ids) return

  const nextRunningGameIds = new Set(ids)
  // renderGalleryView rebuilds every card from scratch (including a
  // cache-busted <img src>, forcing every artwork file to reload from
  // disk) -- calling it on every 3s poll regardless of whether anything
  // actually changed was visible as the whole gallery flashing that often.
  const changed =
    nextRunningGameIds.size !== runningGameIds.size ||
    [...nextRunningGameIds].some((id) => !runningGameIds.has(id))
  runningGameIds = nextRunningGameIds

  for (const [gameId, resolve] of runningWaiters) {
    if (runningGameIds.has(gameId)) {
      runningWaiters.delete(gameId)
      resolve()
    }
  }

  if (changed) renderGalleryView()
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
    !(await showConfirmDialog({ title: `Launch "${game.name}"?`, confirmLabel: 'Launch' }))
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

async function handleStopGame(gameId: string): Promise<void> {
  const game = catalog.games.find((g) => g.id === gameId)
  if (!game) return

  // Always confirmed, unlike launch (where confirmation is opt-in via
  // confirmBeforeLaunch) -- stopping force-kills the process with no save
  // prompt, so an accidental click here loses progress launching never
  // risks.
  const stopConfirmed = await showConfirmDialog({
    title: `Stop "${game.name}"?`,
    message: 'Any unsaved progress will be lost.',
    confirmLabel: 'Stop',
  })
  if (!stopConfirmed) {
    log(`Stop of "${game.name}" cancelled at confirm prompt`)
    return
  }

  // Optimistic: reflects the stop immediately instead of waiting up to
  // RUNNING_GAMES_POLL_INTERVAL_MS for the next poll tick to notice. If the
  // stop actually failed, that same next tick corrects it back.
  runningGameIds.delete(gameId)
  renderGalleryView()

  const stopped = await invokeOrAlert<boolean>('stop_game', { folderPath: game.folderPath })
  if (stopped === undefined) return // invokeOrAlert already surfaced the error
  log(stopped ? `Stopped "${game.name}"` : `"${game.name}" was already stopped`)
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
    showToast(
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

syncLibraryButton.addEventListener('click', () => handleSyncLibrary())
flashFirmwareButton.addEventListener('click', () => handleFlashFirmware())

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
  showToast(`Bound tag ${bindTagUid}.`)
  log(
    `Bound tag ${bindTagUid} -> ${bindSelect.selectedOptions[0]?.textContent ?? gameId}`,
  )
})

simulateBtn.addEventListener('click', triggerSimulatedTagEvent)
devFlashFirmwareButton.addEventListener('click', () => handleFlashFirmware())
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
menuSettingsBtn.addEventListener('click', async () => {
  hideAppMenu()
  showSettingsSection('general')
  launchOnStartupCheckbox.checked = await isAutostartEnabled().catch(() => false)
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
showStoreBadgesCheckbox.addEventListener('change', () =>
  handleToggleShowStoreBadges(showStoreBadgesCheckbox.checked),
)
syncOnStartupCheckbox.addEventListener('change', () =>
  handleToggleSyncOnStartup(syncOnStartupCheckbox.checked),
)
launchOnStartupCheckbox.addEventListener('change', () =>
  handleToggleLaunchOnStartup(launchOnStartupCheckbox.checked),
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
  showToast(`Reader: ${event.payload}`)
  log(`Reader error: ${event.payload}`)
})
// One flash-progress event per espflash callback (init/update/verifying/
// finish) -- 'writing' fires often (once per chunk written), so only its
// first occurrence gets logged to the output log; the running percentage
// goes to the progress dialog instead, which is meant to update rapidly.
let lastFlashStage: FlashProgressPayload['stage'] | null = null
listen<FlashProgressPayload>('flash-progress', (event) => {
  const progress = event.payload
  if (progress.stage === 'writing') {
    progressDialogBarEl.max = progress.total
    progressDialogBarEl.value = progress.current
    const percent = progress.total > 0 ? Math.round((progress.current / progress.total) * 100) : 0
    progressDialogMessageEl.textContent = `Flashing firmware... ${percent}%`
    if (lastFlashStage !== 'writing') log('Writing firmware to the board...')
  } else if (progress.stage === 'verifying') {
    progressDialogBarEl.removeAttribute('value')
    progressDialogBarEl.removeAttribute('max')
    progressDialogMessageEl.textContent = 'Verifying flash...'
    log('Verifying flash...')
  }
  lastFlashStage = progress.stage
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
  // Chained rather than awaited inline in this handler -- loadReaderState
  // and pollRunningGames below shouldn't wait on a sync that can take a
  // few seconds for a large library; they only need the initial catalog
  // load, not the sync that may follow it.
  loadCatalog().then(() => {
    if (catalog && catalog.settings.syncOnStartup) handleSyncLibrary()
  })
  loadReaderState()
  pollRunningGames()
  setInterval(pollRunningGames, RUNNING_GAMES_POLL_INTERVAL_MS)
})
