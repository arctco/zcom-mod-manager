import { Activity, Download, ExternalLink, FileArchive, FolderOpen, PackageCheck, Play, Puzzle, ShieldCheck } from "lucide-react";
import type { Dashboard } from "../types";
import { StatusBadge } from "../components/StatusBadge";

interface Props {
  data: Dashboard;
  onInstall: () => void;
  onDiagnose: () => void;
  onLocate: () => void;
  onOpenMods: () => void;
  onOpenGame: () => void;
  onLaunchGame: () => void;
  /// Opens the tested Zero Company UE4SS build on Nexus Mods in the browser.
  onGetUe4ss: () => void;
  /// Installs a UE4SS package the user has already downloaded.
  onInstallUe4ss: () => void;
  busy: boolean;
  launching: boolean;
  canLaunch?: boolean;
  existingModsFound?: number;
  onReviewExisting?: () => void;
  onDismissExisting?: () => void;
}

export function HomePage({ data, onInstall, onDiagnose, onLocate, onOpenMods, onOpenGame, onLaunchGame, onGetUe4ss, onInstallUe4ss, busy, launching, canLaunch = data.game.detected, existingModsFound, onReviewExisting, onDismissExisting }: Props) {
  const { game, ue4ss } = data;
  return <div className="page">
    <header className="hero">
      <div><p className="eyebrow">TACTICAL MOD CONTROL</p><h1>Star Wars: Zero Company</h1><p className="muted">A safe, purpose-built manager for packaged and UE4SS mods.</p></div>
      <div className="hero-actions"><StatusBadge status={game.detected ? "good" : "error"}>{game.detected ? "Game detected" : "Game not detected"}</StatusBadge><button className="primary" onClick={onLaunchGame} disabled={!canLaunch || launching}><Play aria-hidden size={17} />{launching ? "Launching…" : "Launch game"}</button></div>
    </header>
    {!game.detected && <section className="callout warning"><div><h2>Locate your game installation</h2><p>Automatic Steam discovery did not find a valid Zero Company installation.</p></div><button className="primary" onClick={onLocate}>Locate game</button></section>}
    {!!existingModsFound && <section className="callout warning existing-mod-callout"><div><h2>Existing mods found</h2><p>ZCOM found {existingModsFound} unmanaged mod{existingModsFound === 1 ? "" : "s"} that can be reviewed for migration.</p></div><button onClick={onDismissExisting}>Not now</button><button className="primary" onClick={onReviewExisting}>Review existing mods</button></section>}
    {data.previousBuildId && game.steamBuildId && data.previousBuildId !== game.steamBuildId && <section className="callout warning"><div><h2>Zero Company updated</h2><p>Build {data.previousBuildId} → {game.steamBuildId}. Review installed mods before playing.</p></div><button onClick={onDiagnose}>Review</button></section>}
    <section className="stats" aria-label="Game and mod status">
      <article><span>Steam build</span><strong>{game.steamBuildId ?? "—"}</strong><small>{game.source === "manual" ? "Manual location" : "Steam manifest"}</small></article>
      <article><span>Engine</span><strong>UE 5.6.1</strong><small>Known game engine</small></article>
      <article><span>Mods installed</span><strong>{data.installedMods}</strong><small>{data.enabledMods} enabled</small></article>
      <article><span>Conflicts</span><strong className={data.conflictCount ? "warn-text" : ""}>{data.conflictCount}</strong><small>{data.conflictCount ? "Review recommended" : "No overlap detected"}</small></article>
    </section>
    <div className="home-grid">
      <section className="panel action-panel"><div className="panel-heading"><div><p className="eyebrow">QUICK ACTION</p><h2>Install a downloaded mod</h2></div><Download aria-hidden /></div><p>Drop a ZIP, 7z, packaged mod file, or UE4SS Lua folder. The payload is staged and checked before anything reaches the game.</p><button className="primary large" onClick={onInstall}><Download aria-hidden size={18} />Install mod</button><button onClick={onOpenMods}><FolderOpen aria-hidden size={18} />Open mods folder</button></section>
      <section className="panel runtime"><div className="panel-heading"><h2>Runtime readiness</h2><ShieldCheck aria-hidden /></div>
        <div className="check-row"><PackageCheck aria-hidden /><div><b>Packaged mods</b><small>{game.detected ? "Deployment path ready" : "Waiting for game path"}</small></div><StatusBadge status={game.detected ? "good" : "unknown"}>{game.detected ? "Ready" : "Unknown"}</StatusBadge></div>
        <div className="check-row"><Puzzle aria-hidden /><div><b>UE4SS runtime</b><small>{ue4ss.message ?? (ue4ss.installed ? `${ue4ss.modCount} UE4SS mod${ue4ss.modCount === 1 ? "" : "s"} detected` : "Optional runtime not installed")}</small></div><StatusBadge status={ue4ss.healthy ? "good" : ue4ss.installed ? "warning" : "unknown"}>{ue4ss.installed ? ue4ss.healthy ? "Healthy" : "Attention" : "Not found"}</StatusBadge>
          <div className="row-links"><button className="link-button" onClick={onGetUe4ss}><ExternalLink aria-hidden size={14} />Get the tested build on Nexus Mods</button><button className="link-button" onClick={onInstallUe4ss} disabled={!game.detected || busy}><FileArchive aria-hidden size={14} />{ue4ss.installed ? "Update from downloaded package\u2026" : "Install from downloaded package\u2026"}</button></div>
        </div>
        <div className="check-row"><Activity aria-hidden /><div><b>Container verification</b><small>{data.retoc.version ?? "Configure retoc in Settings"}</small></div><StatusBadge status={data.retoc.found ? "good" : "warning"}>{data.retoc.found ? "Available" : "Setup needed"}</StatusBadge></div>
        <button className="text-button" onClick={onDiagnose}>Run Mod Doctor →</button>
      </section>
    </div>
    {game.path && <button className="path-line" onClick={onOpenGame} title="Open game folder"><FolderOpen aria-hidden size={15} /><span>{game.path}</span></button>}
  </div>;
}
