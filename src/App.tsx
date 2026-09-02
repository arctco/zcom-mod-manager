import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import brandMark from "./assets/icon.svg";
import { Shell, type Page } from "./components/Shell";
import { AdoptionDialog } from "./components/AdoptionDialog";
import { useSubscription } from "./hooks/useSubscription";
import { DiagnosticsPage } from "./pages/DiagnosticsPage";
import { HomePage } from "./pages/HomePage";
import { InstallPage } from "./pages/InstallPage";
import { ModsPage } from "./pages/ModsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { AboutPage } from "./pages/AboutPage";
import { backend, friendlyError } from "./services/backend";
import type { AdoptionGroup, AdoptionReport, AppSettings, Dashboard, DiagnosticReport, DownloadProgress, ExistingModScan, Links, LoadOrderPreview, LoadOrderState, ManagedLibraryInfo, ModPreview, ModSummary, ModUpdate, ModUpdateReport, NexusAccount, NexusStatus, UpdateInfo } from "./types";

const defaultSettings: AppSettings = { gamePath: null, customExecutablePath: null, retocPath: null, logLevel: "normal", advancedPackageNames: false, reducedMotion: false, nexusAutoUpdateCheck: false };
const defaultLinks: Links = { ue4ssDownload: "", nexusGame: "", nexusManager: "", project: "" };
const defaultLoadOrder: LoadOrderState = { entries: [], ue4ssEntries: [], activeConflicts: [], potentialConflicts: [], unapplied: false };

/**
 * WebView2 has occasionally kept a stale CSS viewport height after a mod
 * action. Fixed-position UI still uses the real window in that state, which is
 * why the toast in the report reaches the bottom while the shell stops early.
 * The CSS no longer uses `100vh`; this guard repairs any residual mismatch and
 * leaves measurements in the application log for a Windows follow-up.
 */
function ensureFullViewport(reason: string) {
  const shell = document.querySelector<HTMLElement>(".shell");
  if (!shell) return;
  const bounds = shell.getBoundingClientRect();
  const expected = window.innerHeight;
  if (Math.abs(bounds.height - expected) <= 2 && Math.abs(bounds.top) <= 2) return;
  const main = document.querySelector<HTMLElement>(".main")?.getBoundingClientRect();
  const context = [
    `reason=${reason}`,
    `inner=${window.innerWidth}x${window.innerHeight}`,
    `client=${document.documentElement.clientWidth}x${document.documentElement.clientHeight}`,
    `visual=${window.visualViewport ? `${window.visualViewport.width}x${window.visualViewport.height}` : "unavailable"}`,
    `dpr=${window.devicePixelRatio}`,
    `shell=${bounds.left},${bounds.top},${bounds.width},${bounds.height}`,
    `main=${main ? `${main.left},${main.top},${main.width},${main.height}` : "missing"}`,
  ].join(" ");
  // Fixed positioning uses the geometry that remained correct for the report's
  // toast, making this an immediate recovery as well as a diagnostic.
  shell.style.position = "fixed";
  shell.style.inset = "0";
  try { void backend.reportInterfaceLayout(context).catch(() => {}); }
  catch { /* Browser-only tests do not have the native logging bridge. */ }
}

export default function App() {
  const [page, setPage] = useState<Page>("home");
  const [dashboard, setDashboard] = useState<Dashboard | null>(null);
  const [mods, setMods] = useState<ModSummary[]>([]);
  const [loadOrder, setLoadOrder] = useState<LoadOrderState>(defaultLoadOrder);
  const [orderPreview, setOrderPreview] = useState<LoadOrderPreview | null>(null);
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [managedLibrary, setManagedLibrary] = useState<ManagedLibraryInfo | null>(null);
  const [movingLibrary, setMovingLibrary] = useState(false);
  const [links, setLinks] = useState<Links>(defaultLinks);
  const [nexus, setNexus] = useState<NexusStatus | null>(null);
  const [nexusAccount, setNexusAccount] = useState<NexusAccount | null>(null);
  const [download, setDownload] = useState<DownloadProgress | null>(null);
  const [modUpdates, setModUpdates] = useState<ModUpdateReport | null>(null);
  const [checkingMods, setCheckingMods] = useState(false);
  const [previews, setPreviews] = useState<ModPreview[]>([]);
  // Names the person edited before installing, keyed by staging id. An archive
  // can hold several mods, so each keeps its own draft.
  const [names, setNames] = useState<Record<string, string>>({});
  const [installing, setInstalling] = useState<string | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [busyMod, setBusyMod] = useState<string | null>(null);
  const [orderBusy, setOrderBusy] = useState(false);
  const [launching, setLaunching] = useState(false);
  const [updateChecking, setUpdateChecking] = useState(false);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [advanced, setAdvanced] = useState(false);
  const [existingScan, setExistingScan] = useState<ExistingModScan | null>(null);
  const [existingPrompt, setExistingPrompt] = useState(false);
  const [existingReview, setExistingReview] = useState(false);
  const [discoveringExisting, setDiscoveringExisting] = useState(false);
  const [adoptingExisting, setAdoptingExisting] = useState(false);
  const [toast, setToast] = useState<{ kind: "ok" | "error"; text: string } | null>(null);
  const updateCheckStarted = useRef(false);
  const modUpdateCheckStarted = useRef(false);
  const automaticDiscoveryStarted = useRef(false);
  const existingDiscoveryAttempt = useRef(0);
  // Held in a ref as well as in state so an inspection that finishes while
  // another is starting can still find, and release, what it replaced.
  const previewsRef = useRef<ModPreview[]>([]);
  const inspectAttempt = useRef(0);

  const notify = (text: string, kind: "ok" | "error" = "ok") => { setToast({ text, kind }); window.setTimeout(() => setToast(null), 4500); };
  const refresh = useCallback(async () => {
    try {
      const [nextDashboard, nextMods, nextLoadOrder, nextSettings, nextLinks, nextNexus, nextModUpdates, nextManagedLibrary] = await Promise.all([backend.dashboard(), backend.mods(), backend.loadOrder(), backend.settings(), backend.links(), backend.nexusStatus(), backend.modUpdates(), backend.managedLibrary()]);
      setDashboard(nextDashboard); setMods(nextMods); setLoadOrder(nextLoadOrder); setSettings(nextSettings); setLinks(nextLinks); setNexus(nextNexus); setModUpdates(nextModUpdates); setManagedLibrary(nextManagedLibrary);
      document.documentElement.dataset.reduceMotion = String(nextSettings.reducedMotion);
    } catch (error) { notify(friendlyError(error), "error"); }
  }, []);

  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => {
    // Wait through two paints: a toggle updates its busy state, refreshed mod
    // data, and toast in adjacent React commits.
    let settledFrame = 0;
    const frame = window.requestAnimationFrame(() => {
      settledFrame = window.requestAnimationFrame(() => ensureFullViewport("app-render"));
    });
    return () => {
      window.cancelAnimationFrame(frame);
      if (settledFrame) window.cancelAnimationFrame(settledFrame);
    };
  }, [busyMod, mods, page]);
  useEffect(() => {
    if (!dashboard?.game.detected || !dashboard.existingModScanPending || automaticDiscoveryStarted.current) return;
    automaticDiscoveryStarted.current = true;
    void discoverExisting(false);
  }, [dashboard?.game.detected, dashboard?.existingModScanPending]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (updateCheckStarted.current) return;
    updateCheckStarted.current = true;
    void checkUpdates(false);
  }, []);
  // The opt-in start-up check. The backend keeps its own stored result for a
  // few hours, so reopening the manager does not spend the Nexus allowance,
  // and a quiet failure stays quiet: only the button reports problems.
  useEffect(() => {
    if (!settings.nexusAutoUpdateCheck || !nexus?.hasKey || modUpdateCheckStarted.current) return;
    modUpdateCheckStarted.current = true;
    void checkModUpdates(false);
  }, [settings.nexusAutoUpdateCheck, nexus?.hasKey]); // eslint-disable-line react-hooks/exhaustive-deps
  useSubscription(() => getCurrentWebview().onDragDropEvent(event => {
    if (event.payload.type === "drop" && event.payload.paths[0]) { setPage("install"); void inspect(event.payload.paths[0]); }
  }), []);
  useSubscription(() => listen<string>("zcom://refresh", () => void refresh()), [refresh]);
  useSubscription(() => listen<DownloadProgress>("zcom://download-progress", event => setDownload(event.payload)), []);
  // Kept in a ref so the subscription is created once instead of on every
  // render, which would leave overlapping listeners behind.
  const handoffRef = useRef(handoff);
  handoffRef.current = handoff;
  useSubscription(() => listen<string>("zcom://nxm", event => void handoffRef.current(event.payload)), []);
  // A link that launched the application arrives before that listener exists,
  // so it is collected from the backend instead. `take_pending_nxm` clears it,
  // so a second call returns nothing and the link is handled once.
  useEffect(() => { void backend.takePendingNxm().then(url => { if (url) void handoffRef.current(url); }); }, []);

  async function inspect(path: string) {
    const attempt = ++inspectAttempt.current;
    setLoading(true);
    try {
      const found = await backend.inspect(path);
      // Another inspection started while this one was reading the archive, so
      // this result is stale: release its sandbox rather than leaving it behind.
      if (attempt !== inspectAttempt.current) { await release(found); return; }
      const replaced = previewsRef.current;
      previewsRef.current = found;
      setPreviews(found);
      setNames({});
      await release(replaced);
    }
    catch (e) { if (attempt === inspectAttempt.current) notify(friendlyError(e), "error"); }
    finally { if (attempt === inspectAttempt.current) setLoading(false); }
  }
  /** Releases the sandbox a set of previews was extracted into. */
  async function release(staged: ModPreview[]) {
    if (staged.length === 0) return;
    try { await backend.discardPreviews(staged.map(preview => preview.stagingId)); }
    catch { /* the sandbox is a cache; a failure here is not worth a message */ }
  }
  async function discardPreviews() {
    const staged = previewsRef.current;
    previewsRef.current = [];
    setPreviews([]);
    await release(staged);
  }
  async function choose(options: { directory?: boolean; filters?: { name: string; extensions: string[] }[] } = {}) { const picked = await open({ multiple: false, ...options }); if (typeof picked === "string") await inspect(picked); }
  async function discoverExisting(interactive: boolean) {
    const attempt = ++existingDiscoveryAttempt.current;
    setDiscoveringExisting(true);
    try {
      const scan = await backend.discoverExistingMods();
      if (attempt !== existingDiscoveryAttempt.current) return;
      setExistingScan(scan);
      if (dashboard?.existingModScanPending) await backend.acknowledgeExistingModPrompt();
      if (!interactive) {
        setExistingPrompt(scan.candidates.length > 0 || scan.unsupported.length > 0);
      } else if (scan.candidates.length > 0 || scan.unsupported.length > 0) {
        setExistingReview(true);
      } else {
        notify("No unmanaged supported mods were found.");
      }
    } catch (e) {
      if (attempt === existingDiscoveryAttempt.current) notify(friendlyError(e), "error");
    } finally {
      if (attempt === existingDiscoveryAttempt.current) setDiscoveringExisting(false);
    }
  }
  async function adoptExisting(groups: AdoptionGroup[]): Promise<AdoptionReport> {
    if (!existingScan) return { outcomes: [] };
    setAdoptingExisting(true);
    try {
      const report = await backend.adoptExistingMods(existingScan.scanId, groups);
      const succeeded = report.outcomes.filter(outcome => outcome.modSummary);
      const succeededIds = new Set(succeeded.flatMap(outcome => outcome.candidateIds));
      setExistingScan(current => current ? { ...current, candidates: current.candidates.filter(candidate => !succeededIds.has(candidate.id)) } : null);
      await refresh();
      if (succeeded.length) notify(`${succeeded.length} existing mod${succeeded.length === 1 ? "" : "s"} adopted safely.`);
      const failures = report.outcomes.length - succeeded.length;
      if (failures) notify(`${failures} mod${failures === 1 ? "" : "s"} could not be adopted. Review the details and retry.`, "error");
      return report;
    } catch (e) {
      notify(friendlyError(e), "error");
      return { outcomes: groups.map(group => ({ candidateIds: group.candidateIds, name: group.name, modSummary: null, error: friendlyError(e) })) };
    } finally {
      setAdoptingExisting(false);
    }
  }
  async function locateGame() { const picked = await open({ directory: true, multiple: false, title: "Locate Star Wars Zero Company" }); if (typeof picked === "string") { try { await backend.setGamePath(picked); await refresh(); notify("Game installation connected."); } catch (e) { notify(friendlyError(e), "error"); } } }
  async function install(preview: ModPreview) {
    setInstalling(preview.stagingId);
    try {
      const mod = await backend.install(preview.stagingId, names[preview.stagingId], preview.replaces?.modId);
      notify(preview.replaces ? `${mod.name} replaced ${preview.replaces.name}.` : `${mod.name} installed safely.`);
      const remaining = previewsRef.current.filter(item => item.stagingId !== preview.stagingId);
      previewsRef.current = remaining;
      setPreviews(remaining);
      await refresh();
      if (remaining.length === 0) setPage("mods");
    } catch (e) { notify(friendlyError(e), "error"); } finally { setInstalling(null); }
  }
  async function installAll(selected: ModPreview[]) {
    setInstalling("all");
    let completed = 0;
    const components = [...selected].sort((left, right) => {
      const rank = (preview: ModPreview) => preview.modType === "ue4ss" ? 1 : 0;
      return rank(left) - rank(right);
    });
    try {
      for (const preview of components) {
        await backend.install(preview.stagingId, names[preview.stagingId], preview.replaces?.modId);
        completed += 1;
        const remaining = previewsRef.current.filter(item => item.stagingId !== preview.stagingId);
        previewsRef.current = remaining;
        setPreviews(remaining);
      }
      await refresh();
      notify(components.some(preview => preview.replaces)
        ? `Updated all ${completed} components safely.`
        : `Installed all ${completed} components safely.`);
      setPage("mods");
    } catch (e) {
      await refresh();
      const prefix = completed ? `${completed} component${completed === 1 ? "" : "s"} completed; ` : "";
      notify(`${prefix}${friendlyError(e)}`, "error");
    } finally {
      setInstalling(null);
    }
  }
  async function installRuntimeFrom(preview: ModPreview) {
    setInstalling(preview.stagingId);
    try { await applyUe4ssPackage(preview.sourcePath); await discardPreviews(); }
    finally { setInstalling(null); }
  }
  async function rename(mod: ModSummary) {
    const next = window.prompt(`Rename ${mod.name} to:`, mod.name);
    if (next === null || next.trim() === "" || next.trim() === mod.name) return;
    setBusyMod(mod.id);
    try { await backend.rename(mod.id, next.trim()); await refresh(); notify(`Renamed to ${next.trim()}.`); }
    catch (e) { notify(friendlyError(e), "error"); } finally { setBusyMod(null); }
  }
  async function toggle(mod: ModSummary) { setBusyMod(mod.id); try { await backend.setEnabled(mod.id, !mod.enabled); await refresh(); notify(`${mod.name} ${mod.enabled ? "disabled" : "enabled"}.`); } catch (e) { notify(friendlyError(e), "error"); } finally { setBusyMod(null); } }
  async function uninstall(mod: ModSummary) { if (!window.confirm(`Uninstall ${mod.name}? Its managed library copy and unchanged deployed files will be removed.`)) return; setBusyMod(mod.id); try { await backend.uninstall(mod.id); await refresh(); notify(`${mod.name} uninstalled.`); } catch (e) { notify(`${friendlyError(e)} The changed file was kept.`, "error"); } finally { setBusyMod(null); } }
  async function verify(mod: ModSummary) { setBusyMod(mod.id); try { notify(await backend.verify(mod.id)); } catch (e) { notify(friendlyError(e), "error"); } finally { setBusyMod(null); } }
  async function previewOrder(ids: string[]) { setOrderBusy(true); try { setOrderPreview(await backend.previewLoadOrder(ids)); } catch (e) { notify(friendlyError(e), "error"); } finally { setOrderBusy(false); } }
  async function applyOrder(ids: string[]) { setOrderBusy(true); try { setLoadOrder(await backend.applyLoadOrder(ids)); setOrderPreview(null); await refresh(); notify("Load order applied safely."); } catch (e) { notify(friendlyError(e), "error"); } finally { setOrderBusy(false); } }
  async function applyUe4ssOrder(ids: string[]) { setOrderBusy(true); try { setLoadOrder(await backend.applyUe4ssOrder(ids)); await refresh(); notify("UE4SS start order written to mods.txt."); } catch (e) { notify(friendlyError(e), "error"); } finally { setOrderBusy(false); } }
  async function installUe4ss() {
    const picked = await open({ multiple: false, title: "Select the downloaded UE4SS package", filters: [{ name: "UE4SS package", extensions: ["zip", "7z"] }] });
    if (typeof picked !== "string") return;
    await applyUe4ssPackage(picked);
  }
  async function applyUe4ssPackage(picked: string) {
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
    // The transfer screen goes up before the first request, so pressing Mod
    // Manager Download never looks like nothing happened.
    setPage("install"); await discardPreviews(); setLoading(true);
    setDownload({ name: "", done: 0, total: null });
    try {
      const path = await backend.nexusDownload(url);
      setDownload(null);
      setNames({});
      const found = await backend.inspect(path);
      previewsRef.current = found;
      setPreviews(found);
    } catch (e) { notify(friendlyError(e), "error"); setDownload(null); }
    finally { setLoading(false); }
  }
  // Asks Nexus which files the tracked mods now offer. `force` is the button;
  // without it the backend may answer from its stored result.
  async function checkModUpdates(force: boolean) {
    if (force) setCheckingMods(true);
    try {
      const report = await backend.checkModUpdates(force);
      setModUpdates(report);
      if (!force) return;
      // Identification runs first, so a mod matched from its archive is
      // reported here even though nothing was downloaded to make it happen.
      const matched = report.identified > 0 ? ` ${report.identified} mod${report.identified === 1 ? " was" : "s were"} matched to a Nexus page.` : "";
      const unmatched = report.unmatched > 0 ? ` ${report.unmatched} could not be matched; link them from More details, or stop checking them there.` : "";
      if (report.problem) { notify(`${report.problem}${matched}`, "error"); return; }
      if (report.tracked === 0) { notify(`No installed mod could be matched to a Nexus mod, so there is nothing to check.${unmatched}`); return; }
      notify((report.updates.length === 0
        ? `No updates. ${report.tracked} mod${report.tracked === 1 ? "" : "s"} checked.`
        : `${report.updates.length} update${report.updates.length === 1 ? " is" : "s are"} available.`) + matched + unmatched);
    } catch (e) { if (force) notify(friendlyError(e), "error"); }
    finally { if (force) setCheckingMods(false); }
  }
  // A free account cannot get a download link without the key the website
  // mints, so the update starts where it has to: on the mod's files tab. A
  // premium key resolves the same link here, and the download then runs
  // through the identical inspect-and-replace path as a website handoff.
  async function updateMod(update: ModUpdate) {
    if (nexus?.premium) { await handoff(update.nxmUrl); return; }
    openExternal(update.pageUrl);
  }
  // Points a mod at a Nexus page by hand, for one whose archive is gone or was
  // never a Nexus download.
  async function linkMod(mod: ModSummary, reference: string) {
    try {
      setModUpdates(await backend.linkModToNexus(mod.id, reference));
      await refresh();
      notify(`${mod.name} is now checked for updates.`);
    } catch (e) { notify(friendlyError(e), "error"); }
  }
  // Covers both a wrongly linked mod and one that never came from Nexus: while
  // checking is off it is neither checked nor offered to the archive lookup.
  async function setModChecked(mod: ModSummary, checked: boolean) {
    try {
      setModUpdates(await backend.setModChecked(mod.id, checked));
      await refresh();
      notify(checked ? `${mod.name} is checked for updates again.` : `${mod.name} is no longer checked for updates.`);
    } catch (e) { notify(friendlyError(e), "error"); }
  }
  // Hiding is a view decision: the mod stays installed, deployed, and ordered.
  async function setHidden(mod: ModSummary, hidden: boolean) {
    try {
      await backend.setHidden(mod.id, hidden);
      await refresh();
      notify(hidden ? `${mod.name} is hidden from the library list.` : `${mod.name} is shown again.`);
    } catch (e) { notify(friendlyError(e), "error"); }
  }
  // Saved on the spot, like everything else in that panel. Going through the
  // Settings form would have left it to a Save press and let any refresh in
  // between discard it.
  async function setAutoUpdateCheck(enabled: boolean) {
    try {
      await backend.setNexusAutoCheck(enabled);
      setSettings(current => ({ ...current, nexusAutoUpdateCheck: enabled }));
      notify(enabled ? "Installed mods will be checked for updates at start-up." : "Start-up update checks are off.");
    } catch (e) { notify(friendlyError(e), "error"); }
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
  async function moveLibrary(useDefault = false) {
    if (!managedLibrary) return;
    let destination = managedLibrary.defaultPath;
    if (!useDefault) {
      const picked = await open({ directory: true, multiple: false, title: "Select an empty managed mod library folder" });
      if (typeof picked !== "string") return;
      destination = picked;
    }
    if (destination === managedLibrary.path) return;
    if (!window.confirm("Move the managed mod library to this folder? Installed game files are not moved.")) return;
    setMovingLibrary(true);
    try {
      setManagedLibrary(await backend.moveManagedLibrary(destination));
      notify("Managed mod library moved and verified.");
    } catch (e) { notify(friendlyError(e), "error"); }
    finally { setMovingLibrary(false); }
  }
  async function openFolder(kind: Parameters<typeof backend.openManagedPath>[0]) {
    try { await backend.openManagedPath(kind); }
    catch (e) { notify(friendlyError(e), "error"); }
  }
  async function launchGame() {
    setLaunching(true);
    try {
      const result = await backend.launchGame();
      notify(result.method === "custom-executable" ? "Launching the custom game executable." : "Opening Zero Company in Steam.");
    }
    catch (e) { notify(friendlyError(e), "error"); }
    finally { setLaunching(false); }
  }
  async function checkUpdates(announce: boolean) {
    setUpdateChecking(true); setUpdateError(null);
    try {
      const result = await backend.checkForUpdates();
      setUpdate(result);
      if (announce) notify(result.updateAvailable ? `Version ${result.latestVersion} is available.` : "ZCOM Mod Manager is up to date.");
    } catch (e) {
      const message = friendlyError(e);
      setUpdateError(message);
      if (announce) notify(`Couldn’t check GitHub: ${message}`, "error");
    } finally { setUpdateChecking(false); }
  }

  if (!dashboard) return <div className="splash"><img className="brand-mark" src={brandMark} alt="" width={96} height={96} /><p>Preparing your mod library…</p></div>;
  return <Shell page={page} onPage={setPage} gameReady={dashboard.game.detected} updateAvailable={update?.updateAvailable === true}>
    {page === "home" && <HomePage data={dashboard} onInstall={() => setPage("install")} onDiagnose={() => setPage("diagnostics")} onLocate={locateGame} onOpenMods={() => void openFolder("mods")} onOpenGame={() => void openFolder("game")} onLaunchGame={() => void launchGame()} onGetUe4ss={() => openExternal(links.ue4ssDownload)} onInstallUe4ss={() => void installUe4ss()} busy={loading} launching={launching} canLaunch={dashboard.game.detected || !!settings.customExecutablePath} existingModsFound={existingPrompt ? (existingScan?.candidates.length ?? 0) + (existingScan?.unsupported.length ?? 0) : 0} onDismissExisting={() => setExistingPrompt(false)} onReviewExisting={() => { setExistingPrompt(false); setPage("mods"); setExistingReview(true); }} />}
    {page === "mods" && <ModsPage mods={mods} loadOrder={loadOrder} orderPreview={orderPreview} orderBusy={orderBusy} onPreviewOrder={ids => void previewOrder(ids)} onApplyOrder={ids => void applyOrder(ids)} onApplyUe4ssOrder={ids => void applyUe4ssOrder(ids)} onCancelOrder={() => setOrderPreview(null)} onBrowseNexus={() => openExternal(links.nexusGame)} busy={busyMod} onInstall={() => setPage("install")} onDiscover={() => void discoverExisting(true)} discovering={discoveringExisting} onToggle={toggle} onUninstall={uninstall} onVerify={verify} onRename={rename} onOpenInstalled={mod => void openFolder(`installed:${mod.id}`)} onOpenSource={mod => void openFolder(`mod:${mod.id}`)} updates={modUpdates} checkingUpdates={checkingMods} canCheckUpdates={nexus?.hasKey ?? false} directDownload={nexus?.premium ?? false} onCheckUpdates={() => void checkModUpdates(true)} onUpdateMod={update => void updateMod(update)} onLinkMod={(mod, reference) => void linkMod(mod, reference)} onSetModChecked={(mod, checked) => void setModChecked(mod, checked)} onOpenModPage={mod => { if (mod.nexusUrl) openExternal(mod.nexusUrl); }} onSetHidden={(mod, hidden) => void setHidden(mod, hidden)} />}
    {page === "install" && <InstallPage previews={previews} names={names} loading={loading} download={download} advanced={advanced} installing={installing} onAdvanced={() => setAdvanced(!advanced)} onName={(stagingId, name) => setNames(current => ({ ...current, [stagingId]: name }))} onChooseFile={() => void choose({ filters: [{ name: "Supported mods", extensions: ["zip", "7z", "rar", "pak", "utoc", "ucas"] }] })} onChooseFolder={() => void choose({ directory: true })} onInstall={mod => void install(mod)} onInstallAll={mods => void installAll(mods)} onInstallRuntime={mod => void installRuntimeFrom(mod)} onCancel={() => void discardPreviews()} />}
    {page === "diagnostics" && <DiagnosticsPage report={diagnostics} loading={loading} onRun={() => void runDiagnostics()} onCopy={() => void navigator.clipboard.writeText(diagnostics?.text ?? "").then(() => notify("Diagnostic report copied."))} />}
    {page === "settings" && <SettingsPage settings={settings} retoc={dashboard.retoc} managedLibrary={managedLibrary} movingLibrary={movingLibrary} onChange={setSettings} onSave={() => void saveSettings()} onPickGame={() => void locateGame()} onPickExecutable={async () => { const picked = await open({ multiple: false, title: "Select game executable or launcher" }); if (typeof picked === "string") setSettings({ ...settings, customExecutablePath: picked }); }} onPickRetoc={async () => { const picked = await open({ multiple: false, title: "Select retoc executable" }); if (typeof picked === "string") setSettings({ ...settings, retocPath: picked }); }} onMoveLibrary={() => void moveLibrary()} onUseDefaultLibrary={() => void moveLibrary(true)} onOpenLibrary={() => void openFolder("library")} onOpenLogs={() => void openFolder("logs")} onOpenData={() => void openFolder("data")} links={links} onOpenLink={openExternal} nexus={nexus} nexusAccount={nexusAccount} onSaveNexusKey={saveNexusKey} onClearNexusKey={clearNexusKey} onToggleNxmHandler={toggleNxmHandler} onSetAutoUpdateCheck={setAutoUpdateCheck} />}
    {page === "about" && <AboutPage projectUrl={links.project} nexusUrl={links.nexusManager} onOpenLink={openExternal} update={update} checking={updateChecking} error={updateError} onCheckUpdates={() => void checkUpdates(true)} />}
    {discoveringExisting && <div className="scan-indicator" role="status"><span className="spin" />Scanning game folders for existing mods…</div>}
    {existingReview && existingScan && <AdoptionDialog scan={existingScan} busy={adoptingExisting} onClose={() => setExistingReview(false)} onAdopt={adoptExisting} />}
    {toast && <div className={`toast ${toast.kind}`} role="status">{toast.text}</div>}
  </Shell>;
}
