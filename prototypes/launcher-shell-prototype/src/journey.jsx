// PROTOTYPE for #18: v1 journey states inside the approved shell (variant D from #17).
// Open /journey.html?state=<key>; ← → cycle states. Buttons jump to the state they would lead to.
// Spec sources: journeys #5, update contract #6, ADR 0001. Throwaway: never merge into main.
import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './tokens.css';
import './styles.css';
import './journey.css';
import { PRODUCTS, NEWS, UMBRELLA } from './data.js';

const [PD2R, FOREVER] = PRODUCTS;
const SAVES = 'C:\\Users\\Player\\Saved Games\\Diablo II Resurrected\\mods\\PD2R';

// Each state: product, action area (act), optional notice above it, dialog, sidebar badge, feed override.
const STATES = [
  {
    key: 'd2r-missing', name: 'D2R not found', spec: '#5 Q1: detect, allow manual browsing, remember one install. Never download or copy D2R.',
    act: (go) => <><button className="btn primary" onClick={() => go('d2r-choose')}>Locate D2R</button><span className="ready"><span className="dot warn" />Diablo II: Resurrected wasn't found on this PC</span></>,
    notice: <>Choose the folder that contains <code>D2R.exe</code>. The launcher won't move or copy any files.</>,
  },
  {
    key: 'd2r-choose', name: 'Choose D2R location', spec: '#5 Q1/Q15: several installs found; choosing a location does not migrate an installation.',
    dialog: (go) => (
      <Dialog title="Choose your D2R installation">
        <p>Two installations were found. The launcher remembers the one you pick, and you can change it later in Settings.</p>
        <div className="choices">
          <label><input type="radio" name="loc" defaultChecked /><span><b>C:\Program Files (x86)\Diablo II Resurrected</b><em>Battle.net · build 3.3.0 · supported</em></span></label>
          <label><input type="radio" name="loc" /><span><b>D:\SteamLibrary\steamapps\common\Diablo II Resurrected</b><em>Steam · build 3.3.0 · supported</em></span></label>
        </div>
        <button className="link">Browse for another folder…</button>
        <div className="row"><button className="btn primary" onClick={() => go('loader-missing')}>Use this installation</button><button className="btn ghost" onClick={() => go('d2r-missing')}>Cancel</button></div>
      </Dialog>
    ),
    act: () => <><button className="btn primary" disabled>Locate D2R</button></>,
  },
  {
    key: 'loader-missing', name: 'Loader not ready', spec: '#6 loader lifecycle: detect first; pinned official installer with consent; disclose unsigned; manual fallback + recheck.',
    act: (go) => <><button className="btn primary" onClick={() => go('install')}>Set up D2RLoader</button><button className="btn ghost">Manual steps</button></>,
    notice: <>PD2R needs <b>D2RLoader</b>. Setup downloads the official installer v0.1.0 from GitHub and runs it with your permission. That installer is <b>not code-signed</b>, so Windows may warn you.</>,
  },
  {
    key: 'install', name: 'Ready to install', spec: '#5 Q4: install with progress and actionable failures. Space checked before any change (#6).',
    act: (go) => <><button className="btn primary" onClick={() => go('downloading')}>Install</button><span className="ready"><span className="dot idle" />alpha 0.4.2 · 1.2 GB download · 3.6 GB free space needed</span></>,
  },
  {
    key: 'downloading', name: 'Install progress (cancellable)', spec: '#5 Q8: cancel during download/preparation. Never mark a partial install playable.',
    act: (go) => <Progress pct={46} label="Downloading alpha 0.4.2" detail="540 MB of 1.2 GB · 12 MB/s · about 1 min left" cancel={() => go('install')} cancelLabel="Cancel" />,
    badge: '46%',
  },
  {
    key: 'applying', name: 'Final step (safe-finish wait)', spec: '#6 step 5: during final application, cancelling waits for a safe finish or rollback.',
    act: (go) => <Progress pct={92} label="Installing files" detail="Cancelling now waits until it's safe to stop, or rolls back" cancel={() => go('cancelling')} cancelLabel="Cancel safely" />,
    badge: '92%',
  },
  {
    key: 'cancelling', name: 'Waiting for safe stop', spec: '#6 step 5: the cancel is acknowledged; the launcher finishes or rolls back before stopping.',
    act: () => <Progress pct={92} label="Stopping safely…" detail="Restoring the previous files. Don't close the launcher." cancelLabel="Cancelling…" busy />,
    badge: '…',
  },
  {
    key: 'ready', name: 'Ready (baseline)', spec: '#17 approved ready state.',
    act: () => <><button className="btn primary">Play</button><Manage /><span className="ready"><span className="dot" />Ready · alpha 0.4.2 (latest)</span></>,
  },
  {
    key: 'update', name: 'Update or play installed', spec: '#5 Q4: offer Update and Play installed version when it remains compatible.',
    act: (go) => <><button className="btn primary" onClick={() => go('downloading')}>Update</button><button className="btn ghost">Play 0.4.1</button><Manage /><span className="ready"><span className="dot new" />alpha 0.4.2 available</span></>,
    badge: '↑',
  },
  {
    key: 'save-breaking', name: 'Save-breaking warning', spec: 'ADR 0001 / #5 Q11: prominent warning + explicit acknowledgement; old saves may be rejected or changed by the new release.',
    act: () => <><button className="btn primary">Update</button><button className="btn ghost">Play 0.4.1</button><Manage /></>,
    dialog: (go) => <SaveBreaking go={go} />,
    badge: '↑',
  },
  {
    key: 'blocked', name: 'Blocked prerequisites', spec: '#5 Q7: block Play, explain why, give recovery steps, recheck. No silent patching or downgrade.',
    act: () => <><button className="btn primary" disabled>Play</button><button className="btn ghost">Check again</button><Manage /></>,
    notice: <><b>Can't play:</b> D2R build 3.3.1 isn't supported by alpha 0.4.2 yet (supported: 3.3.0). Wait for a PD2R update, or restore D2R 3.3.0. <button className="link">How to fix</button></>,
    tone: 'error', badge: '!',
  },
  {
    key: 'running', name: 'Game running', spec: '#5 Q6: launcher stays open, no duplicate launch, no changes while it runs.',
    act: () => <><button className="btn primary running" disabled><span className="pulse" />Playing</button><span className="ready">Started 12 min ago · Repair and updates wait until you quit</span></>,
    badge: '●',
  },
  {
    key: 'startup-failed', name: 'Startup failure', spec: '#5 Q6: startup failures explain what the player can do next.',
    act: () => <><button className="btn primary">Try again</button><button className="btn ghost">Repair</button><button className="btn ghost">Export diagnostics</button></>,
    notice: <><b>The game closed while starting</b> (exit code 1). Try again. If it keeps happening, Repair restores PD2R's files and keeps your saves and edited filters.</>,
    tone: 'error', badge: '!',
  },
  {
    key: 'repair', name: 'Repair', spec: '#5 Q9: restore release files; keep saves, edited filters and settings.',
    act: () => <Progress pct={63} label="Repairing" detail="Checked 1,478 of 2,341 files · 3 to restore · saves and edited filters are kept" cancelLabel="Cancel" cancel={() => {}} />,
    badge: '63%',
  },
  {
    key: 'manage', name: 'Manage menu', spec: 'Where Repair, Uninstall and folders live in shell D (no right column).',
    act: () => <><button className="btn primary">Play</button><Manage open /><span className="ready"><span className="dot" />Ready · alpha 0.4.2 (latest)</span></>,
  },
  {
    key: 'uninstall', name: 'Uninstall confirmation', spec: '#5 Q10: remove only managed files; keep saves/settings and show where; D2R, loader and other products stay.',
    act: () => <><button className="btn primary">Play</button><Manage /></>,
    dialog: (go) => (
      <Dialog title="Uninstall Project Diablo 2: Resurrected?" tone="error">
        <p>This removes PD2R's game files only.</p>
        <ul className="kept">
          <li><b>Kept:</b> your saves and settings in <code>{SAVES}</code> <button className="link">Open</button></li>
          <li><b>Not touched:</b> Diablo II: Resurrected, D2RLoader and other products</li>
        </ul>
        <div className="row"><button className="btn primary" onClick={() => go('install')}>Uninstall</button><button className="btn ghost" onClick={() => go('ready')}>Cancel</button></div>
      </Dialog>
    ),
  },
  {
    key: 'offline', name: 'Offline', spec: '#5 Q4 / #6: a failed check leaves a verified install playable; no account needed.',
    act: () => <><button className="btn primary">Play</button><Manage /><span className="ready"><span className="dot idle" />Offline · alpha 0.4.1 verified · can't check for updates</span></>,
    feed: <span className="m" style={{ padding: '12px 0', color: 'var(--muted)', fontSize: 12 }}>News needs a connection. <button className="link">Retry</button></span>,
  },
  {
    key: 'coming-soon', name: 'Coming soon product', spec: '#5 Q5: description + Coming soon; no install, date promise, signup or account.',
    product: FOREVER,
    act: () => <><button className="btn ghost">About Forever</button></>,
  },
];

function Dialog({ title, tone, children }) {
  return (
    <div className="backdrop">
      <div className={'dialog' + (tone ? ' ' + tone : '')} role="dialog" aria-modal="true" aria-label={title}>
        <h2>{title}</h2>
        {children}
      </div>
    </div>
  );
}

function SaveBreaking({ go }) {
  const [ok, setOk] = useState(false);
  return (
    <Dialog title="This update can't use your current characters safely" tone="error">
      <p><b>alpha 0.5.0 changes the save format.</b> Your characters stay in the same folder, and the launcher won't convert or delete them.</p>
      <p>The new version may refuse to load them, or change them the first time you play. Going back to 0.4.1 later restores the game files, but not your saves.</p>
      <p className="tip">To keep a copy, back up <code>{SAVES}</code> first. <button className="link">Open folder</button></p>
      <label className="ack"><input type="checkbox" checked={ok} onChange={(e) => setOk(e.target.checked)} /> I understand my current characters may not work after updating</label>
      <div className="row"><button className="btn primary" disabled={!ok} onClick={() => go('downloading')}>Update to 0.5.0</button><button className="btn ghost" onClick={() => go('update')}>Not now</button></div>
    </Dialog>
  );
}

function Progress({ pct, label, detail, cancel, cancelLabel, busy }) {
  return (
    <div className="progress">
      <div className="top"><b>{label}</b><span>{pct}%</span></div>
      <div className="track"><div className={'fill' + (busy ? ' busy' : '')} style={{ width: pct + '%' }} /></div>
      <div className="bottomrow"><span>{detail}</span><button className="btn ghost small" disabled={!cancel} onClick={cancel}>{cancelLabel}</button></div>
    </div>
  );
}

function Manage({ open }) {
  return (
    <span className="manage">
      <button className="btn ghost square" aria-label="Manage PD2R" aria-expanded={!!open}>⋯</button>
      {open && (
        <div className="menu" role="menu">
          <button role="menuitem">Repair</button>
          <button role="menuitem">Open saves folder</button>
          <button role="menuitem">Open settings folder</button>
          <hr />
          <button role="menuitem" className="danger">Uninstall…</button>
        </div>
      )}
    </span>
  );
}

const SIZES = { 1280: [1280, 720], 1024: [1024, 640] };
const param = (k, d) => new URLSearchParams(location.search).get(k) ?? d;
const setParam = (k, v) => { const u = new URL(location.href); u.searchParams.set(k, v); history.replaceState(null, '', u); };

function App() {
  const [si, setSi] = useState(Math.max(0, STATES.findIndex((s) => s.key === param('state', 'd2r-missing'))));
  const [size, setSize] = useState(SIZES[param('size', '1280')] ? param('size', '1280') : '1280');
  const [vw, setVw] = useState([innerWidth, innerHeight]);
  const s = STATES[si];
  const p = s.product ?? PD2R;
  const [w, h] = SIZES[size];
  const scale = Math.min(1, (vw[0] - 24) / w, (vw[1] - 110) / h);
  const goIdx = (n) => { setSi(n); setParam('state', STATES[n].key); };
  const go = (key) => goIdx(STATES.findIndex((x) => x.key === key));
  const cycle = (d) => setSi((i) => { const n = (i + d + STATES.length) % STATES.length; setParam('state', STATES[n].key); return n; });

  useEffect(() => {
    const onKey = (e) => {
      if (e.target.closest('input,textarea,[contenteditable]')) return;
      if (e.key === 'ArrowLeft') cycle(-1);
      if (e.key === 'ArrowRight') cycle(1);
    };
    const onResize = () => setVw([innerWidth, innerHeight]);
    addEventListener('keydown', onKey); addEventListener('resize', onResize);
    return () => { removeEventListener('keydown', onKey); removeEventListener('resize', onResize); };
  }, []);

  const news = NEWS[p.id];
  return (
    <>
      <div className="stage">
        <div className="window seamless" style={{ width: w, height: h, position: 'absolute', left: '50%', top: 'calc(50% - 44px)', transform: `translate(-50%,-50%) scale(${scale})` }}>
          <header className="titlebar">
            <div className="drag" />
            <button className="win" aria-label="Minimise">&#8211;</button>
            <button className="win" aria-label="Maximise">&#9633;</button>
            <button className="win close" aria-label="Close">&#10005;</button>
          </header>
          <div className="D J">
            <nav aria-label="Products">
              <div className="brand"><span className="glyph" aria-hidden>/</span><span className="name">{UMBRELLA}</span></div>
              {PRODUCTS.map((x) => (
                <button key={x.id} className="item" aria-current={x.id === p.id || undefined}>
                  {x.name}{x.id === 'pd2r' && s.badge && p.id === 'pd2r' && <span className={'badge' + (s.tone === 'error' ? ' error' : '')}>{s.badge}</span>}
                </button>
              ))}
              <button className="item foot">Settings</button>
            </nav>
            <main>
              <div key={p.id} className="art" role="img" aria-label={p.credit} style={{ backgroundImage: `url("${p.art}")`, backgroundPosition: p.artPos, backgroundSize: p.artSize }} />
              <div className="shade" />
              <div className="bottom">
                <div>
                  <span className={'ribbon' + (p.playable ? '' : ' soon')}>{p.tag}</span>
                  <h1 className="h1">{p.headline[0]}<br />{p.headline[1]}</h1>
                  <p>{p.blurb}</p>
                  {s.notice && <div className={'notice' + (s.tone ? ' ' + s.tone : '')}>{s.notice}</div>}
                  <div className="act">{s.act(go)}</div>
                </div>
                <div className="feed" aria-label="News">
                  <span className="label" style={{ padding: '10px 0' }}>News</span>
                  {s.feed ?? (news.length ? news.map((n) => (
                    <a key={n.title} href="#"><span className="m">{n.tag} · {n.date}</span><span className="t">{n.title} ↗</span></a>
                  )) : <span className="m" style={{ padding: '12px 0', color: 'var(--muted)', fontSize: 12 }}>Nothing yet.</span>)}
                </div>
              </div>
              {s.dialog && s.dialog(go)}
            </main>
          </div>
        </div>
      </div>
      {import.meta.env.DEV && (
        <div style={bar}>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            <button style={pill} onClick={() => cycle(-1)} aria-label="Previous state">←</button>
            <select style={sel} value={si} onChange={(e) => goIdx(+e.target.value)} aria-label="Journey state">
              {STATES.map((x, i) => <option key={x.key} value={i}>{i + 1}/{STATES.length} {x.name}</option>)}
            </select>
            <button style={pill} onClick={() => cycle(1)} aria-label="Next state">→</button>
            <button style={pill} onClick={() => { const n = size === '1280' ? '1024' : '1280'; setSize(n); setParam('size', n); }}>{w}×{h}</button>
          </div>
          <div style={{ fontWeight: 400, fontSize: 12, maxWidth: 640, textAlign: 'center' }}>{s.spec}</div>
        </div>
      )}
    </>
  );
}

// Deliberately off-palette so nobody mistakes it for the design.
const bar = { position: 'fixed', bottom: 10, left: '50%', transform: 'translateX(-50%)', display: 'grid', justifyItems: 'center', gap: 4, background: '#ffe14d', color: '#111', font: '600 13px system-ui', padding: '6px 14px', borderRadius: 18, boxShadow: '0 4px 16px #0008', zIndex: 99 };
const pill = { background: '#111', color: '#ffe14d', borderRadius: 999, padding: '4px 12px', font: 'inherit' };
const sel = { font: 'inherit', color: '#111', colorScheme: 'light', background: '#fff8c4', border: '1px solid #111', borderRadius: 6, padding: '2px 6px' };

createRoot(document.getElementById('root')).render(<App />);
