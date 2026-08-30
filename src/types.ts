export type Health = "good" | "warning" | "error" | "unknown";

export interface GameInfo {
  detected: boolean;
  path: string | null;
  steamBuildId: string | null;
  installState: string | null;
  engine: string;
  compatDataPath: string | null;
  source: "automatic" | "manual" | "none";
}

export interface Ue4ssInfo {
  installed: boolean;
  healthy: boolean;
  luaMods: number;
  logFound: boolean;
  protonOverride: boolean | null;
  message: string | null;
}

export interface Ue4ssInstallReport {
  installed: number;
  preserved: string[];
  protonHint: boolean;
}

export interface NexusAccount {
  name: string;
  premium: boolean;
}

export interface NexusStatus {
  hasKey: boolean;
  /** Where the key is held. "database" means plain text, and is surfaced to the user. */
  storage: "keyring" | "database" | null;
  handlerRegistered: boolean;
  /** The application currently holding nxm://, when it is not this one. */
  handlerOwner: string | null;
  /** Why registration cannot take effect on this system, if it cannot. */
  handlerProblem: string | null;
}

export interface DownloadProgress {
  name: string;
  done: number;
  total: number | null;
}

export interface Links {
  ue4ssDownload: string;
  nexusGame: string;
  project: string;
}

export interface UpdateInfo {
  currentVersion: string;
  latestVersion: string;
  releaseUrl: string;
  updateAvailable: boolean;
}

export interface Dashboard {
  game: GameInfo;
  installedMods: number;
  enabledMods: number;
  conflictCount: number;
  ue4ss: Ue4ssInfo;
  previousBuildId: string | null;
  dataDirectory: string;
  retoc: ToolInfo;
}

export interface ToolInfo {
  found: boolean;
  path: string | null;
  version: string | null;
}

export interface ModSummary {
  id: string;
  name: string;
  version: string | null;
  modType: "iostore" | "pak" | "ue4ss";
  enabled: boolean;
  installedAt: string;
  installedBuild: string | null;
  packageCount: number;
  conflictCount: number;
  potentialConflictCount: number;
  loadPriority: number | null;
  files: ModFile[];
}

export interface PreviewConflict {
  modId: string;
  name: string;
  packageCount: number;
}

export interface LoadOrderEntry {
  id: string;
  name: string;
  modType: "iostore" | "pak";
  enabled: boolean;
  priority: number | null;
  supported: boolean;
  supportReason: string | null;
  applied: boolean;
  activeConflictCount: number;
  potentialConflictCount: number;
}

export interface ConflictGroup {
  id: string;
  memberIds: string[];
  packageCount: number;
  active: boolean;
  potential: boolean;
  winnerId: string | null;
}

export interface LoadOrderState {
  entries: LoadOrderEntry[];
  activeConflicts: ConflictGroup[];
  potentialConflicts: ConflictGroup[];
  unapplied: boolean;
}

export interface LoadOrderMove {
  modId: string;
  from: string;
  to: string;
}

export interface WinnerChange {
  conflictId: string;
  fromModId: string | null;
  toModId: string | null;
}

export interface LoadOrderPreview {
  orderedModIds: string[];
  moves: LoadOrderMove[];
  activeConflicts: ConflictGroup[];
  potentialConflicts: ConflictGroup[];
  winnerChanges: WinnerChange[];
}

export interface ModFile {
  name: string;
  destination: string;
  size: number;
  sha256: string;
}

export interface ModPreview {
  stagingId: string;
  name: string;
  version: string | null;
  author: string | null;
  description: string | null;
  modType: "iostore" | "pak" | "ue4ss";
  files: string[];
  warnings: string[];
  valid: boolean;
  verification: "passed" | "failed" | "unavailable" | "not-required";
  verificationDetails: string | null;
  packageCount: number;
  packageNames: string[];
  compatibility: Health;
  compatibilityMessage: string;
  testedBuilds: string[];
  conflicts: PreviewConflict[];
  recommendedPriority: number | null;
  loadOrderSupported: boolean;
  loadOrderSupportReason: string | null;
}

export interface DiagnosticItem {
  label: string;
  status: Health;
  value: string;
  action: string | null;
}

export interface DiagnosticReport {
  overall: "GOOD" | "NEEDS ATTENTION" | "BLOCKED";
  items: DiagnosticItem[];
  text: string;
}

export interface AppSettings {
  gamePath: string | null;
  retocPath: string | null;
  logLevel: "normal" | "verbose" | "developer";
  advancedPackageNames: boolean;
  reducedMotion: boolean;
}
