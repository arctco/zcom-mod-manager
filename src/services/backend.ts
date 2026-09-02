import { invoke } from "@tauri-apps/api/core";
import type { AdoptionGroup, AdoptionReport, AppSettings, Dashboard, DiagnosticReport, ExistingModScan, LaunchReport, Links, LoadOrderPreview, LoadOrderState, ManagedLibraryInfo, ModPreview, ModSummary, ModUpdateReport, NexusAccount, NexusStatus, Ue4ssInstallReport, UpdateInfo } from "../types";

export const backend = {
  dashboard: () => invoke<Dashboard>("get_dashboard"),
  mods: () => invoke<ModSummary[]>("list_mods"),
  loadOrder: () => invoke<LoadOrderState>("get_load_order_state"),
  previewLoadOrder: (orderedModIds: string[]) => invoke<LoadOrderPreview>("preview_load_order", { orderedModIds }),
  applyLoadOrder: (orderedModIds: string[]) => invoke<LoadOrderState>("apply_load_order", { orderedModIds }),
  applyUe4ssOrder: (orderedModIds: string[]) => invoke<LoadOrderState>("apply_ue4ss_order", { orderedModIds }),
  inspect: (path: string) => invoke<ModPreview[]>("inspect_mod", { path }),
  discoverExistingMods: () => invoke<ExistingModScan>("discover_existing_mods"),
  adoptExistingMods: (scanId: string, groups: AdoptionGroup[]) => invoke<AdoptionReport>("adopt_existing_mods", { scanId, groups }),
  acknowledgeExistingModPrompt: () => invoke<void>("acknowledge_existing_mod_prompt"),
  install: (stagingId: string, name?: string, replace?: string) => invoke<ModSummary>("install_mod", { stagingId, name: name ?? null, replace: replace ?? null }),
  discardPreviews: (stagingIds: string[]) => invoke<void>("discard_previews", { stagingIds }),
  rename: (id: string, name: string) => invoke<void>("rename_mod", { id, name }),
  setEnabled: (id: string, enabled: boolean) => invoke<void>("set_mod_enabled", { id, enabled }),
  setHidden: (id: string, hidden: boolean) => invoke<void>("set_mod_hidden", { id, hidden }),
  uninstall: (id: string, force = false) => invoke<void>("uninstall_mod", { id, force }),
  verify: (id: string) => invoke<string>("verify_mod", { id }),
  installUe4ss: (path: string) => invoke<Ue4ssInstallReport>("install_ue4ss", { path }),
  links: () => invoke<Links>("get_links"),
  checkForUpdates: () => invoke<UpdateInfo>("check_for_updates"),
  nexusStatus: () => invoke<NexusStatus>("nexus_status"),
  setNexusKey: (key: string) => invoke<NexusAccount>("set_nexus_key", { key }),
  clearNexusKey: () => invoke<void>("clear_nexus_key"),
  setNxmHandler: (enabled: boolean) => invoke<NexusStatus>("set_nxm_handler", { enabled }),
  nexusDownload: (url: string) => invoke<string>("nexus_download", { url }),
  modUpdates: () => invoke<ModUpdateReport>("mod_updates"),
  checkModUpdates: (force: boolean) => invoke<ModUpdateReport>("check_mod_updates", { force }),
  setNexusAutoCheck: (enabled: boolean) => invoke<void>("set_nexus_auto_check", { enabled }),
  linkModToNexus: (modId: string, reference: string) => invoke<ModUpdateReport>("link_mod_to_nexus", { modId, reference }),
  setModChecked: (modId: string, checked: boolean) => invoke<ModUpdateReport>("set_mod_checked", { modId, checked }),
  takePendingNxm: () => invoke<string | null>("take_pending_nxm"),
  diagnostics: () => invoke<DiagnosticReport>("run_diagnostics"),
  settings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => invoke<void>("save_settings", { settings }),
  setGamePath: (path: string) => invoke<GameInfo>("set_game_path", { path }),
  managedLibrary: () => invoke<ManagedLibraryInfo>("get_managed_library"),
  moveManagedLibrary: (path: string) => invoke<ManagedLibraryInfo>("move_managed_library", { path }),
  copyDiagnostics: () => invoke<string>("diagnostic_report"),
  openManagedPath: (kind: "game" | "mods" | "logs" | "data" | "library" | `mod:${string}` | `installed:${string}`) => invoke<void>("open_managed_path", { kind }),
  launchGame: () => invoke<LaunchReport>("launch_game"),
  reportInterfaceError: (message: string, stack: string | null, context: string) => invoke<void>("report_interface_error", { message, stack, context }),
  reportInterfaceLayout: (context: string) => invoke<void>("report_interface_layout", { context })
};

interface GameInfo {
  detected: boolean;
  path: string | null;
  steamBuildId: string | null;
  installState: string | null;
  engine: string;
  compatDataPath: string | null;
  source: "automatic" | "manual" | "none";
}

export function friendlyError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "The operation could not be completed.";
}
