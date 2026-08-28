import { Activity, Download, Home, Package, Settings } from "lucide-react";

export type Page = "home" | "mods" | "install" | "diagnostics" | "settings";
const nav: Array<[Page, string, typeof Home]> = [
  ["home", "Home", Home], ["mods", "Mods", Package], ["install", "Install", Download],
  ["diagnostics", "Diagnostics", Activity], ["settings", "Settings", Settings]
];

export function Shell({ page, onPage, gameReady, children }: { page: Page; onPage: (page: Page) => void; gameReady: boolean; children: React.ReactNode }) {
  return <div className="shell">
    <aside className="sidebar">
      <button className="brand" onClick={() => onPage("home")} aria-label="ZCOM Mod Manager home">
        <span className="brand-mark">ZC</span><span><b>ZCOM</b><small>MOD MANAGER</small></span>
      </button>
      <nav aria-label="Primary navigation">
        {nav.map(([id, label, Icon]) => <button key={id} className={page === id ? "active" : ""} aria-current={page === id ? "page" : undefined} onClick={() => onPage(id)}><Icon aria-hidden size={19} />{label}</button>)}
      </nav>
      <div className="sidebar-status"><span className={gameReady ? "pulse good" : "pulse"} /> <span>{gameReady ? "Game connected" : "Game not found"}</span></div>
      <div className="version">v0.1.0</div>
    </aside>
    <main className="main">{children}</main>
  </div>;
}
