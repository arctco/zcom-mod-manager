import { Clipboard, RefreshCw, Stethoscope } from "lucide-react";
import { StatusBadge } from "../components/StatusBadge";
import type { DiagnosticReport } from "../types";

export function DiagnosticsPage({ report, loading, onRun, onCopy }: { report: DiagnosticReport | null; loading: boolean; onRun: () => void; onCopy: () => void }) {
  return <div className="page"><header className="page-header"><div><p className="eyebrow">MOD DOCTOR</p><h1>Diagnostics</h1><p className="muted">Check the game, mod deployment, UE4SS, retoc, and Proton configuration.</p></div><button className="primary" disabled={loading} onClick={onRun}><RefreshCw className={loading ? "spin" : ""} size={18} />{loading ? "Checking…" : "Run diagnostics"}</button></header>
    {!report ? <section className="empty-state"><Stethoscope aria-hidden size={34} /><h2>Ready for a health check</h2><p>Diagnostics are read-only. They do not change your game or Steam settings.</p><button className="primary" onClick={onRun}>Start check</button></section> : <><section className={`doctor-summary ${report.overall === "GOOD" ? "good" : "warning"}`}><Stethoscope aria-hidden size={30} /><div><span>Overall</span><strong>{report.overall}</strong></div><button onClick={onCopy}><Clipboard size={17} />Copy report</button></section><section className="diagnostic-list">{report.items.map(item => <article key={item.label}><StatusBadge status={item.status}>{item.label}</StatusBadge><div><b>{item.value}</b>{item.action && <p>{item.action}</p>}</div></article>)}</section></>}
  </div>;
}
