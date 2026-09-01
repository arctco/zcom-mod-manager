import { AlertTriangle, ExternalLink, KeyRound, Loader2 } from "lucide-react";
import { useState } from "react";
import type { NexusAccount, NexusStatus } from "../types";

interface Props {
  status: NexusStatus | null;
  account: NexusAccount | null;
  onSaveKey: (key: string) => Promise<void>;
  onClearKey: () => Promise<void>;
  onToggleHandler: (enabled: boolean) => Promise<void>;
  onOpenLink: (url: string) => void;
  /** Saved on the spot, like the key and the handler beside it. */
  autoCheck: boolean;
  onAutoCheckChange: (enabled: boolean) => Promise<void>;
}

const API_KEY_PAGE = "https://www.nexusmods.com/users/myaccount?tab=api";

export function NexusPanel({ status, account, onSaveKey, onClearKey, onToggleHandler, onOpenLink, autoCheck, onAutoCheckChange }: Props) {
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);

  async function run(action: () => Promise<void>) {
    setBusy(true);
    try { await action(); } finally { setBusy(false); }
  }

  return <article className="panel">
    <h2>Nexus Mods downloads</h2>
    <p>
      Downloads always start on the Nexus Mods website. Press <b>Mod Manager Download</b>
      {" "}on a mod page and the link is handed to this application, which fetches the file
      and runs it through the same checks as a mod you pick by hand. Nothing is downloaded
      on its own, and the manager never browses Nexus for you.
    </p>

    <label>
      Personal API key
      <div className="input-action">
        <input
          type="password"
          value={key}
          onChange={e => setKey(e.target.value)}
          placeholder={status?.hasKey ? "A key is stored" : "Paste your key"}
          autoComplete="off"
          spellCheck={false}
          aria-label="Nexus Mods personal API key"
        />
        <button disabled={busy || !key.trim()} onClick={() => void run(async () => { await onSaveKey(key); setKey(""); })}>
          {busy ? <Loader2 className="spin" size={16} /> : <KeyRound size={16} />}Verify and save
        </button>
      </div>
    </label>
    <small className={status?.hasKey ? "success-text" : "muted"}>
      {account ? `Connected as ${account.name} · ${account.premium ? "premium" : "free"} account`
        : status?.accountName ? `Connected as ${status.accountName} · ${status.premium ? "premium" : "free"} account`
          : status?.hasKey ? "A key is stored. Save a new one to replace it."
            : "No key stored yet."}
    </small>

    {status?.storage === "database" && <div className="inline-warning">
      <AlertTriangle aria-hidden size={17} />
      No system secret store was available, so the key is saved in the application
      database as plain text. Install a keyring service to have it protected.
    </div>}

    <label className="check">
      <input
        type="checkbox"
        checked={status?.handlerRegistered ?? false}
        disabled={busy}
        onChange={e => void run(() => onToggleHandler(e.target.checked))}
      />
      Handle <code>nxm://</code> links from the browser
    </label>
    <small>
      Claims the protocol for this application. Leave it off if another mod manager
      should keep it; turning it off hands the association back.
    </small>
    {status && !status.handlerRegistered && status.handlerProblem && <div className="inline-warning">
      <AlertTriangle aria-hidden size={17} />{status.handlerProblem}
    </div>}
    {status && !status.handlerRegistered && !status.handlerProblem && status.handlerOwner && <div className="inline-warning">
      <AlertTriangle aria-hidden size={17} />
      <span><code>nxm://</code> links currently open in <b>{status.handlerOwner}</b>. Enabling the
      switch above claims them for this application instead.</span>
    </div>}

    <label className="check">
      <input
        type="checkbox"
        checked={autoCheck}
        disabled={busy}
        onChange={e => void run(() => onAutoCheckChange(e.target.checked))}
      />
      Check installed mods for updates when the application starts
    </label>
    <small>
      Off by default, and saved as soon as you set it. Only mods matched to a
      Nexus page are checked, the result stands for several hours before another
      check is made, and the Mods page can always check on demand instead.
    </small>

    <div className="settings-actions">
      <button onClick={() => onOpenLink(API_KEY_PAGE)}><ExternalLink aria-hidden size={16} />Get an API key</button>
      {status?.hasKey && <button disabled={busy} onClick={() => void run(onClearKey)}>Remove stored key</button>}
    </div>
  </article>;
}
