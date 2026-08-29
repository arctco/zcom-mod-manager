import { ExternalLink, FolderInput, FolderOpen, MoreHorizontal, Package, ShieldCheck, Trash2, X } from "lucide-react";
import { useMemo, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { StatusBadge } from "../components/StatusBadge";
import type { ModSummary } from "../types";
import { formatDate } from "../utils/format";

interface Props {
  mods: ModSummary[]; busy: string | null; onInstall: () => void;
  onToggle: (mod: ModSummary) => void; onUninstall: (mod: ModSummary) => void;
  onVerify: (mod: ModSummary) => void; onOpenInstalled: (mod: ModSummary) => void;
  onOpenSource: (mod: ModSummary) => void; onBrowseNexus: () => void;
}

type Filter = "all" | "enabled" | "disabled" | "conflicts";
const filters: Array<[Filter, string]> = [["all", "All mods"], ["enabled", "Enabled only"], ["disabled", "Disabled only"], ["conflicts", "With conflicts"]];

function matches(mod: ModSummary, query: string, filter: Filter): boolean {
  if (filter === "enabled" && !mod.enabled) return false;
  if (filter === "disabled" && mod.enabled) return false;
  if (filter === "conflicts" && mod.conflictCount === 0) return false;
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  return [mod.name, mod.version ?? "", mod.modType, ...mod.files.map(file => file.name)]
    .some(field => field.toLowerCase().includes(needle));
}

export function ModsPage({ mods, busy, onInstall, onToggle, onUninstall, onVerify, onOpenInstalled, onOpenSource, onBrowseNexus }: Props) {
  const [selected, setSelected] = useState<ModSummary | null>(null);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const visible = useMemo(() => mods.filter(mod => matches(mod, query, filter)), [mods, query, filter]);
  return <div className="page">
    <header className="page-header"><div><p className="eyebrow">MANAGED LIBRARY</p><h1>Mods</h1><p className="muted">Every deployed file is ownership-tracked and checksum guarded.</p></div><button className="primary" onClick={onInstall}>Install mod</button></header>
    {mods.length === 0 ? <EmptyState title="Your mod library is empty" body="Install a downloaded archive or mod folder to get started." action={<><button className="primary" onClick={onInstall}>Choose a mod</button><button onClick={onBrowseNexus}><ExternalLink aria-hidden size={17} />Browse on Nexus Mods</button></>} /> : <>
      <div className="list-toolbar">
        <input type="search" value={query} onChange={e => setQuery(e.target.value)} placeholder="Search by name, version, type, or file" aria-label="Search installed mods" />
        <select value={filter} onChange={e => setFilter(e.target.value as Filter)} aria-label="Filter installed mods">{filters.map(([id, label]) => <option key={id} value={id}>{label}</option>)}</select>
        <span className="result-count">{visible.length} of {mods.length} shown</span>
      </div>
      {visible.length === 0 ? <EmptyState title="No mods match this view" body="Clear the search box or choose a different filter to see the rest of your library." action={<button onClick={() => { setQuery(""); setFilter("all"); }}>Reset filters</button>} /> : <section className="mod-list" aria-label="Installed mods">
        <div className="mod-list-head"><span>Status</span><span>Mod</span><span>Type</span><span>Health</span><span>Installed</span><span>Actions</span></div>
        {visible.map(mod => <article className="mod-row" key={mod.id}>
          <label className="switch"><input type="checkbox" checked={mod.enabled} disabled={busy === mod.id} onChange={() => onToggle(mod)} /><span /><em>{mod.enabled ? "Enabled" : "Disabled"}</em></label>
          <div className="mod-name"><span className="mod-icon"><Package aria-hidden size={19} /></span><div><b>{mod.name}</b><small>{mod.version ? `Version ${mod.version}` : `${mod.files.length} managed file${mod.files.length === 1 ? "" : "s"}`}</small></div></div>
          <span className="type-chip">{mod.modType === "ue4ss" ? "UE4SS Lua" : mod.modType === "iostore" ? "IoStore" : "PAK"}</span>
          <StatusBadge status={mod.conflictCount ? "warning" : "good"}>{mod.conflictCount ? `${mod.conflictCount} conflict${mod.conflictCount === 1 ? "" : "s"}` : "Healthy"}</StatusBadge>
          <span className="date">{formatDate(mod.installedAt)}</span>
          <div className="row-actions"><button title="Verify files" aria-label={`Verify ${mod.name}`} onClick={() => onVerify(mod)}><ShieldCheck size={17} /></button><button title="Open installed files" aria-label={`Open ${mod.name} files`} onClick={() => onOpenInstalled(mod)}><FolderOpen size={17} /></button><button title="Open managed source" aria-label={`Open ${mod.name} managed source`} onClick={() => onOpenSource(mod)}><FolderInput size={17} /></button><button className="danger-icon" title="Uninstall" aria-label={`Uninstall ${mod.name}`} onClick={() => onUninstall(mod)}><Trash2 size={17} /></button><button title="More details" aria-label={`More details for ${mod.name}`} onClick={() => setSelected(mod)}><MoreHorizontal size={17} /></button></div>
        </article>)}
      </section>}
    </>}
    {selected && <section className="panel mod-details" aria-label={`${selected.name} details`}><button className="detail-close" onClick={() => setSelected(null)} aria-label="Close details"><X size={17} /></button><p className="eyebrow">MOD DETAILS</p><h2>{selected.name}</h2><div className="detail-list"><div><span>Version</span><b>{selected.version ?? "Not provided"}</b></div><div><span>Type</span><b>{selected.modType}</b></div><div><span>Status</span><b>{selected.enabled ? "Enabled" : "Disabled"}</b></div><div><span>Game build when installed</span><b>{selected.installedBuild ?? "Unknown"}</b></div><div><span>Packages</span><b>{selected.packageCount}</b></div><div><span>Conflicts</span><b>{selected.conflictCount}</b></div></div><h3>Managed files</h3><ul className="file-list">{selected.files.map(file => <li key={file.destination}>{file.name} · {file.sha256.slice(0, 12)}…</li>)}</ul></section>}
  </div>;
}
