// PROTOTYPE for #28: where the launcher tells the player about its OWN update (Tauri signed updater, ADR 0003),
// inside shell E, without competing with PD2R's journey state. Open /selfupdate.html?variant=A|B|C; ← → cycle.
// The bar drives the self-update phase, what PD2R is doing at the same time, and Settings → Launcher controls.
// Shared rule in every variant: the update downloads in the background and installs on the next restart or Quit;
// Restart is held while PD2R runs or installs. Versions are illustrative. Throwaway: never merge into main.
import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './tokens.css';
import './styles.css';
import './journey.css';
import './settings.css';
import './selfupdate.css';
import { PRODUCTS, UMBRELLA } from './data.js';

const [PD2R] = PRODUCTS;
const CUR = '1.2.0', NEXT = '1.3.0';

const VARIANTS = [
  { key: 'A', name: 'Sidebar block above Settings' },
  { key: 'B', name: 'Titlebar chip with popover' },
  { key: 'C', name: 'Toast once, then a dot on Settings' },
];
const PHASES = [
  ['none', 'Up to date'],
  ['downloading', 'Downloading in background'],
  ['ready', 'Ready to restart'],
  ['failed-net', 'Failed: download'],
  ['failed-sig', 'Failed: signature check'],
  ['restarting', 'Restarting'],
  ['updated', 'Just updated (after restart)'],
];
const PRODUCT = [['ready', 'PD2R ready'], ['installing', 'PD2R downloading'], ['running', 'PD2R running']];
const SETTINGS = [['version', 'Version line only'], ['check', 'Version + Check for updates'], ['auto', 'Check + auto-download switch']];

// One source of copy for the self-update, rendered three ways.
function useUpdate(phase, product, setPhase) {
  const held = product === 'running' ? 'Restarts after you quit PD2R' : product === 'installing' ? 'Restarts after PD2R finishes downloading' : null;
  const restart = <button className="btn primary small" disabled={!!held} onClick={() => { setPhase('restarting'); setTimeout(() => setPhase('updated'), 1600); }}>Restart to update</button>;
  const retry = <button className="btn ghost small" onClick={() => { setPhase('downloading'); setTimeout(() => setPhase('ready'), 1500); }}>Try again</button>;
  return {
    none: null,
    downloading: { title: `Launcher ${NEXT}`, text: 'Downloading in the background. You can keep playing.', pct: 38 },
    ready: { title: `Launcher ${NEXT} is ready`, text: held ? `${held}, or it installs the next time you Quit.` : 'Restart takes a few seconds. Otherwise it installs the next time you Quit.', action: restart, notes: true },
    'failed-net': { tone: 'error', title: "Couldn't download the launcher update", text: `Check your connection. Launcher ${CUR} keeps working.`, action: retry },
    'failed-sig': { tone: 'error', title: 'Launcher update rejected', text: `${NEXT} didn't pass its signature check, so it was deleted and nothing was installed. Launcher ${CUR} keeps working.`, action: <><button className="btn ghost small">Export diagnostics</button></> },
    restarting: null,
    updated: { title: `Updated to launcher ${NEXT}`, text: 'Your settings and products are unchanged.', notes: true },
  }[phase];
}

const Notes = () => <button className="link">What's new</button>;

// A: block pinned in the sidebar, above Settings. Never enters the hero.
function SideBlock({ u }) {
  if (!u) return null;
  return (
    <div className={'sublock' + (u.tone ? ' ' + u.tone : '')} role="status">
      <b>{u.title}</b>
      <span>{u.text}</span>
      {u.pct != null && <div className="track"><div className="fill" style={{ width: u.pct + '%' }} /></div>}
      {(u.action || u.notes) && <div className="row">{u.action}{u.notes && <Notes />}</div>}
    </div>
  );
}

// B: chip in the titlebar; details in a popover.
function Chip({ u, phase }) {
  const [open, setOpen] = useState(false);
  useEffect(() => setOpen(phase.startsWith('failed')), [phase]);
  if (!u) return null;
  const label = phase === 'downloading' ? `↓ Launcher ${u.pct}%` : phase === 'ready' ? '↑ Restart to update' : phase === 'updated' ? `✓ ${NEXT}` : '! Launcher update';
  return (
    <span className="suchip-wrap">
      <button className={'suchip' + (u.tone ? ' ' + u.tone : '')} aria-expanded={open} onClick={() => setOpen(!open)}>{label}</button>
      {open && (
        <div className={'supop' + (u.tone ? ' ' + u.tone : '')} role="dialog" aria-label={u.title}>
          <b>{u.title}</b><span>{u.text}</span>
          {(u.action || u.notes) && <div className="row">{u.action}{u.notes && <Notes />}</div>}
        </div>
      )}
    </span>
  );
}

// C: the existing toast surface, shown once per phase; "Later" leaves a dot on Settings.
function Toast({ u, phase, onLater }) {
  const [gone, setGone] = useState(false);
  useEffect(() => setGone(false), [phase]);
  useEffect(() => { if (phase === 'updated') { const t = setTimeout(() => setGone(true), 4000); return () => clearTimeout(t); } }, [phase]);
  if (!u || gone || phase === 'downloading') return null; // downloading is silent in C
  return (
    <div className={'toast sutoast' + (u.tone ? ' error' : '')} role="status">
      <div><b>{u.title}</b><br />{u.text}{(u.action || u.notes) && <div className="row">{u.action}{u.notes && <Notes />}</div>}</div>
      {phase !== 'updated' && <button className="link" onClick={() => { setGone(true); onLater(); }}>Later</button>}
    </div>
  );
}

function SettingsLauncher({ u, phase, mode, setPhase }) {
  const [auto, setAuto] = useState(true);
  const [start, setStart] = useState(false);
  const [checking, setChecking] = useState(false);
  const status = { none: 'Up to date', downloading: `Downloading ${NEXT} · 38%`, ready: `${NEXT} ready · installs on restart`, 'failed-net': "Last check failed: couldn't download", 'failed-sig': `${NEXT} rejected: signature check failed`, updated: 'Up to date' }[phase] ?? '';
  const check = () => { setChecking(true); setTimeout(() => { setChecking(false); setPhase(phase === 'none' || phase === 'updated' ? 'downloading' : phase); }, 900); };
  return (
    <div className="S B1">
      <div className="subnav" role="tablist" aria-label="Settings sections">
        <h1 className="h1">Settings</h1>
        {['Game', 'Launcher', 'Folders and plugins', 'Troubleshooting'].map((n) => <button key={n} role="tab" aria-selected={n === 'Launcher'}>{n}</button>)}
      </div>
      <div className="panel scroll" role="tabpanel">
        <section className="ssec"><h3 className="label">Launcher</h3>
          <div className="srow">
            <span><b>Launcher version</b><span className="hint">{phase === 'updated' ? NEXT : CUR} · {checking ? 'Checking…' : status}{phase === 'updated' && <> · <Notes /></>}</span></span>
            <span className="ctl">
              {mode !== 'version' && (phase === 'ready' || phase.startsWith('failed') ? u.action : <button className="btn ghost small" disabled={checking || phase === 'downloading'} onClick={check}>Check for updates</button>)}
            </span>
          </div>
          {mode === 'auto' && (
            <div className="srow">
              <span><b>Download launcher updates automatically</b><span className="hint">{auto ? 'Updates download in the background and install when you restart or Quit.' : "You'll be told when an update exists. Nothing downloads until you choose Check for updates."}</span></span>
              <span className="ctl"><button className="switch" role="switch" aria-checked={auto} onClick={() => setAuto(!auto)}><span /></button></span>
            </div>
          )}
          <div className="srow">
            <span><b>Start with Windows</b><span className="hint">Opens to the tray</span></span>
            <span className="ctl"><button className="switch" role="switch" aria-checked={start} onClick={() => setStart(!start)}><span /></button></span>
          </div>
        </section>
        <p className="fine">Launcher updates are signed. Product updates (PD2R) stay on the product page.</p>
      </div>
    </div>
  );
}

function Act({ product }) {
  if (product === 'running') return <div className="act"><button className="btn primary running" disabled><span className="pulse" />Playing</button><span className="ready">Started 12 min ago · Repair and updates wait until you quit</span></div>;
  if (product === 'installing') return (
    <div className="act"><div className="progress">
      <div className="top"><b>Downloading alpha 0.4.2</b><span>46%</span></div>
      <div className="track"><div className="fill" style={{ width: '46%' }} /></div>
      <div className="bottomrow"><span>540 MB of 1.2 GB · about 1 min left</span><button className="btn ghost small">Cancel</button></div>
    </div></div>
  );
  return <div className="act"><button className="btn primary">Play</button><button className="btn ghost square" aria-label="Manage PD2R">⋯</button><span className="ready"><span className="dot" />Ready · alpha 0.4.2 (latest)</span></div>;
}

const SIZES = { 1280: [1280, 720], 1024: [1024, 640] };
const param = (k, d) => new URLSearchParams(location.search).get(k) ?? d;
const setParam = (k, v) => { const u = new URL(location.href); u.searchParams.set(k, v); history.replaceState(null, '', u); };
const pick = (list, k, d) => (list.some(([x]) => x === param(k, d)) ? param(k, d) : d);

function App() {
  const [vi, setVi] = useState(Math.max(0, VARIANTS.findIndex((v) => v.key === param('variant', 'A'))));
  const [phase, setPhaseRaw] = useState(pick(PHASES, 'phase', 'ready'));
  const [product, setProduct] = useState(pick(PRODUCT, 'product', 'ready'));
  const [mode, setMode] = useState(pick(SETTINGS, 'settings', 'check'));
  const [view, setView] = useState(param('view', 'product'));
  const [dot, setDot] = useState(false);
  const [size, setSize] = useState(SIZES[param('size', '1280')] ? param('size', '1280') : '1280');
  const [vw, setVw] = useState([innerWidth, innerHeight]);
  const setPhase = (p) => { setPhaseRaw(p); setParam('phase', p); setDot(false); };
  const v = VARIANTS[vi];
  const u = useUpdate(phase, product, setPhase);
  const [w, h] = SIZES[size];
  const scale = Math.min(1, (vw[0] - 24) / w, (vw[1] - 130) / h);
  const cycle = (d) => setVi((i) => { const n = (i + d + VARIANTS.length) % VARIANTS.length; setParam('variant', VARIANTS[n].key); return n; });
  const go = (k, val, set) => { set(val); setParam(k, val); };

  useEffect(() => {
    const onKey = (e) => {
      if (e.target.closest('input,select,textarea')) return;
      if (e.key === 'ArrowLeft') cycle(-1);
      if (e.key === 'ArrowRight') cycle(1);
    };
    const onResize = () => setVw([innerWidth, innerHeight]);
    addEventListener('keydown', onKey); addEventListener('resize', onResize);
    return () => { removeEventListener('keydown', onKey); removeEventListener('resize', onResize); };
  }, []);

  const settingsDot = v.key === 'C' && dot && view !== 'settings';
  return (
    <>
      <div className="stage">
        <div className="window seamless" style={{ width: w, height: h, position: 'absolute', left: '50%', top: 'calc(50% - 54px)', transform: `translate(-50%,-50%) scale(${scale})` }}>
          <header className="titlebar">
            <div className="drag" />
            {v.key === 'B' && <Chip u={u} phase={phase} />}
            <button className="win" aria-label="Minimise">&#8211;</button>
            <button className="win" aria-label="Maximise">&#9633;</button>
            <button className="win close" aria-label="Close to tray">&#10005;</button>
          </header>
          <div className="D J T mid">
            <nav aria-label="Products">
              <div className="brand"><span className="glyph" aria-hidden>/</span><span className="name">{UMBRELLA}</span></div>
              {PRODUCTS.map((x) => (
                <button key={x.id} className="item" aria-current={(view === 'product' && x.id === 'pd2r') || undefined} onClick={() => x.id === 'pd2r' && go('view', 'product', setView)}>
                  {x.name}{x.id === 'pd2r' && product !== 'ready' && <span className="badge">{product === 'running' ? '●' : '46%'}</span>}
                </button>
              ))}
              {v.key === 'A' && <SideBlock u={u} />}
              <button className={'item foot' + (v.key === 'A' && u ? ' after' : '')} aria-current={view === 'settings' || undefined} onClick={() => go('view', 'settings', setView)}>
                Settings{settingsDot && <span className="alert" aria-label=" (launcher update)" />}
              </button>
            </nav>
            <main>
              {view === 'settings' ? <SettingsLauncher u={u} phase={phase} mode={mode} setPhase={setPhase} /> : (
                <>
                  <div className="art" role="img" aria-label={PD2R.credit} style={{ backgroundImage: `url("${PD2R.art}")`, backgroundPosition: PD2R.artPos }} />
                  <div className="shade" />
                  <div className="copy">
                    <span className="ribbon">{PD2R.tag}</span>
                    <h1 className="h1">{PD2R.headline[0]}<br />{PD2R.headline[1]}</h1>
                    <p>{PD2R.blurb}</p>
                  </div>
                  <div className="bottom"><div><Act product={product} /></div><div /></div>
                </>
              )}
              {v.key === 'C' && <Toast u={u} phase={phase} onLater={() => setDot(true)} />}
              {phase === 'restarting' && <div className="surestart"><span className="glyph">/</span><b>Updating the launcher…</b><span>Back in a few seconds.</span></div>}
            </main>
          </div>
        </div>
      </div>
      {import.meta.env.DEV && (
        <div style={bar}>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center', whiteSpace: 'nowrap' }}>
            <button style={pill} onClick={() => cycle(-1)} aria-label="Previous variant">←</button>
            <span style={{ minWidth: 250, textAlign: 'center' }}>{v.key}: {v.name}</span>
            <button style={pill} onClick={() => cycle(1)} aria-label="Next variant">→</button>
            <button style={pill} onClick={() => { const n = size === '1280' ? '1024' : '1280'; setSize(n); setParam('size', n); }}>{w}×{h}</button>
            <button style={pill} onClick={() => go('view', view === 'settings' ? 'product' : 'settings', setView)}>{view === 'settings' ? 'Show PD2R' : 'Settings → Launcher'}</button>
          </div>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center', whiteSpace: 'nowrap' }}>
            <select style={sel} value={phase} onChange={(e) => setPhase(e.target.value)} aria-label="Self-update phase">{PHASES.map(([k, n]) => <option key={k} value={k}>Launcher: {n}</option>)}</select>
            <select style={sel} value={product} onChange={(e) => go('product', e.target.value, setProduct)} aria-label="PD2R state">{PRODUCT.map(([k, n]) => <option key={k} value={k}>{n}</option>)}</select>
            <select style={sel} value={mode} onChange={(e) => go('settings', e.target.value, setMode)} aria-label="Settings controls">{SETTINGS.map(([k, n]) => <option key={k} value={k}>Settings: {n}</option>)}</select>
          </div>
          <div style={{ fontWeight: 400, fontSize: 12 }}>State: launcher {phase} · PD2R {product} · Restart {phase !== 'ready' ? 'n/a' : product === 'ready' ? 'allowed' : 'held'} · settings {mode}{v.key === 'C' ? ` · Settings dot ${settingsDot ? 'on' : 'off'}` : ''}</div>
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
