import { ExternalLink, Github, RefreshCw, ShieldCheck } from "lucide-react";
import type { UpdateInfo } from "../types";

export function AboutPage({ projectUrl, nexusUrl, onOpenLink, update, checking, error, onCheckUpdates }: { projectUrl: string; nexusUrl: string; onOpenLink: (url: string) => void; update: UpdateInfo | null; checking: boolean; error: string | null; onCheckUpdates: () => void }) {
  return <div className="page about-page">
    <header className="page-header"><div><p className="eyebrow">ZCOM MOD MANAGER</p><h1>About</h1><p className="muted">A focused, open-source mod manager for Star Wars: Zero Company.</p></div></header>
    <section className="about-grid">
      <article className="panel about-intro"><ShieldCheck aria-hidden size={32} /><div><h2>Safer mod management</h2><p>ZCOM keeps a managed library of your mods, validates supported archives before installation, tracks deployed files by checksum, and helps identify conflicts or game-update compatibility problems.</p><p>It supports IoStore, PAK, and UE4SS mods while keeping installation, enable/disable, verification, and removal in one place.</p></div></article>
      <article className="panel version-card"><p className="eyebrow">INSTALLED VERSION</p><strong>v{__APP_VERSION__}</strong><p className="muted">GitHub is checked automatically when the manager opens. You can retry here at any time.</p><button className="primary" disabled={checking} onClick={onCheckUpdates}><RefreshCw className={checking ? "spin" : ""} aria-hidden size={17} />{checking ? "Checking GitHub…" : "Check again"}</button>
        <div className="update-result" aria-live="polite">
          {update && (update.updateAvailable
            // A release is published in both places, so the update offers both
            // rather than assuming where this copy came from.
            ? <><b className="warn-text">Version {update.latestVersion} is available.</b><button className="link-button" onClick={() => onOpenLink(update.releaseUrl)}>Open release page <ExternalLink aria-hidden size={14} /></button><button className="link-button" onClick={() => onOpenLink(nexusUrl)}>Get it on Nexus Mods <ExternalLink aria-hidden size={14} /></button></>
            : <b className="success-text">You’re up to date. GitHub’s latest release is v{update.latestVersion}.</b>)}
          {error && <b className="status-error">Couldn’t check GitHub: {error}</b>}
        </div>
      </article>
      <article className="panel about-project"><h2>Community project</h2><p>ZCOM Mod Manager is an independent community project and is not affiliated with or endorsed by Lucasfilm Games, Electronic Arts, Bit Reactor, or Respawn Entertainment.</p><div className="settings-actions"><button onClick={() => onOpenLink(projectUrl)}><Github aria-hidden size={17} />View source on GitHub</button><button onClick={() => onOpenLink(`${projectUrl}/releases`)}><ExternalLink aria-hidden size={17} />All releases</button><button onClick={() => onOpenLink(nexusUrl)}><ExternalLink aria-hidden size={17} />Nexus Mods page</button></div><small>Licensed under GNU GPLv3-only · © 2026 Victor Hugo (arctco)</small></article>
    </section>
  </div>;
}
