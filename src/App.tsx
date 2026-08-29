import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath, openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import helmet from "./assets/helmet.png";
import { Shell, type Page } from "./components/Shell";
import { DiagnosticsPage } from "./pages/DiagnosticsPage";
import { HomePage } from "./pages/HomePage";
import { InstallPage } from "./pages/InstallPage";
import { ModsPage } from "./pages/ModsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { backend, friendlyError } from "./services/backend";
import type { AppSettings, Dashboard, DiagnosticReport, DownloadProgress, Links, ModPreview, ModSummary, NexusAccount, NexusStatus } from "./types";

const defaultSettings: AppSettings = { gamePath: null, retocPath: null, logLevel: "normal", advancedPackageNames: false, reducedMotion: false };
const defaultLinks: Links = { ue4ssDownload: "", nexusGame: "", project: "" };

export default function App() {
  const [page, setPage] = useState<Page>("home");
  const [dashboard, setDashboard] = useState<Dashboard | null>(null);
  const [mods, setMods] = useState<ModSummary[]>([]);
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [links, setLinks] = useState<Links>(defaultLinks);
  const [nexus, setNexus] = useState<NexusStatus | null>(null);
  const [nexusAccount, setNexusAccount] = useState<NexusAccount | null>(null);
  const [download, setDownload] = useState<DownloadProgress | null>(null);
  const [preview, setPreview] = useState<ModPreview | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [busyMod, setBusyMod] = useState<string | null>(null);
  const [advanced, setAdvanced] = useState(false);
  const [toast, setToast] = useState<{ kind: "ok" | "error"; text: string } | null>(null);

  const notify = (text: string, kind: "ok" | "error" = "ok") => { setToast({ text, kind }); window.setTimeout(() => setToast(null), 4500); };
  const refresh = useCallback(async () => {
    try {
      const [nextDashboard, nextMods, nextSettings, nextLinks, nextNexus] = await Promise.all([backend.dashboard(), backend.mods(), backend.settings(), backend.links(), backend.nexusStatus()]);
      setDashboard(nextDashboard); setMods(nextMods); setSettings(nextSettings); setLinks(nextLinks); setNexus(nextNexus);
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
  useEffect(() => {
    let stop: (() => void) | undefined;
    void listen<DownloadProgress>("zcom://download-progress", event => setDownload(event.payload)).then(fn => { stop = fn; });
    return () => stop?.();
  }, []);
  // Kept in a ref so the subscription is created once instead of on every
  // render, which would leave overlapping listeners behind.
  const handoffRef = useRef(handoff);
  handoffRef.current = handoff;
  useEffect(() => {
    let stop: (() => void) | undefined;
    void listen<string>("zcom://nxm", event => void handoffRef.current(event.payload)).then(fn => { stop = fn; });
    // A link that launched the application arrives before this listener exists,
    // so it is collected from the backend instead.
    void backend.takePendingNxm().then(url => { if (url) void handoffRef.current(url); });
    return () => stop?.();
  }, []);

  async function inspect(path: string) { setLoading(true); setPreview(null); try { setPreview(await backend.inspect(path)); } catch (e) { notify(friendlyError(e), "error"); } finally { setLoading(false); } }
  async function choose(options: { directory?: boolean; filters?: { name: string; extensions: string[] }[] } = {}) { const picked = await open({ multiple: false, ...options }); if (typeof picked === "string") await inspect(picked); }
  async function locateGame() { const picked = await open({ directory: true, multiple: false, title: "Locate Star Wars Zero Company" }); if (typeof picked === "string") { try { await backend.setGamePath(picked); await refresh(); notify("Game installation connected."); } catch (e) { notify(friendlyError(e), "error"); } } }
  async function install() { if (!preview) return; setLoading(true); try { const mod = await backend.install(preview.stagingId); notify(`${mod.name} installed safely.`); setPreview(null); await refresh(); setPage("mods"); } catch (e) { notify(friendlyError(e), "error"); } finally { setLoading(false); } }
  async function toggle(mod: ModSummary) { setBusyMod(mod.id); try { await backend.setEnabled(mod.id, !mod.enabled); await refresh(); notify(`${mod.name} ${mod.enabled ? "disabled" : "enabled"}.`); } catch (e) { notify(friendlyError(e), "error"); } finally { setBusyMod(null); } }
  async function uninstall(mod: ModSummary) { if (!window.confirm(`Uninstall ${mod.name}? Its managed library copy and unchanged deployed files will be removed.`)) return; setBusyMod(mod.id); try { await backend.uninstall(mod.id); await refresh(); notify(`${mod.name} uninstalled.`); } catch (e) { notify(`${friendlyError(e)} The changed file was kept.`, "error"); } finally { setBusyMod(null); } }
  async function verify(mod: ModSummary) { setBusyMod(mod.id); try { notify(await backend.verify(mod.id)); } catch (e) { notify(friendlyError(e), "error"); } finally { setBusyMod(null); } }
  async function installUe4ss() {
    const picked = await open({ multiple: false, title: "Select the downloaded UE4SS package", filters: [{ name: "UE4SS package", extensions: ["zip", "7z"] }] });
    if (typeof picked !== "string") return;
    setLoading(true);
    try {
      const report = await backend.installUe4ss(picked);
      const kept = report.preserved.length ? ` ${report.preserved.length} existing file${report.preserved.length === 1 ? "" : "s"} kept (${report.preserved.join(", ")}).` : "";
      const proton = report.protonHint ? ' Add WINEDLLOVERRIDES="dwmapi=n,b" %command% to the game\u2019s Steam launch options.' : "";
      await refresh();
      notify(`UE4SS runtime installed: ${report.installed} file${report.installed === 1 ? "" : "s"}.${kept}${proton}`);
    } catch (e) { notify(friendlyError(e), "error"); } finally { setLoading(false); }
  }
  // Handles a link the browser passed over from the Mod Manager Download button.
  async function handoff(url: string) {
    setPage("install"); setPreview(null); setLoading(true); setDownload(null);
    try {
      const path = await backend.nexusDownload(url);
      setDownload(null);
      setPreview(await backend.inspect(path));
    } catch (e) { notify(friendlyError(e), "error"); setDownload(null); }
    finally { setLoading(false); }
  }
  async function saveNexusKey(key: string) {
    try {
      const account = await backend.setNexusKey(key);
      setNexusAccount(account);
      setNexus(await backend.nexusStatus());
      notify(`Nexus Mods connected as ${account.name}.`);
    } catch (e) { notify(friendlyError(e), "error"); }
  }
  async function clearNexusKey() {
    try { await backend.clearNexusKey(); setNexusAccount(null); setNexus(await backend.nexusStatus()); notify("Nexus Mods key removed."); }
    catch (e) { notify(friendlyError(e), "error"); }
  }
  async function toggleNxmHandler(enabled: boolean) {
    try {
      const next = await backend.setNxmHandler(enabled);
      setNexus(next);
      if (next.handlerRegistered) { notify("This application now handles nxm:// links."); return; }
      if (!enabled) { notify("nxm:// links are no longer handled here."); return; }
      // Registration was written but the system still resolves elsewhere, so
      // say what actually holds the protocol rather than claiming success.
      notify(next.handlerProblem ?? (next.handlerOwner
        ? `nxm:// links still open in ${next.handlerOwner}.`
        : "The nxm:// association did not take effect."), "error");
    } catch (e) { notify(friendlyError(e), "error"); }
  }
  function openExternal(url: string) { if (url) void openUrl(url).catch(e => notify(friendlyError(e), "error")); }

  async function runDiagnostics() { setLoading(true); try { setDiagnostics(await backend.diagnostics()); } catch (e) { notify(friendlyError(e), "error"); } finally { setLoading(false); } }
  async function saveSettings() { try { await backend.saveSettings(settings); await refresh(); notify("Settings saved."); } catch (e) { notify(friendlyError(e), "error"); } }

  if (!dashboard) return <div className="splash"><img className="brand-mark" src={helmet} alt="" width={96} height={96} /><p>Preparing your mod library…</p></div>;
  return <Shell page={page} onPage={setPage} gameReady={dashboard.game.detected}>
    {page === "home" && <HomePage data={dashboard} onInstall={() => setPage("install")} onDiagnose={() => setPage("diagnostics")} onLocate={locateGame} openMods={async () => openPath(await backend.managedPath("mods"))} onGetUe4ss={() => openExternal(links.ue4ssDownload)} onInstallUe4ss={() => void installUe4ss()} busy={loading} />}
    {page === "mods" && <ModsPage mods={mods} onBrowseNexus={() => openExternal(links.nexusGame)} busy={busyMod} onInstall={() => setPage("install")} onToggle={toggle} onUninstall={uninstall} onVerify={verify} onOpenInstalled={mod => { const first = mod.files[0]?.destination; if (first) void revealItemInDir(first); }} onOpenSource={mod => void backend.managedPath(`mod:${mod.id}`).then(openPath)} />}
    {page === "install" && <InstallPage preview={preview} loading={loading} download={download} advanced={advanced} onAdvanced={() => setAdvanced(!advanced)} onChooseFile={() => void choose({ filters: [{ name: "Supported mods", extensions: ["zip", "7z", "pak", "utoc", "ucas"] }] })} onChooseFolder={() => void choose({ directory: true })} onInstall={() => void install()} onCancel={() => setPreview(null)} />}
    {page === "diagnostics" && <DiagnosticsPage report={diagnostics} loading={loading} onRun={() => void runDiagnostics()} onCopy={() => void navigator.clipboard.writeText(diagnostics?.text ?? "").then(() => notify("Diagnostic report copied."))} />}
    {page === "settings" && <SettingsPage settings={settings} retoc={dashboard.retoc} onChange={setSettings} onSave={() => void saveSettings()} onPickGame={() => void locateGame()} onPickRetoc={async () => { const picked = await open({ multiple: false, title: "Select retoc executable" }); if (typeof picked === "string") setSettings({ ...settings, retocPath: picked }); }} onOpenLogs={() => void backend.managedPath("logs").then(openPath)} onOpenData={() => void backend.managedPath("data").then(openPath)} links={links} onOpenLink={openExternal} nexus={nexus} nexusAccount={nexusAccount} onSaveNexusKey={saveNexusKey} onClearNexusKey={clearNexusKey} onToggleNxmHandler={toggleNxmHandler} />}
    {toast && <div className={`toast ${toast.kind}`} role="status">{toast.text}</div>}
  </Shell>;
}
