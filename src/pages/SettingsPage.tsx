import { ExternalLink } from "lucide-react";
import { NexusPanel } from "../components/NexusPanel";
import type { AppSettings, Links, NexusAccount, NexusStatus, ToolInfo } from "../types";

interface Props {
  settings: AppSettings; retoc: ToolInfo; onChange: (settings: AppSettings) => void;
  onSave: () => void; onPickGame: () => void; onPickRetoc: () => void;
  onOpenLogs: () => void; onOpenData: () => void;
  links: Links; onOpenLink: (url: string) => void;
  nexus: NexusStatus | null; nexusAccount: NexusAccount | null;
  onSaveNexusKey: (key: string) => Promise<void>;
  onClearNexusKey: () => Promise<void>;
  onToggleNxmHandler: (enabled: boolean) => Promise<void>;
}

export function SettingsPage({ settings, retoc, onChange, onSave, onPickGame, onPickRetoc, onOpenLogs, onOpenData, links, onOpenLink, nexus, nexusAccount, onSaveNexusKey, onClearNexusKey, onToggleNxmHandler }: Props) {
  return <div className="page"><header className="page-header"><div><p className="eyebrow">APPLICATION</p><h1>Settings</h1><p className="muted">Local-only preferences. ZCOM Mod Manager has no telemetry or account.</p></div><button className="primary" onClick={onSave}>Save settings</button></header>
    <section className="settings-sections"><article className="panel"><h2>Game installation</h2><p>Automatic Steam discovery is preferred. Set a manual location if the game is in an unusual library.</p><label>Game directory<div className="input-action"><input readOnly value={settings.gamePath ?? "Automatic detection"} /><button onClick={onPickGame}>Browse</button></div></label></article>
      <article className="panel"><h2>retoc container tool</h2><p>Version 0.1.5 is bundled in release builds and discovered locally in development.</p><label>retoc executable<div className="input-action"><input readOnly value={settings.retocPath ?? retoc.path ?? "Not found"} /><button onClick={onPickRetoc}>Browse</button></div></label><small className={retoc.found ? "success-text" : "warn-text"}>{retoc.found ? `Detected ${retoc.version ?? "retoc"}` : "Required to validate IoStore mods"}</small></article>
      <article className="panel"><h2>Privacy and display</h2><label>Log detail<select value={settings.logLevel} onChange={e => onChange({ ...settings, logLevel: e.target.value as AppSettings["logLevel"] })}><option value="normal">Normal — sanitized</option><option value="verbose">Verbose</option><option value="developer">Developer — may expose game paths</option></select></label><label className="check"><input type="checkbox" checked={settings.advancedPackageNames} onChange={e => onChange({ ...settings, advancedPackageNames: e.target.checked })} />Allow raw package names in advanced views</label><label className="check"><input type="checkbox" checked={settings.reducedMotion} onChange={e => onChange({ ...settings, reducedMotion: e.target.checked })} />Reduce interface motion</label><div className="settings-actions"><button onClick={onOpenLogs}>Open logs folder</button><button onClick={onOpenData}>Open app data</button></div></article>
      <NexusPanel status={nexus} account={nexusAccount} onSaveKey={onSaveNexusKey} onClearKey={onClearNexusKey} onToggleHandler={onToggleNxmHandler} onOpenLink={onOpenLink} />
      <article className="panel"><h2>Community resources</h2><p>Links open in your browser. ZCOM Mod Manager never downloads anything by itself.</p><div className="settings-actions"><button onClick={() => onOpenLink(links.nexusGame)}><ExternalLink aria-hidden size={16} />Zero Company mods on Nexus</button><button onClick={() => onOpenLink(links.ue4ssDownload)}><ExternalLink aria-hidden size={16} />UE4SS for Zero Company</button><button onClick={() => onOpenLink(links.project)}><ExternalLink aria-hidden size={16} />Project repository</button></div></article>
    </section>
  </div>;
}
