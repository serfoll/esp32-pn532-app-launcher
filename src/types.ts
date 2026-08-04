export interface Settings {
  rootFolders: string[];
  confirmBeforeLaunch: boolean;
}

export interface Game {
  id: string;
  name: string;
  folderPath: string;
  exePath: string;
  artworkPath: string | null;
  available: boolean;
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
