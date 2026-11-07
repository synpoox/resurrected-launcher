// PROTOTYPE for #19: settings and troubleshooting inside shell D, in four layouts.
// Open /settings.html?variant=A|B|C|D; ← → cycle layouts. The bar's toggles force edge cases
// (game running, plugin folder can't be created). Spec: #5 Q12/Q15, #8 tray + diagnostics, #16 plugin folders.
// Paths and versions are illustrative. Throwaway: never merge into main.
import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './tokens.css';
import './styles.css';
import './journey.css';
import './settings.css';
import { PRODUCTS, UMBRELLA } from './data.js';

const [PD2R] = PRODUCTS;
const INSTALLS = [
  { path: 'C:\\Program Files (x86)\\Diablo II Resurrected', store: 'Battle.net', build: '3.3.0', pd2r: true },
  { path: 'D:\\SteamLibrary\\steamapps\\common\\Diablo II Resurrected', store: 'Steam', build: '3.3.0', pd2r: false },
];
const SAVES = 'C:\\Users\\Player\\Saved Games\\Diablo II Resurrected\\mods\\PD2R';
const NOT_INSTALLED = PRODUCTS.filter((p) => !p.playable).map((p) => p.short).join(' and ');

const VARIANTS = [
  { key: 'A', name: 'One page, grouped by task' },
  { key: 'B', name: 'Sub-tabs inside Settings' },
  { key: 'C', name: 'Drawer over the product page' },
  { key: 'D', name: 'Cards grouped by what they belong to' },
];

// ---------- one set of controls, laid out four ways ----------
function useControls({ running, failFolder }) {
  const [game, setGame] = useState(INSTALLS[0]);
  const [startup, setStartup] = useState(false); // #8: optional, off by default
  const [dialog, setDialog] = useState(null);
  const [toast, setToast] = useState(null);
  const [recheck, setRecheck] = useState(null);
  const [created, setCreated] = useState({}); // plugin folders are missing until first opened

  useEffect(() => { if (toast && toast.tone !== 'error') { const t = setTimeout(() => setToast(null), 4000); return () => clearTimeout(t); } }, [toast]);

  const open = (path, plugin) => {
    if (plugin && failFolder) return setToast({ tone: 'error', text: <>Couldn't create <code>{path}</code>: access denied. Create the folder yourself in File Explorer, or check the D2R location. The launcher won't ask for administrator rights.</> });
    if (plugin && !created[path]) { setCreated({ ...created, [path]: true }); return setToast({ text: <>Created the empty folder and opened it in File Explorer: <code>{path}</code></> }); }
    setToast({ text: <>Opened in File Explorer: <code>{path}</code></> });
  };
  const pick = (g) => { setGame(g); setDialog(null); setRecheck(g); };

  const f = {
    globalPlugins: game.path + '\\d2rloader\\plugins',
    pd2rPlugins: game.path + '\\mods\\PD2R\\d2rloader\\plugins',
    config: game.path + '\\mods\\PD2R\\config',
  };

  const c = {
    location: (
      <Row label="Diablo II: Resurrected location" hint={<><code>{game.path}</code><br />{game.store} · build {game.build} · supported</>}>
        <button className="btn ghost small" disabled={running} onClick={() => setDialog('location')}>Change…</button>
        {running && <span className="why">Quit the game to change this</span>}
      </Row>
    ),
    recheck: recheck && (
      <div className="notice">
        <b>Rechecked {recheck.store} install.</b> D2R {recheck.build} is supported.{' '}
        {recheck.pd2r ? 'PD2R alpha 0.4.2 was found there and is ready to play.' : 'PD2R isn\'t installed at this location, so its page now offers Install.'}{' '}
        Nothing was moved, copied or deleted.
      </div>
    ),
    startup: (
      <Row label="Start with Windows" hint="Opens in the tray when you sign in to Windows. Closing the window always keeps the launcher in the tray; use Quit in the tray menu to exit.">
        <Switch on={startup} set={setStartup} label="Start with Windows" />
      </Row>
    ),
    saves: <Row label="PD2R saves" hint={<code>{SAVES}</code>}><button className="btn ghost small" onClick={() => open(SAVES)}>Open folder</button></Row>,
    config: game.pd2r && <Row label="PD2R settings and filters" hint={<code>{f.config}</code>}><button className="btn ghost small" onClick={() => open(f.config)}>Open folder</button></Row>,
    globalPlugins: (
      <Row label="Plugins for every product" hint={<>Loads for every product. <code>{f.globalPlugins}</code></>}>
        <button className="btn ghost small" onClick={() => open(f.globalPlugins, true)}>Open plugins folder</button>
      </Row>
    ),
    pd2rPlugins: game.pd2r && (
      <Row label="PD2R plugins" hint={<>Loads only with PD2R. Its <code>core-*</code> plugins are part of the mod. <code>{f.pd2rPlugins}</code></>}>
        <button className="btn ghost small" onClick={() => open(f.pd2rPlugins, true)}>Open plugins folder</button>
      </Row>
    ),
    pluginNote: <p className="fine">The launcher doesn't list, enable or disable plugins. Plugins are allowed offline and in direct TCP/IP co-op only. {game.pd2r ? NOT_INSTALLED : 'PD2R, ' + NOT_INSTALLED} aren't installed{game.pd2r ? '' : ' at this location'}, so they have no folder buttons.</p>,
    diagnostics: (
      <Row label="Diagnostics" hint="A report of launcher logs and your setup, for bug reports. Nothing is sent: you review it, then save it as a file to share yourself.">
        <button className="btn ghost small" onClick={() => setDialog('diag')}>Export diagnostics…</button>
      </Row>
    ),
    overlay: (
      <>
        {dialog === 'location' && <LocationDialog current={game} onPick={pick} onCancel={() => setDialog(null)} />}
        {dialog === 'diag' && <DiagnosticsDialog game={game} onDone={(t) => { setDialog(null); if (t) setToast(t); }} />}
        {toast && <div className={'toast' + (toast.tone ? ' ' + toast.tone : '')} role="status"><span>{toast.text}</span><button className="link" onClick={() => setToast(null)}>Dismiss</button></div>}
      </>
    ),
  };
  return c;
}

function Row({ label, hint, children }) {
  return (
    <div className="srow">
      <div><b>{label}</b>{hint && <span className="hint">{hint}</span>}</div>
      <div className="ctl">{children}</div>
    </div>
  );
}

function Switch({ on, set, label }) {
  return <button className="switch" role="switch" aria-checked={on} aria-label={label} onClick={() => set(!on)}><span /></button>;
}

function Dialog({ title, tone, children }) {
  return (
    <div className="backdrop">
      <div className={'dialog' + (tone ? ' ' + tone : '')} role="dialog" aria-modal="true" aria-label={title}><h2>{title}</h2>{children}</div>
    </div>
  );
}

function LocationDialog({ current, onPick, onCancel }) {
  const [sel, setSel] = useState(INSTALLS.indexOf(current));
  const [err, setErr] = useState(false);
  return (
    <Dialog title="Change D2R location">
      <p>Pick the installation the launcher should use. Changing this <b>doesn't move, copy or delete anything</b>: the launcher checks D2R and looks for PD2R again at the new location.</p>
      <div className="choices">
        {INSTALLS.map((g, i) => (
          <label key={g.path}><input type="radio" name="loc" checked={sel === i} onChange={() => { setSel(i); setErr(false); }} />
            <span><b>{g.path}</b><em>{g.store} · build {g.build} · supported · {g.pd2r ? 'PD2R installed here' : 'no PD2R here'}{g === current ? ' · current' : ''}</em></span></label>
        ))}
      </div>
      <button className="link" onClick={() => setErr(true)}>Browse for another folder…</button>
      {err && <div className="notice error">That folder (<code>C:\Games</code>) doesn't contain <code>D2R.exe</code>. Choose the folder D2R is installed in.</div>}
      <div className="row"><button className="btn primary" onClick={() => onPick(INSTALLS[sel])}>Use this location</button><button className="btn ghost" onClick={onCancel}>Cancel</button></div>
    </Dialog>
  );
}

function DiagnosticsDialog({ game, onDone }) {
  const [logs, setLogs] = useState(true);
  const report = [
    'Project Resurrected launcher 0.1.0 · Windows 11 23H2 (x64)',
    `D2R: ${game.path.replace('Player', '<you>')} · ${game.store} · build ${game.build}`,
    'D2RLoader: 0.1.0 (official installer)',
    'PD2R: alpha 0.4.2 · verified 2026-11-06 · plugins folder: 2 files',
    'Forever, 09: not installed',
    'Last error: game exited while starting (exit code 1) · 2026-11-07 09:41',
    ...(logs ? ['', '--- launcher log (last 7 days, 412 lines) ---', '09:41:02 launch pd2r alpha-0.4.2', '09:41:05 game exited code=1 after 3.1s', `09:41:05 saves untouched: ${SAVES.replace('Player', '<you>')}`, '…'] : []),
  ].join('\n');
  return (
    <Dialog title="Review diagnostics before saving">
      <p>This is everything in the report. It stays on your PC until you share the file yourself.</p>
      <pre className="report" tabIndex={0}>{report}</pre>
      <p className="fine">Never included: passwords, account sessions, save files or plugin files. Your Windows user name is replaced with <code>&lt;you&gt;</code>.</p>
      <label className="ack"><input type="checkbox" checked={logs} onChange={(e) => setLogs(e.target.checked)} /> Include launcher log (last 7 days)</label>
      <div className="row">
        <button className="btn primary" onClick={() => onDone({ text: <>Saved <code>resurrected-diagnostics-2026-11-07.zip</code> to Downloads. <button className="link">Show in folder</button></> })}>Save as file…</button>
        <button className="btn ghost" onClick={() => onDone()}>Cancel</button>
      </div>
    </Dialog>
  );
}

// ---------- layouts ----------
function Section({ title, children }) {
  return <section className="ssec"><h3 className="label">{title}</h3>{children}</section>;
}

function VariantA({ c }) {
  return (
    <div className="S A1 scroll">
      <h1 className="h1">Settings</h1>
      <Section title="Game">{c.location}{c.recheck}</Section>
      <Section title="Launcher">{c.startup}</Section>
      <Section title="Folders">{c.saves}{c.config}{c.globalPlugins}{c.pd2rPlugins}{c.pluginNote}</Section>
      <Section title="Troubleshooting">{c.diagnostics}</Section>
    </div>
  );
}

const TABS = [['game', 'Game'], ['launcher', 'Launcher'], ['folders', 'Folders and plugins'], ['trouble', 'Troubleshooting']];
function VariantB({ c }) {
  const [t, setT] = useState('game');
  return (
    <div className="S B1">
      <div className="subnav" role="tablist" aria-label="Settings sections">
        <h1 className="h1">Settings</h1>
        {TABS.map(([k, n]) => <button key={k} role="tab" aria-selected={t === k} onClick={() => setT(k)}>{n}</button>)}
      </div>
      <div className="panel scroll" role="tabpanel">
        {t === 'game' && <Section title="Game">{c.location}{c.recheck}</Section>}
        {t === 'launcher' && <Section title="Launcher">{c.startup}</Section>}
        {t === 'folders' && <><Section title="PD2R">{c.saves}{c.config}{c.pd2rPlugins}</Section><Section title="All products">{c.globalPlugins}</Section>{c.pluginNote}</>}
        {t === 'trouble' && <Section title="Troubleshooting">{c.diagnostics}<p className="fine">For game problems, Repair is in PD2R's ⋯ menu next to Play.</p></Section>}
      </div>
    </div>
  );
}

function VariantC({ c, open, close }) {
  return (
    <>
      <div className="art" role="img" aria-label={PD2R.credit} style={{ backgroundImage: `url("${PD2R.art}")`, backgroundPosition: PD2R.artPos }} />
      <div className="shade" />
      <div className="bottom"><div>
        <span className="ribbon">{PD2R.tag}</span>
        <h1 className="h1">{PD2R.headline[0]}<br />{PD2R.headline[1]}</h1>
        <p>{PD2R.blurb}</p>
        <div className="act"><button className="btn primary">Play</button></div>
      </div></div>
      {open && <><div className="dim" onClick={close} />
      <aside className="S drawer scroll" aria-label="Settings">
        <div className="dhead"><h2>Settings</h2><button className="btn ghost small" onClick={close}>Close</button></div>
        <Section title="Game">{c.location}{c.recheck}</Section>
        <Section title="Launcher">{c.startup}</Section>
        <Section title="Folders">{c.saves}{c.config}{c.pd2rPlugins}{c.globalPlugins}{c.pluginNote}</Section>
        <Section title="Troubleshooting">{c.diagnostics}</Section>
      </aside></>}
    </>
  );
}

function VariantD({ c }) {
  return (
    <div className="S D1 scroll">
      <h1 className="h1">Settings</h1>
      <div className="cards">
        <section className="card"><h3>Diablo II: Resurrected</h3>{c.location}{c.recheck}{c.globalPlugins}</section>
        <section className="card"><h3>Project Diablo 2: Resurrected</h3>{c.saves}{c.config}{c.pd2rPlugins}</section>
        <section className="card"><h3>Launcher</h3>{c.startup}</section>
        <section className="card"><h3>Troubleshooting</h3>{c.diagnostics}</section>
      </div>
      {c.pluginNote}
    </div>
  );
}

// ---------- frame (same as journey.html) ----------
const SIZES = { 1280: [1280, 720], 1024: [1024, 640] };
const param = (k, d) => new URLSearchParams(location.search).get(k) ?? d;
const setParam = (k, v) => { const u = new URL(location.href); u.searchParams.set(k, v); history.replaceState(null, '', u); };

function App() {
  const [vi, setVi] = useState(Math.max(0, VARIANTS.findIndex((v) => v.key === param('variant', 'A'))));
  const [size, setSize] = useState(SIZES[param('size', '1280')] ? param('size', '1280') : '1280');
  const [running, setRunning] = useState(false);
  const [failFolder, setFailFolder] = useState(false);
  const [drawer, setDrawer] = useState(true);
  const [vw, setVw] = useState([innerWidth, innerHeight]);
  const c = useControls({ running, failFolder });
  const v = VARIANTS[vi];
  const [w, h] = SIZES[size];
  const scale = Math.min(1, (vw[0] - 24) / w, (vw[1] - 110) / h);
  const cycle = (d) => setVi((i) => { const n = (i + d + VARIANTS.length) % VARIANTS.length; setParam('variant', VARIANTS[n].key); return n; });

  useEffect(() => {
    const onKey = (e) => {
      if (e.target.closest('input,textarea,[contenteditable],[role=dialog]')) return;
      if (e.key === 'ArrowLeft') cycle(-1);
      if (e.key === 'ArrowRight') cycle(1);
    };
    const onResize = () => setVw([innerWidth, innerHeight]);
    addEventListener('keydown', onKey); addEventListener('resize', onResize);
    return () => { removeEventListener('keydown', onKey); removeEventListener('resize', onResize); };
  }, []);

  const isC = v.key === 'C';
  const settingsOn = !isC || drawer;
  return (
    <>
      <div className="stage">
        <div className="window seamless" style={{ width: w, height: h, position: 'absolute', left: '50%', top: 'calc(50% - 44px)', transform: `translate(-50%,-50%) scale(${scale})` }}>
          <header className="titlebar">
            <div className="drag" />
            <button className="win" aria-label="Minimise">&#8211;</button>
            <button className="win" aria-label="Maximise">&#9633;</button>
            <button className="win close" aria-label="Close to tray">&#10005;</button>
          </header>
          <div className="D J">
            <nav aria-label="Products">
              <div className="brand"><span className="glyph" aria-hidden>/</span><span className="name">{UMBRELLA}</span></div>
              {PRODUCTS.map((x) => (
                <button key={x.id} className="item" aria-current={(isC ? x.id === 'pd2r' : false) || undefined}>
                  {x.name}{x.id === 'pd2r' && running && <span className="badge">●</span>}
                </button>
              ))}
              <button className="item foot" aria-current={settingsOn || undefined} onClick={() => isC && setDrawer(true)}>Settings</button>
            </nav>
            <main>
              {v.key === 'A' && <VariantA c={c} />}
              {v.key === 'B' && <VariantB c={c} />}
              {v.key === 'D' && <VariantD c={c} />}
              {isC && <VariantC c={c} open={drawer} close={() => setDrawer(false)} />}
              {c.overlay}
            </main>
          </div>
        </div>
      </div>
      {import.meta.env.DEV && (
        <div style={bar}>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center', whiteSpace: 'nowrap' }}>
            <button style={pill} onClick={() => cycle(-1)} aria-label="Previous layout">←</button>
            <span style={{ minWidth: 260, textAlign: 'center' }}>{v.key}: {v.name}</span>
            <button style={pill} onClick={() => cycle(1)} aria-label="Next layout">→</button>
            <button style={pill} onClick={() => { const n = size === '1280' ? '1024' : '1280'; setSize(n); setParam('size', n); }}>{w}×{h}</button>
            <label><input type="checkbox" checked={running} onChange={(e) => setRunning(e.target.checked)} /> game running</label>
            <label><input type="checkbox" checked={failFolder} onChange={(e) => setFailFolder(e.target.checked)} /> plugin folder fails</label>
          </div>
          <div style={{ fontWeight: 400, fontSize: 12 }}>Click Change…, Open folder and Export diagnostics… to try the flows.</div>
        </div>
      )}
    </>
  );
}

// Deliberately off-palette so nobody mistakes it for the design.
const bar = { position: 'fixed', bottom: 10, left: '50%', transform: 'translateX(-50%)', display: 'grid', justifyItems: 'center', gap: 4, background: '#ffe14d', color: '#111', font: '600 13px system-ui', padding: '6px 14px', borderRadius: 18, boxShadow: '0 4px 16px #0008', zIndex: 99 };
const pill = { background: '#111', color: '#ffe14d', borderRadius: 999, padding: '4px 12px', font: 'inherit' };

createRoot(document.getElementById('root')).render(<App />);
