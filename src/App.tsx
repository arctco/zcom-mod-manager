import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { Shell, type Page } from "./components/Shell";
import { DiagnosticsPage } from "./pages/DiagnosticsPage";
import { HomePage } from "./pages/HomePage";
import { InstallPage } from "./pages/InstallPage";
import { ModsPage } from "./pages/ModsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { backend, friendlyError } from "./services/backend";
import type { AppSettings, Dashboard, DiagnosticReport, ModPreview, ModSummary } from "./types";

const defaultSettings: AppSettings = { gamePath: null, retocPath: null, logLevel: "normal", advancedPackageNames: false, reducedMotion: false };

export default function App() {
  const [page, setPage] = useState<Page>("home");
  const [dashboard, setDashboard] = useState<Dashboard | null>(null);
  const [mods, setMods] = useState<ModSummary[]>([]);
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [preview, setPreview] = useState<ModPreview | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [busyMod, setBusyMod] = useState<string | null>(null);
  const [advanced, setAdvanced] = useState(false);
  const [toast, setToast] = useState<{ kind: "ok" | "error"; text: string } | null>(null);

  const notify = (text: string, kind: "ok" | "error" = "ok") => { setToast({ text, kind }); window.setTimeout(() => setToast(null), 4500); };
  const refresh = useCallback(async () => {
    try {
      const [nextDashboard, nextMods, nextSettings] = await Promise.all([backend.dashboard(), backend.mods(), backend.settings()]);
      setDashboard(nextDashboard); setMods(nextMods); setSettings(nextSettings);
      document.documentElement.dataset.reduceMotion = String(nextSettings.reducedMotion);
    } catch (error) { notify(friendlyError(error), "error"); }
  }, []);

  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => {
    let stop: (() => void) | undefined;
    void getCurrentWebview().onDragDropEvent(event => {
      if (event.payload.type === "drop" && event.payload.paths[0]) { setPage("install"); void inspect(event.payload.paths[0]); }
    }).then(unlisten => { stop = unlisten; });
    return () => stop?.();
  }, []);
  useEffect(() => { let stop: (() => void) | undefined; void listen<string>("zcom://refresh", () => void refresh()).then(fn => { stop = fn; }); return () => stop?.(); }, [refresh]);

  async function inspect(path: string) { setLoading(true); setPreview(null); try { setPreview(await backend.inspect(path)); } catch (e) { notify(friendlyError(e), "error"); } finally { setLoading(false); } }
  async function choose(options: { directory?: boolean; filters?: { name: string; extensions: string[] }[] } = {}) { const picked = await open({ multiple: false, ...options }); if (typeof picked === "string") await inspect(picked); }
  async function locateGame() { const picked = await open({ directory: true, multiple: false, title: "Locate Star Wars Zero Company" }); if (typeof picked === "string") { try { await backend.setGamePath(picked); await refresh(); notify("Game installation connected."); } catch (e) { notify(friendlyError(e), "error"); } } }
  async function install() { if (!preview) return; setLoading(true); try { const mod = await backend.install(preview.stagingId); notify(`${mod.name} installed safely.`); setPreview(null); await refresh(); setPage("mods"); } catch (e) { notify(friendlyError(e), "error"); } finally { setLoading(false); } }
  async function toggle(mod: ModSummary) { setBusyMod(mod.id); try { await backend.setEnabled(mod.id, !mod.enabled); await refresh(); notify(`${mod.name} ${mod.enabled ? "disabled" : "enabled"}.`); } catch (e) { notify(friendlyError(e), "error"); } finally { setBusyMod(null); } }
  async function uninstall(mod: ModSummary) { if (!window.confirm(`Uninstall ${mod.name}? Its managed library copy and unchanged deployed files will be removed.`)) return; setBusyMod(mod.id); try { await backend.uninstall(mod.id); await refresh(); notify(`${mod.name} uninstalled.`); } catch (e) { notify(`${friendlyError(e)} The changed file was kept.`, "error"); } finally { setBusyMod(null); } }
  async function verify(mod: ModSummary) { setBusyMod(mod.id); try { notify(await backend.verify(mod.id)); } catch (e) { notify(friendlyError(e), "error"); } finally { setBusyMod(null); } }
  async function runDiagnostics() { setLoading(true); try { setDiagnostics(await backend.diagnostics()); } catch (e) { notify(friendlyError(e), "error"); } finally { setLoading(false); } }
  async function saveSettings() { try { await backend.saveSettings(settings); await refresh(); notify("Settings saved."); } catch (e) { notify(friendlyError(e), "error"); } }

  if (!dashboard) return <div className="splash"><span className="brand-mark">ZC</span><p>Preparing your mod library…</p></div>;
  return <Shell page={page} onPage={setPage} gameReady={dashboard.game.detected}>
    {page === "home" && <HomePage data={dashboard} onInstall={() => setPage("install")} onDiagnose={() => setPage("diagnostics")} onLocate={locateGame} openMods={async () => openPath(await backend.managedPath("mods"))} />}
    {page === "mods" && <ModsPage mods={mods} busy={busyMod} onInstall={() => setPage("install")} onToggle={toggle} onUninstall={uninstall} onVerify={verify} onOpenInstalled={mod => { const first = mod.files[0]?.destination; if (first) void revealItemInDir(first); }} onOpenSource={mod => void backend.managedPath(`mod:${mod.id}`).then(openPath)} />}
    {page === "install" && <InstallPage preview={preview} loading={loading} advanced={advanced} onAdvanced={() => setAdvanced(!advanced)} onChooseFile={() => void choose({ filters: [{ name: "Supported mods", extensions: ["zip", "7z", "pak", "utoc", "ucas"] }] })} onChooseFolder={() => void choose({ directory: true })} onInstall={() => void install()} onCancel={() => setPreview(null)} />}
    {page === "diagnostics" && <DiagnosticsPage report={diagnostics} loading={loading} onRun={() => void runDiagnostics()} onCopy={() => void navigator.clipboard.writeText(diagnostics?.text ?? "").then(() => notify("Diagnostic report copied."))} />}
    {page === "settings" && <SettingsPage settings={settings} retoc={dashboard.retoc} onChange={setSettings} onSave={() => void saveSettings()} onPickGame={() => void locateGame()} onPickRetoc={async () => { const picked = await open({ multiple: false, title: "Select retoc executable" }); if (typeof picked === "string") setSettings({ ...settings, retocPath: picked }); }} onOpenLogs={() => void backend.managedPath("logs").then(openPath)} onOpenData={() => void backend.managedPath("data").then(openPath)} />}
    {toast && <div className={`toast ${toast.kind}`} role="status">{toast.text}</div>}
  </Shell>;
}
