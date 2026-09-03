import { AlertTriangle, Archive, ArrowUpCircle, Check, ChevronRight, Download, FileArchive, FolderOpen, Pencil, ShieldCheck, X } from "lucide-react";
import { FomodWizard } from "../components/FomodWizard";
import { StatusBadge } from "../components/StatusBadge";
import type { DownloadProgress, FomodAnswer, FomodSession, ModPreview, PreviewType } from "../types";
import { formatBytes } from "../utils/format";

const typeLabel: Record<PreviewType, string> = {
  iostore: "IoStore packaged mod",
  pak: "PAK-only mod",
  ue4ss: "UE4SS mod",
  gamedir: "Game folder mod",
  "ue4ss-runtime": "UE4SS runtime package"
};

interface Props {
  previews: ModPreview[];
  names: Record<string, string>;
  loading: boolean;
  download: DownloadProgress | null;
  advanced: boolean;
  installing: string | null;
  /** The scripted installer awaiting answers, when the download carries one. */
  installer: FomodSession | null;
  installerRestored: FomodAnswer | null;
  installerCanGoBack: boolean;
  onInstallerNext: (answer: FomodAnswer) => void;
  onInstallerBack: () => void;
  onAdvanced: () => void;
  onName: (stagingId: string, name: string) => void;
  onChooseFile: () => void;
  onChooseFolder: () => void;
  onInstall: (preview: ModPreview) => void;
  onInstallAll: (previews: ModPreview[]) => void;
  onInstallRuntime: (preview: ModPreview) => void;
  onCancel: () => void;
}

function verificationText(preview: ModPreview): string {
  if (preview.verification === "passed") return "✓ retoc verification passed";
  if (preview.verification === "not-required") return "✓ Not required";
  if (preview.verification === "unavailable") return "retoc setup required";
  return "Verification failed";
}

function Candidate({ preview, name, advanced, installing, onName, onAdvanced, onInstall, onInstallRuntime }: {
  preview: ModPreview; name: string; advanced: boolean; installing: string | null;
  onName: (stagingId: string, name: string) => void; onAdvanced: () => void;
  onInstall: (preview: ModPreview) => void; onInstallRuntime: (preview: ModPreview) => void;
}) {
  const runtime = preview.modType === "ue4ss-runtime";
  const busy = installing !== null;
  const thisBusy = installing === preview.stagingId || installing === "all";
  const upgrade = preview.replaces;
  return <section className="panel preview-main">
    <div className="preview-title">
      <div className="mod-icon large"><Archive aria-hidden /></div>
      <div className="preview-naming">
        <p className="eyebrow">{runtime ? "RUNTIME PACKAGE" : preview.optionLabel ? `ARCHIVE OPTION · ${preview.optionLabel}` : "INSTALLATION PREVIEW"}</p>
        {runtime ? <h2>{preview.name}</h2> : <label className="name-field">
          <span><Pencil aria-hidden size={14} />Mod name</span>
          <input value={name} onChange={event => onName(preview.stagingId, event.target.value)} maxLength={120} aria-label="Mod name" placeholder={preview.name} />
        </label>}
        {preview.author && <p>by {preview.author}</p>}
        {preview.version && <p className="muted">Version {preview.version}</p>}
      </div>
      <StatusBadge status={preview.valid ? "good" : "error"}>{preview.valid ? (runtime ? "Ready to set up" : "Ready to install") : "Validation failed"}</StatusBadge>
    </div>
    {preview.description && <p className="description">{preview.description}</p>}
    {upgrade && <div className="inline-note upgrade-note" role="status"><b><ArrowUpCircle aria-hidden size={16} />Replaces {upgrade.name}{upgrade.version ? ` ${upgrade.version}` : ""}</b><span>{upgrade.reason} Installing puts this in its place, keeps its position in the load order, and removes the old version only once the new one is deployed.</span></div>}
    <div className="detail-list">
      <div><span>Detected type</span><b>{typeLabel[preview.modType]}</b></div>
      <div><span>Container</span><b className={preview.verification === "passed" || preview.verification === "not-required" ? "success-text" : "warn-text"}>{verificationText(preview)}</b></div>
      <div><span>Packages modified</span><b>{preview.packageCount || "Unknown"}</b></div>
      <div><span>Game compatibility</span><StatusBadge status={preview.compatibility}>{preview.compatibilityMessage}</StatusBadge></div>
    </div>
    <h3>{runtime ? "Runtime files" : "Files"}</h3>
    <ul className="file-list">{preview.files.map(file => <li key={file}><Check aria-hidden size={16} />{file}</li>)}</ul>
    {preview.conflicts.length > 0 && <section className="install-conflicts" aria-label="Detected mod conflicts">
      <div className="inline-warning"><AlertTriangle aria-hidden size={17} /><div><b>Overlaps {preview.conflicts.length} installed mod{preview.conflicts.length === 1 ? "" : "s"}</b><span>{preview.loadOrderSupported ? "This mod will be installed at the highest priority and win these package conflicts." : `This layout is not orderable yet. ${preview.loadOrderSupportReason ?? "No winner will be claimed."}`}</span></div></div>
      <ul>{preview.conflicts.map(conflict => <li key={conflict.modId}><b>{conflict.name}</b><span>{conflict.packageCount} overlapping package{conflict.packageCount === 1 ? "" : "s"}</span></li>)}</ul>
    </section>}
    {preview.warnings.map(warning => <div className="inline-warning" key={warning}><AlertTriangle aria-hidden size={17} />{warning}</div>)}
    <button className="disclosure" onClick={onAdvanced} aria-expanded={advanced}><ChevronRight className={advanced ? "rotated" : ""} size={16} />Advanced details</button>
    {advanced && <div className="advanced"><p>{preview.verificationDetails ?? "No additional tool output."}</p>{preview.packageNames.length > 0 && <><h3>Package paths (spoilers possible)</h3><code>{preview.packageNames.join("\n")}</code></>}</div>}
    <footer className="dialog-actions">
      {runtime
        ? <button className="primary" onClick={() => onInstallRuntime(preview)} disabled={busy}><Download size={17} />{thisBusy ? "Setting up…" : "Install UE4SS runtime"}</button>
        : <button className="primary" onClick={() => onInstall(preview)} disabled={!preview.valid || busy}>{upgrade ? <ArrowUpCircle size={17} /> : <ShieldCheck size={17} />}{thisBusy ? (upgrade ? "Replacing…" : "Installing…") : upgrade ? "Replace installed version" : "Install"}</button>}
    </footer>
  </section>;
}

export function InstallPage({ previews, names, loading, download, advanced, installing, installer, installerRestored, installerCanGoBack, onInstallerNext, onInstallerBack, onAdvanced, onName, onChooseFile, onChooseFolder, onInstall, onInstallAll, onInstallRuntime, onCancel }: Props) {
  const many = previews.length > 1;
  const optionCount = previews.filter(preview => preview.optionLabel).length;
  const additionalCount = previews.length - optionCount;
  const canInstallAll = many
    && optionCount === 0
    && previews.every(preview => preview.modType !== "ue4ss-runtime" && preview.valid);
  return <div className="page install-page">
    <header className="page-header"><div><p className="eyebrow">SAFE INSTALLER</p><h1>{installer ? "Choose your options" : previews.length ? "Review mod" : "Install a mod"}</h1><p className="muted">Nothing is deployed until validation succeeds and you confirm.</p></div>{previews.length > 0 && <button onClick={onCancel}><X size={17} />{many ? "Cancel all" : "Cancel"}</button>}</header>
    {installer
      // An archive that scripts its own installation asks its questions first;
      // the answers decide which of its files are read as mods to review.
      ? <FomodWizard session={installer} restored={installerRestored} busy={loading} canGoBack={installerCanGoBack} onNext={onInstallerNext} onBack={onInstallerBack} onCancel={onCancel} />
      : previews.length === 0
      ? download
        // A handoff from the website: the transfer is the whole screen until
        // there is a payload to review, so it is never a silent wait.
        ? <section className="drop-zone downloading" aria-busy>
          <div className="drop-icon"><Download aria-hidden size={32} /></div>
          <h2>{download.name ? "Downloading from Nexus Mods" : "Contacting Nexus Mods…"}</h2>
          <p>{download.name || "Resolving the download link for this file."}</p>
          <div className="download-progress" role="progressbar" aria-label="Download progress" aria-valuemin={0} aria-valuemax={download.total ?? undefined} aria-valuenow={download.total ? download.done : undefined}>
            <div className={`download-bar ${download.total ? "" : "indeterminate"}`} style={download.total ? { width: `${Math.min(100, Math.round((download.done / download.total) * 100))}%` } : undefined} />
          </div>
          <p className="download-line">{download.total
            ? `${formatBytes(download.done)} of ${formatBytes(download.total)} · ${Math.min(100, Math.round((download.done / download.total) * 100))}%`
            : download.done > 0 ? formatBytes(download.done) : "Starting…"}</p>
          <small>The file is inspected and confirmed before anything is installed.</small>
        </section>
        : <section className={`drop-zone ${loading ? "loading" : ""}`} aria-busy={loading}>
          <div className="drop-icon"><Archive aria-hidden size={32} /></div>
          <h2>{loading ? "Inspecting payload…" : "Drop a mod here"}</h2>
          <p>ZIP, 7z, PAK, UTOC/UCAS, a UE4SS Lua or DLL mod, or a game-folder mod</p>
          <div><button className="primary" onClick={onChooseFile} disabled={loading}><FileArchive size={18} />Choose archive or file</button><button onClick={onChooseFolder} disabled={loading}><FolderOpen size={18} />Choose folder</button></div>
          <small>Archives are treated as untrusted input and extracted into a temporary sandbox.</small>
        </section>
      : <div className="preview-grid">
        <div className="preview-stack">
          {many && <div className="inline-note" role="status"><b>{optionCount ? `${optionCount} packaged options${additionalCount ? ` and ${additionalCount} additional mod${additionalCount === 1 ? "" : "s"}` : ""} found` : `${previews.length} components found in this download`}</b><span>{optionCount ? "Each containing folder is a separate version or component. Install only the option or options you want; alternatives may conflict if installed together." : "Review every component below. You can install the complete bundle at once or install components separately."}</span>{canInstallAll && <button className="primary" onClick={() => onInstallAll(previews)} disabled={installing !== null}><ShieldCheck size={17} />{installing === "all" ? "Installing bundle…" : previews.some(preview => preview.replaces) ? "Update all components" : "Install all components"}</button>}</div>}
          {previews.map(preview => <Candidate key={preview.stagingId} preview={preview} name={names[preview.stagingId] ?? preview.name} advanced={advanced} installing={installing} onName={onName} onAdvanced={onAdvanced} onInstall={onInstall} onInstallRuntime={onInstallRuntime} />)}
        </div>
        <aside className="panel safety-note"><ShieldCheck aria-hidden /><h2>Safe by default</h2><p>The manager keeps its own source copy, deploys only recognized payload files, and records a SHA-256 checksum for every destination.</p><ul><li>No executables are run</li><li>Unknown files are ignored</li><li>Partial installs roll back</li><li>Replaced game files are kept and restored</li><li>Changed files are kept on removal</li></ul></aside>
      </div>}
  </div>;
}
