export interface Settings {
  rootFolders: string[];
  confirmBeforeLaunch: boolean;
  showOutputLog: boolean;
  closeBehavior: "ask" | "minimize" | "quit";
  showStoreBadges: boolean;
  syncOnStartup: boolean;
}

export type Store = "steam";

export interface Game {
  id: string;
  name: string;
  folderPath: string;
  exePath: string;
  artworkPath: string | null;
  available: boolean;
  hasCustomArtwork: boolean;
  store: Store | null;
}

export interface Binding {
  tagUid: string;
  gameId: string;
}

export interface Catalog {
  version: number;
  settings: Settings;
  games: Game[];
  bindings: Binding[];
}

export interface ScanCandidate {
  folderPath: string;
  name: string;
  exePath: string | null;
}

export interface SyncResult {
  catalog: Catalog;
  added: number;
  skippedNames: string[];
}
