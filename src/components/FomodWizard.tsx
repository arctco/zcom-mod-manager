import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, ArrowLeft, ArrowRight, Info, ShieldCheck, X } from "lucide-react";
import type { FomodAnswer, FomodGroup, FomodPlugin, FomodSession } from "../types";

interface Props {
  session: FomodSession;
  /** The answer this step was given last time, when stepping back to it. */
  restored: FomodAnswer | null;
  busy: boolean;
  canGoBack: boolean;
  onNext: (answer: FomodAnswer) => void;
  onBack: () => void;
  onCancel: () => void;
}

/** How many options a group accepts, written for the person choosing. */
function rule(group: FomodGroup): string | null {
  switch (group.kind) {
    case "SelectExactlyOne": return "Choose one";
    case "SelectAtMostOne": return "Choose one, or none";
    case "SelectAtLeastOne": return "Choose at least one";
    case "SelectAll": return "All of these are installed";
    default: return null;
  }
}

function unmet(group: FomodGroup, chosen: string[]): string | null {
  const count = group.plugins.filter(plugin => chosen.includes(plugin.id)).length;
  if (group.kind === "SelectExactlyOne" && count !== 1) return `${group.name} needs exactly one option.`;
  if (group.kind === "SelectAtLeastOne" && count === 0) return `${group.name} needs at least one option.`;
  return null;
}

/** The options the author's own answer starts a step on. */
function defaults(groups: FomodGroup[]): string[] {
  return groups.flatMap(group => group.plugins.filter(plugin => plugin.selected).map(plugin => plugin.id));
}

/**
 * A locked option is one the person cannot change: the script either requires
 * it outright, or has ruled it out because of an earlier answer.
 */
function locked(plugin: FomodPlugin, group: FomodGroup): boolean {
  return plugin.kind === "Required" || plugin.kind === "NotUsable" || group.kind === "SelectAll";
}

export function FomodWizard({ session, restored, busy, canGoBack, onNext, onBack, onCancel }: Props) {
  const step = session.step;
  const groups = useMemo(() => step?.groups ?? [], [step]);
  const [chosen, setChosen] = useState<string[]>(() => defaults(groups));
  const [focused, setFocused] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  // Each step starts from the author's recommendation, except when this is a
  // step being returned to, where the person's own earlier answer is what they
  // expect to find waiting.
  useEffect(() => {
    const restore = restored && step && restored.step === step.index ? restored.plugins : null;
    const start = restore
      // A required option stays required, and one an earlier answer has since
      // ruled out cannot come back just because it was picked before.
      ? groups.flatMap(group => group.plugins
        .filter(plugin => plugin.kind !== "NotUsable" && (plugin.kind === "Required" || group.kind === "SelectAll" || restore.includes(plugin.id)))
        .map(plugin => plugin.id))
      : defaults(groups);
    setChosen(start);
    setFocused(null);
    setProblem(null);
  }, [step?.index, groups, restored]);

  if (!step) return null;

  function toggle(group: FomodGroup, plugin: FomodPlugin, on: boolean) {
    setProblem(null);
    setFocused(plugin.id);
    setChosen(current => {
      const siblings = group.plugins.map(item => item.id);
      const single = group.kind === "SelectExactlyOne" || group.kind === "SelectAtMostOne";
      const kept = single ? current.filter(id => !siblings.includes(id)) : current.filter(id => id !== plugin.id);
      return on ? [...kept, plugin.id] : kept;
    });
  }

  function next() {
    const failed = groups.map(group => unmet(group, chosen)).find(Boolean);
    if (failed) { setProblem(failed); return; }
    onNext({ step: step!.index, plugins: chosen });
  }

  const detail = groups.flatMap(group => group.plugins).find(plugin => plugin.id === focused)
    ?? groups.flatMap(group => group.plugins).find(plugin => chosen.includes(plugin.id))
    ?? groups[0]?.plugins[0]
    ?? null;
  const last = session.position >= session.total;

  return <section className="fomod" aria-label={`${session.moduleName} installer`}>
    <header className="fomod-header">
      {session.moduleImage && <img className="fomod-banner" src={session.moduleImage} alt="" />}
      <div>
        <p className="eyebrow">GUIDED INSTALLER</p>
        <h2>{session.moduleName}</h2>
        <p className="muted">{session.author ? `by ${session.author}` : ""}{session.author && session.version ? " · " : ""}{session.version ? `Version ${session.version}` : ""}</p>
      </div>
      <div className="fomod-progress" role="status">Step {session.position} of {session.total}</div>
    </header>
    {session.warnings.map(warning => <div className="inline-warning" key={warning}><AlertTriangle aria-hidden size={17} />{warning}</div>)}
    <h3 className="fomod-step-name">{step.name}</h3>
    <div className="fomod-body">
      <div className="fomod-groups">
        {groups.map((group, groupIndex) => <fieldset className="fomod-group" key={`${step.index}-${groupIndex}`}>
          <legend>{group.name}{rule(group) && <span> · {rule(group)}</span>}</legend>
          {group.plugins.map(plugin => {
            const single = group.kind === "SelectExactlyOne" || group.kind === "SelectAtMostOne";
            const on = chosen.includes(plugin.id);
            return <label
              key={plugin.id}
              className={`fomod-option ${on ? "on" : ""} ${plugin.kind === "NotUsable" ? "unusable" : ""} ${detail?.id === plugin.id ? "focused" : ""}`}
              onMouseEnter={() => setFocused(plugin.id)}
            >
              <input
                type={single ? "radio" : "checkbox"}
                name={single ? `${step.index}-${groupIndex}` : undefined}
                checked={on}
                disabled={locked(plugin, group) || busy}
                onChange={event => toggle(group, plugin, event.target.checked)}
                // A one-of group offering "none" is answered by unticking the
                // option already chosen, which a radio cannot report on its own.
                onClick={() => { if (group.kind === "SelectAtMostOne" && on) toggle(group, plugin, false); }}
              />
              <span className="fomod-option-name">{plugin.name}</span>
              {plugin.kind === "Recommended" && <span className="fomod-tag">Recommended</span>}
              {plugin.kind === "Required" && <span className="fomod-tag required">Required</span>}
              {plugin.kind === "NotUsable" && <span className="fomod-tag unusable">Unavailable</span>}
            </label>;
          })}
        </fieldset>)}
      </div>
      <aside className="fomod-detail" aria-live="polite">
        {detail?.image && <img src={detail.image} alt="" />}
        <h4>{detail?.name ?? "No options"}</h4>
        <p>{detail?.description ?? "This option has no description."}</p>
        {detail?.kind === "NotUsable" && <div className="inline-warning"><AlertTriangle aria-hidden size={16} />An earlier answer rules this option out.</div>}
      </aside>
    </div>
    {problem && <div className="inline-warning" role="alert"><AlertTriangle aria-hidden size={17} />{problem}</div>}
    <div className="inline-note"><b><Info aria-hidden size={16} />No changes are applied yet</b><span>The installer only decides which of the archive's files are used. They are validated and shown for confirmation before anything is deployed or replaced.</span></div>
    <footer className="dialog-actions">
      <button onClick={onCancel} disabled={busy}><X size={17} />Cancel</button>
      <button onClick={onBack} disabled={!canGoBack || busy}><ArrowLeft size={17} />Back</button>
      <button className="primary" onClick={next} disabled={busy}>
        {last ? <ShieldCheck size={17} /> : <ArrowRight size={17} />}
        {busy ? "Working…" : last ? "Finish and review" : "Next"}
      </button>
    </footer>
  </section>;
}
