import { ArrowDown, ArrowUp, ExternalLink, FolderInput, FolderOpen, GripVertical, MoreHorizontal, Package, Pencil, ShieldCheck, Trash2, Trophy, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { StatusBadge } from "../components/StatusBadge";
import type { ConflictGroup, LoadOrderEntry, LoadOrderPreview, LoadOrderState, ModSummary } from "../types";
import { formatDate } from "../utils/format";

interface Props {
  mods: ModSummary[]; loadOrder: LoadOrderState; orderPreview: LoadOrderPreview | null;
  busy: string | null; orderBusy: boolean; onInstall: () => void;
  onToggle: (mod: ModSummary) => void; onUninstall: (mod: ModSummary) => void;
  onVerify: (mod: ModSummary) => void; onRename: (mod: ModSummary) => void;
  onOpenInstalled: (mod: ModSummary) => void;
  onOpenSource: (mod: ModSummary) => void; onBrowseNexus: () => void;
  onPreviewOrder: (ids: string[]) => void; onApplyOrder: (ids: string[]) => void;
  onApplyUe4ssOrder: (ids: string[]) => void; onCancelOrder: () => void;
  onDiscover?: () => void;
  discovering?: boolean;
}

type Filter = "all" | "enabled" | "disabled" | "conflicts";
type ModsTab = "library" | "load-order";
const filters: Array<[Filter, string]> = [["all", "All mods"], ["enabled", "Enabled only"], ["disabled", "Disabled only"], ["conflicts", "With conflicts"]];

function matches(mod: ModSummary, query: string, filter: Filter): boolean {
  if (filter === "enabled" && !mod.enabled) return false;
  if (filter === "disabled" && mod.enabled) return false;
  if (filter === "conflicts" && mod.potentialConflictCount === 0) return false;
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  return [mod.name, mod.version ?? "", mod.modType, ...mod.files.map(file => file.name)]
    .some(field => field.toLowerCase().includes(needle));
}

export function moveOrder(ids: string[], id: string, delta: -1 | 1): string[] {
  const index = ids.indexOf(id);
  const target = index + delta;
  if (index < 0 || target < 0 || target >= ids.length) return ids;
  const next = [...ids];
  [next[index], next[target]] = [next[target], next[index]];
  return next;
}

export function dropOrder(ids: string[], dragged: string, target: string, after: boolean): string[] {
  if (dragged === target || !ids.includes(dragged) || !ids.includes(target)) return ids;
  const next = ids.filter(id => id !== dragged);
  const targetIndex = next.indexOf(target);
  next.splice(targetIndex + (after ? 1 : 0), 0, dragged);
  return next;
}

export function winnerFor(group: ConflictGroup, order: string[], entries: LoadOrderEntry[]): string | null {
  if (group.memberIds.some(id => entries.some(entry => entry.id === id && !entry.supported))) return null;
  const enabled = new Set(entries.filter(entry => entry.enabled).map(entry => entry.id));
  return order.find(id => enabled.has(id) && group.memberIds.includes(id)) ?? null;
}

export function ModsPage({ mods, loadOrder, orderPreview, busy, orderBusy, onInstall, onToggle, onUninstall, onVerify, onRename, onOpenInstalled, onOpenSource, onBrowseNexus, onPreviewOrder, onApplyOrder, onApplyUe4ssOrder, onCancelOrder, onDiscover, discovering }: Props) {
  const [selected, setSelected] = useState<ModSummary | null>(null);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [tab, setTab] = useState<ModsTab>("library");
  const supported = useMemo(() => loadOrder.entries.filter(entry => entry.supported), [loadOrder.entries]);
  const unsupported = useMemo(() => loadOrder.entries.filter(entry => !entry.supported), [loadOrder.entries]);
  const canonicalIds = useMemo(() => supported.map(entry => entry.id), [supported]);
  const [draftIds, setDraftIds] = useState<string[]>(canonicalIds);
  const [dragged, setDragged] = useState<string | null>(null);
  const ue4ssIds = useMemo(() => loadOrder.ue4ssEntries.map(entry => entry.id), [loadOrder.ue4ssEntries]);
  const [ue4ssDraft, setUe4ssDraft] = useState<string[]>(ue4ssIds);
  useEffect(() => { setUe4ssDraft(ue4ssIds); }, [ue4ssIds.join("|")]); // eslint-disable-line react-hooks/exhaustive-deps
  const ue4ssById = useMemo(() => new Map(loadOrder.ue4ssEntries.map(entry => [entry.id, entry])), [loadOrder.ue4ssEntries]);
  const ue4ssDirty = ue4ssDraft.join("|") !== ue4ssIds.join("|");
  useEffect(() => { setDraftIds(canonicalIds); onCancelOrder(); }, [canonicalIds.join("|")]); // eslint-disable-line react-hooks/exhaustive-deps
  const byId = useMemo(() => new Map(loadOrder.entries.map(entry => [entry.id, entry])), [loadOrder.entries]);
  const names = useMemo(() => new Map(loadOrder.entries.map(entry => [entry.id, entry.name])), [loadOrder.entries]);
  // Only packaged mods have a deployment order. Saying so beside the list keeps
  // a library of UE4SS mods from looking like the editor lost them.
  const elsewhere = useMemo(() => {
    const counted = (type: ModSummary["modType"]) => mods.filter(mod => mod.modType === type).length;
    const entries: Array<[string, string]> = [];
    const runtime = counted("ue4ss");
    const fixed = counted("gamedir");
    if (runtime > 0) entries.push([`${runtime} UE4SS mod${runtime === 1 ? "" : "s"}`, "UE4SS loads these itself, in the order of its own mods.txt."]);
    if (fixed > 0) entries.push([`${fixed} game-folder mod${fixed === 1 ? "" : "s"}`, "These replace files at fixed paths, so there is nothing to order."]);
    return entries;
  }, [mods]);
  const conflicts = useMemo(() => [...new Map([...loadOrder.activeConflicts, ...loadOrder.potentialConflicts].map(group => [group.id, group])).values()], [loadOrder.activeConflicts, loadOrder.potentialConflicts]);
  const visible = useMemo(() => mods.filter(mod => matches(mod, query, filter)), [mods, query, filter]);
  const dirty = draftIds.join("|") !== canonicalIds.join("|");

  function changed(next: string[]) { setDraftIds(next); onCancelOrder(); }
  function dropOn(target: string, after: boolean, transferred?: string) {
    const source = transferred || dragged;
    if (!source || source === target) return;
    changed(dropOrder(draftIds, source, target, after)); setDragged(null);
  }
  function selectTab(next: ModsTab) {
    setTab(next);
    window.requestAnimationFrame(() => document.getElementById(`mods-${next}-tab`)?.focus());
  }

  const library = mods.length === 0 ? <EmptyState title="Your mod library is empty" body="Install a downloaded archive or mod folder to get started." action={<><button className="primary" onClick={onInstall}>Choose a mod</button><button onClick={onBrowseNexus}><ExternalLink aria-hidden size={17} />Browse on Nexus Mods</button></>} /> : <>
    <div className="list-toolbar">
      <input type="search" value={query} onChange={event => setQuery(event.target.value)} placeholder="Search by name, version, type, or file" aria-label="Search installed mods" />
      <select value={filter} onChange={event => setFilter(event.target.value as Filter)} aria-label="Filter installed mods">{filters.map(([id, label]) => <option key={id} value={id}>{label}</option>)}</select>
      <span className="result-count">{visible.length} of {mods.length} shown</span>
    </div>
    {visible.length === 0 ? <EmptyState title="No mods match this view" body="Clear the search box or choose a different filter to see the rest of your library." action={<button onClick={() => { setQuery(""); setFilter("all"); }}>Reset filters</button>} /> : <section className="mod-list" aria-label="Installed mods">
      <div className="mod-list-head"><span>Status</span><span>Mod</span><span>Type</span><span>Health</span><span>Installed</span><span>Actions</span></div>
      {visible.map(mod => <article className="mod-row" key={mod.id}>
        <label className="switch"><input type="checkbox" checked={mod.enabled} disabled={busy === mod.id} onChange={() => onToggle(mod)} /><span /><em>{mod.enabled ? "Enabled" : "Disabled"}</em></label>
        <div className="mod-name"><span className="mod-icon"><Package aria-hidden size={19} /></span><div><b>{mod.name}</b><small>{mod.version ? `Version ${mod.version}` : `${mod.files.length} managed file${mod.files.length === 1 ? "" : "s"}`}</small></div></div>
        <span className="type-chip">{mod.modType === "ue4ss" ? "UE4SS" : mod.modType === "iostore" ? "IoStore" : mod.modType === "gamedir" ? "Game folder" : "PAK"}</span>
        <StatusBadge status={mod.conflictCount ? "warning" : "good"}>{mod.conflictCount ? `${mod.conflictCount} active` : mod.potentialConflictCount ? `${mod.potentialConflictCount} potential` : "Healthy"}</StatusBadge>
        <span className="date">{formatDate(mod.installedAt)}</span>
        <div className="row-actions"><button title="Rename" aria-label={`Rename ${mod.name}`} onClick={() => onRename(mod)}><Pencil size={17} /></button><button title="Verify files" aria-label={`Verify ${mod.name}`} onClick={() => onVerify(mod)}><ShieldCheck size={17} /></button><button title="Open installed files" aria-label={`Open ${mod.name} files`} onClick={() => onOpenInstalled(mod)}><FolderOpen size={17} /></button><button title="Open managed source" aria-label={`Open ${mod.name} managed source`} onClick={() => onOpenSource(mod)}><FolderInput size={17} /></button><button className="danger-icon" title="Uninstall" aria-label={`Uninstall ${mod.name}`} onClick={() => onUninstall(mod)}><Trash2 size={17} /></button><button title="More details" aria-label={`More details for ${mod.name}`} onClick={() => setSelected(mod)}><MoreHorizontal size={17} /></button></div>
      </article>)}
    </section>}
  </>;

  const activeOverlaps = loadOrder.activeConflicts.length;
  // UE4SS starts DLL mods and Lua mods in two separate passes, so a single
  // sequence would promise an interleaving the runtime cannot deliver.
  const kindOf = (id: string) => ue4ssById.get(id)?.runtimeKind ?? "script";
  const nativeIds = ue4ssDraft.filter(id => kindOf(id) !== "script");
  const scriptIds = ue4ssDraft.filter(id => kindOf(id) === "script");
  function moveWithin(id: string, delta: -1 | 1) {
    const script = kindOf(id) === "script";
    const moved = moveOrder(script ? scriptIds : nativeIds, id, delta);
    setUe4ssDraft(script ? [...nativeIds, ...moved] : [...moved, ...scriptIds]);
  }
  const passList = (title: string, note: string, ids: string[]) => ids.length === 0 ? null : <div className="ue4ss-pass" key={title}>
    <h3>{title}</h3><p className="muted">{note}</p>
    <div className="order-list">
      {ids.map((id, index) => { const entry = ue4ssById.get(id); if (!entry) return null; return <article className={`order-row ${entry.enabled ? "" : "disabled"}`} key={id}>
        <span className="order-rank" aria-label={`Position ${index + 1}`}>{index + 1}</span>
        <div className="order-name"><b>{entry.name}</b><small>{entry.enabled ? entry.runtimeKind === "mixed" ? "DLL and Lua" : entry.runtimeKind === "native" ? "DLL mod" : "Lua mod" : "Disabled — position retained"}</small></div>
        <div className="order-buttons"><button aria-label={`Move ${entry.name} up`} disabled={index === 0 || orderBusy} onClick={() => moveWithin(id, -1)}><ArrowUp size={16} /></button><button aria-label={`Move ${entry.name} down`} disabled={index === ids.length - 1 || orderBusy} onClick={() => moveWithin(id, 1)}><ArrowDown size={16} /></button></div>
      </article>; })}
    </div>
  </div>;

  const ue4ssOrder = loadOrder.ue4ssEntries.length === 0 ? null : <section className="ue4ss-order" aria-label="UE4SS start order">
    <div className="order-explainer"><div><h2>UE4SS start order</h2><p>UE4SS reads <code>mods.txt</code> from the top down, but in two passes: every DLL mod starts first, then every Lua mod once the scripting runtime is ready. Order is therefore set within each pass, not across them.</p></div><span>{loadOrder.ue4ssEntries.length} mod{loadOrder.ue4ssEntries.length === 1 ? "" : "s"}</span></div>
    {passList("Starts first — DLL mods", "Native mods, started as UE4SS initializes.", nativeIds)}
    {passList("Starts second — Lua mods", "Script mods, started once the Lua runtime exists.", scriptIds)}
    {ue4ssDirty && <div className="order-changebar" role="status"><div><b>UE4SS start order changed</b><span>This rewrites the managed entries in mods.txt. Runtime entries and comments keep their place.</span></div><button onClick={() => setUe4ssDraft(ue4ssIds)} disabled={orderBusy}>Discard</button><button className="primary" onClick={() => onApplyUe4ssOrder(ue4ssDraft)} disabled={orderBusy}>{orderBusy ? "Writing…" : "Apply start order"}</button></div>}
  </section>;

  const orderView = loadOrder.entries.length === 0 && loadOrder.ue4ssEntries.length === 0 ? <EmptyState title="No packaged mods to order" body={elsewhere.length > 0 ? `Only IoStore and PAK mods have a deployment order. ${elsewhere.map(([label, reason]) => `${label}: ${reason}`).join(" ")}` : "IoStore and PAK mods appear here after installation. UE4SS mods keep their own runtime ordering."} /> : <div className="order-layout">
    <section className="order-main" aria-label="Packaged mod priority">
      {loadOrder.entries.length === 0 ? <div className="order-explainer"><div><h2>No packaged mods to order</h2><p>Only IoStore and PAK mods carry a deployment priority.</p></div></div> : <>
      <div className="order-explainer"><div><h2>Highest priority wins</h2><p>Move a mod toward the top to make it win overlapping packages. Nothing changes in the game folder until you review and apply.</p></div><span>{activeOverlaps} active overlap{activeOverlaps === 1 ? "" : "s"}</span></div>
      <div className="order-list">
        {draftIds.map((id, index) => { const entry = byId.get(id)!; const wins = loadOrder.activeConflicts.filter(group => winnerFor(group, draftIds, loadOrder.entries) === id).length; return <article className={`order-row ${entry.enabled ? "" : "disabled"}`} key={id} onDragOver={event => event.preventDefault()} onDrop={event => { const bounds = event.currentTarget.getBoundingClientRect(); dropOn(id, event.clientY >= bounds.top + bounds.height / 2, event.dataTransfer.getData("text/plain")); }}>
          <button className="drag-handle" draggable aria-label={`Drag ${entry.name}`} title="Drag to reorder" onDragStart={event => { event.dataTransfer.setData("text/plain", id); event.dataTransfer.effectAllowed = "move"; setDragged(id); }} onDragEnd={() => setDragged(null)}><GripVertical size={18} /></button>
          <span className="order-rank" aria-label={`Position ${index + 1}`}>{index + 1}</span>
          <div className="order-name"><b>{entry.name}</b><small>{entry.enabled ? entry.modType === "iostore" ? "IoStore" : "PAK" : "Disabled — position retained"}</small></div>
          <div className="order-health">{wins > 0 ? <span className="winner"><Trophy aria-hidden size={15} />Wins {wins}</span> : entry.activeConflictCount ? <span className="loser">Loses overlap</span> : entry.potentialConflictCount ? <span>Potential overlap</span> : <span>No known overlap</span>}</div>
          <div className="order-buttons"><button aria-label={`Move ${entry.name} up`} disabled={index === 0 || orderBusy} onClick={() => changed(moveOrder(draftIds, id, -1))}><ArrowUp size={16} /></button><button aria-label={`Move ${entry.name} down`} disabled={index === draftIds.length - 1 || orderBusy} onClick={() => changed(moveOrder(draftIds, id, 1))}><ArrowDown size={16} /></button></div>
        </article>; })}
      </div>
      {unsupported.length > 0 && <section className="unsupported-order"><h3>Not orderable yet</h3>{unsupported.map(entry => <div key={entry.id}><b>{entry.name}</b><span>{entry.supportReason}</span></div>)}</section>}
      {elsewhere.length > 0 && <section className="unsupported-order"><h3>Ordered elsewhere</h3>{elsewhere.map(([label, reason]) => <div key={label}><b>{label}</b><span>{reason}</span></div>)}</section>}
      {(dirty || loadOrder.unapplied) && <div className="order-changebar" role="status"><div><b>{dirty ? "Load order changed" : "Deployment names need normalization"}</b><span>Review the exact filenames before applying.</span></div><button onClick={() => { setDraftIds(canonicalIds); onCancelOrder(); }} disabled={orderBusy}>Discard</button><button className="primary" onClick={() => onPreviewOrder(draftIds)} disabled={orderBusy}>Review changes</button></div>}
      </>}
      {ue4ssOrder}
      {orderPreview && <section className="order-review panel" aria-label="Load order review"><h2>Review deployment changes</h2><p>Close Zero Company before applying. The manager will checksum every current file and roll back if any rename or database update fails.</p><div className="review-stats"><b>{orderPreview.moves.length}</b><span>file rename{orderPreview.moves.length === 1 ? "" : "s"}</span><b>{orderPreview.winnerChanges.length}</b><span>winner change{orderPreview.winnerChanges.length === 1 ? "" : "s"}</span></div>{orderPreview.moves.length > 0 && <ul className="move-list">{orderPreview.moves.map((move, index) => <li key={`${move.modId}-${index}`}><span>{move.from}</span><b>→</b><span>{move.to}</span></li>)}</ul>}<footer className="dialog-actions"><button onClick={onCancelOrder} disabled={orderBusy}>Back</button><button className="primary" onClick={() => onApplyOrder(orderPreview.orderedModIds)} disabled={orderBusy}>{orderBusy ? "Applying…" : "Apply order"}</button></footer></section>}
    </section>
    <aside className="conflict-panel panel"><p className="eyebrow">CONFLICT WINNERS</p><h2>Package overlaps</h2>{conflicts.length === 0 ? <p className="muted">No package-level overlaps are known.</p> : conflicts.map(group => { const winner = winnerFor(group, draftIds, loadOrder.entries); const unsupportedMember = group.memberIds.some(id => byId.get(id)?.supported === false); const status = group.active && group.potential ? "Active + potential" : group.active ? "Active" : "Potential"; return <article key={group.id}><span>{status} · {group.packageCount} package{group.packageCount === 1 ? "" : "s"}</span><b>{group.memberIds.map(id => names.get(id) ?? "Unknown mod").join(" ↔ ")}</b><small>{group.active && winner ? `${names.get(winner)} wins` : unsupportedMember ? "Winner unavailable for an unsupported layout" : "Enable at least two mods to choose a winner"}</small></article>; })}</aside>
  </div>;

  return <div className="page">
    <header className="page-header"><div><p className="eyebrow">MANAGED LIBRARY</p><h1>Mods</h1><p className="muted">Every deployed file is ownership-tracked and checksum guarded.</p></div><div className="header-actions"><button onClick={onDiscover} disabled={discovering}>{discovering ? "Scanning…" : "Discover existing mods"}</button><button className="primary" onClick={onInstall}>Install mod</button></div></header>
    <div className="page-tabs" role="tablist" aria-label="Mods views" onKeyDown={event => { if (event.key === "ArrowLeft" || event.key === "ArrowRight") { event.preventDefault(); selectTab(tab === "library" ? "load-order" : "library"); } }}><button id="mods-library-tab" role="tab" aria-controls="mods-library-panel" aria-selected={tab === "library"} tabIndex={tab === "library" ? 0 : -1} className={tab === "library" ? "active" : ""} onClick={() => setTab("library")}>Library</button><button id="mods-load-order-tab" role="tab" aria-controls="mods-load-order-panel" aria-selected={tab === "load-order"} tabIndex={tab === "load-order" ? 0 : -1} className={tab === "load-order" ? "active" : ""} onClick={() => setTab("load-order")}>Load order</button></div>
    <div id={`mods-${tab}-panel`} role="tabpanel" aria-labelledby={`mods-${tab}-tab`}>{tab === "library" ? library : orderView}</div>
    {selected && <section className="panel mod-details" aria-label={`${selected.name} details`}><button className="detail-close" onClick={() => setSelected(null)} aria-label="Close details"><X size={17} /></button><p className="eyebrow">MOD DETAILS</p><h2>{selected.name}</h2><div className="detail-list"><div><span>Version</span><b>{selected.version ?? "Not provided"}</b></div><div><span>Type</span><b>{selected.modType}</b></div><div><span>Status</span><b>{selected.enabled ? "Enabled" : "Disabled"}</b></div><div><span>Game build when installed</span><b>{selected.installedBuild ?? "Unknown"}</b></div><div><span>Packages</span><b>{selected.packageCount}</b></div><div><span>Active conflicts</span><b>{selected.conflictCount}</b></div></div><h3>Managed files</h3><ul className="file-list">{selected.files.map(file => <li key={file.destination}>{file.name} · {file.sha256.slice(0, 12)}…</li>)}</ul></section>}
  </div>;
}
