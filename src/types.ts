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

export interface Links {
  ue4ssDownload: string;
  nexusGame: string;
  project: string;
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
  files: ModFile[];
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
