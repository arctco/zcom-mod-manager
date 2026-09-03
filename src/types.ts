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
  modCount: number;
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
  /** Who the stored key belongs to, remembered from when it was verified. */
  accountName: string | null;
  /** Only a premium account can resolve a download link without the website. */
  premium: boolean;
  /** Where the key is held. "database" means plain text, and is surfaced to the user. */
  storage: "keyring" | "database" | null;
  handlerRegistered: boolean;
  /** The application currently holding nxm://, when it is not this one. */
  handlerOwner: string | null;
  /** Why registration cannot take effect on this system, if it cannot. */
  handlerProblem: string | null;
}

export interface ModUpdate {
  /** The installed mod, not the Nexus mod. */
  modId: string;
  name: string;
  installedVersion: string | null;
  installedFileId: number;
  nexusModId: number;
  latestFileId: number;
  latestVersion: string | null;
  latestFileName: string;
  /** The mod's files tab, where a free account has to start the download. */
  pageUrl: string;
  /** The link the website would hand over; a premium key resolves it directly. */
  nxmUrl: string;
  checkedAt: string;
}

export interface ModUpdateReport {
  updates: ModUpdate[];
  /** Installed mods that carry Nexus provenance and can be checked at all. */
  tracked: number;
  checkedAt: string | null;
  /** Mods matched to a Nexus page by their archive during this check. */
  identified: number;
  /** Installed mods that could not be matched, and so are not checked. */
  unmatched: number;
  /** Mods the user has taken out of checking, which are never looked up. */
  ignored: number;
  /** True when nothing was fetched and this is the stored result. */
  fromCache: boolean;
  problem: string | null;
}

export interface DownloadProgress {
  name: string;
  done: number;
  total: number | null;
}

export interface Links {
  ue4ssDownload: string;
  nexusGame: string;
  /** The manager's own Nexus Mods page, where a release lands as well. */
  nexusManager: string;
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
  existingModScanPending: boolean;
}

export interface ToolInfo {
  found: boolean;
  path: string | null;
  version: string | null;
}

/** What a payload is and where it is deployed. */
export type ModType = "iostore" | "pak" | "ue4ss" | "gamedir";
/** A preview may also describe the UE4SS runtime, which is not a mod. */
export type PreviewType = ModType | "ue4ss-runtime";

export interface ModSummary {
  id: string;
  name: string;
  version: string | null;
  modType: ModType;
  enabled: boolean;
  installedAt: string;
  installedBuild: string | null;
  packageCount: number;
  conflictCount: number;
  potentialConflictCount: number;
  loadPriority: number | null;
  /** The Nexus mod this came from, when known. Only these are update-checked. */
  nexusModId: number | null;
  /** That mod's page on Nexus, when it is linked to one. */
  nexusUrl: string | null;
  /** Taken out of update checking by the user, and never looked up again. */
  nexusIgnored: boolean;
  /** Kept out of the library list. Still installed, deployed, and ordered. */
  hidden: boolean;
  files: ModFile[];
}

/** An installed mod a candidate would take the place of. */
export interface ReplacedMod {
  modId: string;
  name: string;
  version: string | null;
  reason: string;
}

export interface PreviewConflict {
  modId: string;
  name: string;
  packageCount: number;
}

export interface LoadOrderEntry {
  id: string;
  name: string;
  modType: "iostore" | "pak" | "ue4ss";
  /** Which UE4SS start pass a mod belongs to; null for packaged mods. */
  runtimeKind: "native" | "script" | "mixed" | null;
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
  /** UE4SS mods in the order the runtime starts them, first to last. */
  ue4ssEntries: LoadOrderEntry[];
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
  /** The archive or folder this candidate was read from. */
  sourcePath: string;
  name: string;
  version: string | null;
  author: string | null;
  description: string | null;
  modType: PreviewType;
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
  replaces: ReplacedMod | null;
  recommendedPriority: number | null;
  loadOrderSupported: boolean;
  loadOrderSupportReason: string | null;
  /** Folder label for one selectable option in a multi-option archive. */
  optionLabel: string | null;
}

/** One option a scripted installer offers inside a group. */
export interface FomodPlugin {
  id: string;
  name: string;
  description: string | null;
  /** A data: URL, since the sandbox is not reachable from the interface. */
  image: string | null;
  kind: "Required" | "Recommended" | "Optional" | "CouldBeUsable" | "NotUsable";
  /** Whether the author's own answer selects this option. */
  selected: boolean;
}

export type FomodGroupKind = "SelectExactlyOne" | "SelectAtMostOne" | "SelectAtLeastOne" | "SelectAny" | "SelectAll";

export interface FomodGroup {
  name: string;
  kind: FomodGroupKind;
  plugins: FomodPlugin[];
}

export interface FomodStep {
  index: number;
  name: string;
  groups: FomodGroup[];
}

/** One answered step, as it is handed back to the backend. */
export interface FomodAnswer {
  step: number;
  plugins: string[];
}

export interface FomodSession {
  sessionId: string;
  moduleName: string;
  moduleImage: string | null;
  author: string | null;
  version: string | null;
  description: string | null;
  /** The question awaiting an answer, or null once there are none left. */
  step: FomodStep | null;
  position: number;
  /** The most this installer can still ask; it falls as steps are skipped. */
  total: number;
  complete: boolean;
  warnings: string[];
}

/**
 * What reading a download produced: either the mods it contains, or the first
 * question its scripted installer asks.
 */
export interface Inspection {
  previews: ModPreview[];
  installer: FomodSession | null;
}

export interface ExistingModCandidate {
  id: string;
  name: string;
  version: string | null;
  modType: ModType;
  files: string[];
  enabled: boolean;
  packageCount: number;
  warnings: string[];
  adoptable: boolean;
  blockedReason: string | null;
  selectedByDefault: boolean;
  likelyRuntimeComponent: boolean;
  inferredPriority: number | null;
}

export interface ExistingModScan {
  scanId: string;
  candidates: ExistingModCandidate[];
  unsupported: string[];
  warnings: string[];
}

export interface AdoptionGroup {
  candidateIds: string[];
  name: string;
}

export interface AdoptionOutcome {
  candidateIds: string[];
  name: string;
  modSummary: ModSummary | null;
  error: string | null;
}

export interface AdoptionReport {
  outcomes: AdoptionOutcome[];
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
  customExecutablePath: string | null;
  retocPath: string | null;
  logLevel: "normal" | "verbose" | "developer";
  advancedPackageNames: boolean;
  reducedMotion: boolean;
  /** Allows one throttled Nexus update check on start-up. Off by default. */
  nexusAutoUpdateCheck: boolean;
}

export interface ManagedLibraryInfo {
  path: string;
  defaultPath: string;
  isDefault: boolean;
}

export interface LaunchReport {
  method: "steam" | "custom-executable";
}
