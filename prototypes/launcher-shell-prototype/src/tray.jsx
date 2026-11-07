// PROTOTYPE for #27: the tray icon and menu, three ways, on a simulated Windows 11 desktop.
// The launcher window is the approved shell itself (journey.html in an iframe, embed mode), so the tray is judged
// next to the real window. Open /tray.html?variant=A|B|C&tray=idle|update|running|progress|error; ← → cycle variants.
// The yellow bar drives what the prototype can't: tray state, Start with Windows sign-in, the game exiting or crashing.
// Spec: #8 tray required, #19 close keeps the launcher in the tray + Start with Windows opens to tray, #20 gap list.
// Copy and versions are illustrative. Throwaway: never merge into main.
import { useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './tokens.css';
import './styles.css';
import './journey.css';
import './tray.css';
import { UMBRELLA } from './data.js';

const VARIANTS = [
  { key: 'A', name: 'Plain menu', click: 'opens the window', right: 'Open · PD2R action · Quit' },
  { key: 'B', name: 'Status menu + notifications', click: 'shows or hides the window', right: 'status line, PD2R action and submenu, Settings, Quit' },
  { key: 'C', name: 'Launcher flyout', click: 'opens a launcher-styled flyout', right: 'Open · Quit only' },
];

const TRAY = {
  idle: { name: 'Idle (ready)', journey: 'ready' },
  update: { name: 'Update available', journey: 'update' },
  running: { name: 'Game running', journey: 'running' },
  progress: { name: 'Progress (downloading)', journey: 'downloading' },
  error: { name: 'Error (startup failed)', journey: 'startup-failed' },
};
const status = (t, pct) => ({
  idle: 'Ready · alpha 0.4.2 (latest)',
  update: 'alpha 0.4.2 available · 0.4.1 installed',
  running: 'Playing · started 12 min ago',
  progress: `Downloading alpha 0.4.2 · ${pct}%`,
  error: 'Closed while starting (exit code 1)',
})[t];

const param = (k, d) => new URLSearchParams(location.search).get(k) ?? d;
const setParam = (k, v) => { const u = new URL(location.href); u.searchParams.set(k, v); history.replaceState(null, '', u); };

// ---------- the icon: the 48px mark simplified for 16px, plus one overlay per state ----------
function TrayIcon({ t, pct, size = 16 }) {
  const sw = size <= 24 ? 4.5 : 2.5; // the mark's 2px stroke vanishes at 16px
  return (
    <svg width={size} height={size} viewBox="0 0 48 48" aria-hidden>
      <rect width="48" height="48" fill="#121213" />
      <path d="M24 3 43 14v20L24 45 5 34V14Z" fill="none" stroke="#f4f1ed" strokeWidth={sw} />
      <path d="m15 33 9-19 9 19M19 26h10" fill="none" stroke="#f4f1ed" strokeWidth={sw} />
      {t === 'update' && <><rect x="24" y="24" width="24" height="24" fill="#e3353d" /><path d="M36 43V30m-6 6 6-6 6 6" stroke="#fff" strokeWidth="4.5" fill="none" /></>}
      {t === 'running' && <circle cx="38" cy="38" r="9" fill="#f4f1ed" stroke="#121213" strokeWidth="4" />}
      {t === 'error' && <><rect x="24" y="24" width="24" height="24" fill="#c72d37" /><path d="M36 28v11m0 4v2" stroke="#fff" strokeWidth="5" /></>}
      {t === 'progress' && <><rect y="40" width="48" height="8" fill="#353233" /><rect y="40" width={48 * pct / 100} height="8" fill="#e3353d" /></>}
    </svg>
  );
}

// ---------- Windows 11 context menu (native, follows the taskbar theme) ----------
function NativeMenu({ items, x, light, close }) {
  const [sub, setSub] = useState(null);
  const left = Math.min(x - 120, innerWidth - 300);
  const list = (its, isSub) => its.map((it, i) => it === '-' ? <hr key={i} /> : (
    <div key={i} style={{ position: 'relative' }} onMouseEnter={() => !isSub && setSub(it.sub ? i : null)}>
      <button role="menuitem" className={'mi' + (it.bold ? ' bold' : '')} disabled={it.disabled}
        onClick={() => { if (it.sub) return setSub(i); it.run?.(); close(); }}>
        <span className="ck">{it.check ? '✓' : ''}</span>{it.label}{it.sub && <span className="arrow">›</span>}
      </button>
      {it.sub && sub === i && <div className={'native sub' + (light ? ' light' : '')}>{list(it.sub, true)}</div>}
    </div>
  ));
  return <div className={'native' + (light ? ' light' : '')} role="menu" style={{ left }}>{list(items)}</div>;
}

function Toast({ n, light, onOpen, onClose }) {
  useEffect(() => { const t = setTimeout(onClose, 7000); return () => clearTimeout(t); }, []);
  return (
    <div className={'wtoast' + (light ? ' light' : '')} role="status" onClick={onOpen}>
      <div className="app"><TrayIcon t="idle" size={16} /> {UMBRELLA}<button className="x" onClick={(e) => { e.stopPropagation(); onClose(); }} aria-label="Dismiss">✕</button></div>
      <b>{n.title}</b>
      <p>{n.body}</p>
      {n.action && <button className="wbtn" onClick={(e) => { e.stopPropagation(); n.action.run(); onClose(); }}>{n.action.label}</button>}
    </div>
  );
}

// ---------- variant C: flyout in launcher styling ----------
function Flyout({ t, pct, acts, open, settings, quit }) {
  return (
    <div className="flyout J" role="dialog" aria-label={UMBRELLA}>
      <div className="fhead"><span className="glyph" aria-hidden>/</span>{UMBRELLA}</div>
      <div className={'frow' + (t === 'error' ? ' error' : '')}>
        <b>Project Diablo 2: Resurrected</b>
        <span className="fstat">{t === 'running' && <span className="pulse" />}{status(t, pct)}</span>
        {t === 'progress' && <div className="track"><div className="fill" style={{ width: pct + '%' }} /></div>}
        <div className="fact">
          {acts.map((a, i) => <button key={a.label} className={'btn small ' + (i === 0 && !a.disabled ? 'primary' : 'ghost')} disabled={a.disabled} onClick={a.run}>{a.label}</button>)}
        </div>
      </div>
      <div className="ffoot">
        <button onClick={open}>Open launcher</button>
        <button onClick={settings}>Settings</button>
        <button onClick={quit}>Quit</button>
      </div>
    </div>
  );
}

function App() {
  const [vi, setVi] = useState(Math.max(0, VARIANTS.findIndex((v) => v.key === param('variant', 'A'))));
  const [t, setT0] = useState(TRAY[param('tray', '')] ? param('tray') : 'idle');
  const [pct, setPct] = useState(46);
  const [win, setWin] = useState('open'); // open | min | hidden | quit
  const [menu, setMenu] = useState(null); // { kind: 'native' | 'flyout', x }
  const [overflow, setOverflow] = useState(false);
  const [toasts, setToasts] = useState([]);
  const [quitAsk, setQuitAsk] = useState(false);
  const [hinted, setHinted] = useState(false);
  const [light, setLight] = useState(false);
  const [pinned, setPinned] = useState(param('pinned', '1') === '1');
  const [sheet, setSheet] = useState(false);
  const [tip, setTip] = useState(false);
  const [log, setLog] = useState('Launcher opened from the Start menu.');
  const [vw, setVw] = useState([innerWidth, innerHeight]);
  const tipTimer = useRef();
  const v = VARIANTS[vi];
  const setT = (n) => { setT0(n); setParam('tray', n); };
  const cycle = (d) => setVi((i) => { const n = (i + d + VARIANTS.length) % VARIANTS.length; setParam('variant', VARIANTS[n].key); return n; });
  const notify = (n) => setToasts((ts) => [...ts.slice(-1), { ...n, id: Math.random() }]);
  const away = win !== 'open';

  useEffect(() => {
    const onKey = (e) => {
      if (e.target.closest('input,textarea,select,[contenteditable]')) return;
      if (e.key === 'ArrowLeft') cycle(-1);
      if (e.key === 'ArrowRight') cycle(1);
      if (e.key === 'Escape') setMenu(null);
    };
    const onMsg = (e) => { if (e.data === 'close') closeWin(); if (e.data === 'minimise') { setWin('min'); setLog('Minimised to the taskbar.'); } };
    const onResize = () => setVw([innerWidth, innerHeight]);
    addEventListener('keydown', onKey); addEventListener('message', onMsg); addEventListener('resize', onResize);
    return () => { removeEventListener('keydown', onKey); removeEventListener('message', onMsg); removeEventListener('resize', onResize); };
  });

  // Downloads keep going in the tray; finishing returns to idle.
  useEffect(() => {
    if (t !== 'progress') return;
    if (pct >= 100) {
      setT('idle'); setPct(46);
      if (v.key === 'B' && away) notify({ title: 'PD2R alpha 0.4.2 is installed', body: 'Ready to play.', action: { label: 'Play', run: play } });
      return;
    }
    const id = setTimeout(() => setPct((p) => Math.min(100, p + 3)), 600);
    return () => clearTimeout(id);
  }, [t, pct]);

  const openWin = (why) => { setWin('open'); setMenu(null); setOverflow(false); setLog(why ?? 'Window opened.'); };
  const closeWin = () => {
    setWin('hidden');
    setLog('Closed the window: the launcher keeps running in the tray.');
    if (!hinted) { setHinted(true); notify({ title: 'Still running in the tray', body: 'Downloads and update checks carry on here. To exit, right-click the icon and choose Quit.' }); }
  };
  const play = () => { setT('running'); setLog('Started PD2R from the tray. The window stays where it was.'); };
  // A save-breaking release would open the window on its warning dialog instead (ADR 0001); not simulated here.
  const update = () => { setT('progress'); setPct(46); setLog('Update started from the tray.'); };
  const cancel = () => { setT('update'); setPct(46); setLog('Download cancelled. Nothing was installed.'); };
  const quit = () => {
    setMenu(null);
    if (t === 'progress') { openWin('Quit while downloading: the window opens to ask first.'); return setQuitAsk(true); }
    setWin('quit'); setToasts([]);
    setLog(t === 'running' ? 'Quit the launcher. The game keeps running.' : 'Quit the launcher. The tray icon is gone.');
  };
  const settings = () => openWin('Opened Settings (the window would switch to Settings).');

  const acts = {
    idle: [{ label: 'Play PD2R', run: play }],
    update: [{ label: 'Update PD2R', run: update }, { label: 'Play 0.4.1', run: play }],
    running: [{ label: 'PD2R is running', disabled: true }],
    progress: [{ label: `Downloading PD2R · ${pct}%`, disabled: true }, { label: 'Cancel download', run: cancel }],
    error: [{ label: 'Try again', run: play }, { label: 'Show what happened', run: () => openWin() }],
  }[t];

  const menus = {
    A: [{ label: `Open ${UMBRELLA}`, bold: true, run: () => openWin() }, '-', ...acts, '-', { label: 'Quit', run: quit }],
    B: [
      { label: 'PD2R · ' + status(t, pct), disabled: true }, '-',
      ...acts,
      { label: 'Project Diablo 2: Resurrected', sub: [
        { label: 'Repair', disabled: t === 'running' || t === 'progress', run: () => openWin('Repair runs in the window.') },
        { label: 'Open saves folder', run: () => setLog('Opened saves folder in File Explorer.') },
        { label: 'Open settings folder', run: () => setLog('Opened settings folder in File Explorer.') },
      ] },
      '-',
      { label: win === 'open' ? 'Hide window' : `Open ${UMBRELLA}`, bold: true, run: () => (win === 'open' ? closeWin() : openWin()) },
      { label: 'Settings', run: settings },
      '-', { label: 'Quit', run: quit },
    ],
    C: [{ label: `Open ${UMBRELLA}`, bold: true, run: () => openWin() }, '-', { label: 'Quit', run: quit }],
  }[v.key];

  const leftClick = (e) => {
    setTip(false);
    const x = e.currentTarget.getBoundingClientRect().left;
    if (v.key === 'A') return openWin('Left-click: window opened.');
    if (v.key === 'B') return win === 'open' ? closeWin() : openWin('Left-click: window shown.');
    setMenu(menu?.kind === 'flyout' ? null : { kind: 'flyout', x });
  };
  const rightClick = (e) => { e.preventDefault(); setTip(false); setMenu({ kind: 'native', x: e.currentTarget.getBoundingClientRect().left }); };

  // B notifies about changes that happen while the window is away; the bar's state picker stands in for them.
  const pickState = (n) => {
    setT(n); setPct(46); setLog(`Tray state: ${TRAY[n].name}.`);
    if (v.key !== 'B' || !away) return;
    if (n === 'update') notify({ title: 'PD2R update available', body: 'alpha 0.4.2 is ready to download. You can keep playing 0.4.1.', action: { label: 'Update', run: update } });
    if (n === 'error') notify({ title: 'PD2R closed while starting', body: 'Exit code 1. Try again, or open the launcher for Repair and diagnostics.', action: { label: 'Show what happened', run: () => openWin() } });
  };

  const icon = (
    <button className="ticon" aria-label={`${UMBRELLA}: PD2R ${status(t, pct)}`} onClick={leftClick} onContextMenu={rightClick}
      onMouseEnter={() => { tipTimer.current = setTimeout(() => setTip(true), 400); }} onMouseLeave={() => { clearTimeout(tipTimer.current); setTip(false); }}>
      <TrayIcon t={t} pct={pct} />
      {tip && !menu && <span className="ttip">{UMBRELLA}<br />PD2R: {status(t, pct)}</span>}
    </button>
  );

  const scale = Math.min(1, (vw[0] - 80) / 1280, (vw[1] - 48 - 150) / 720);
  const running = win !== 'quit';
  return (
    <div className={'desk' + (light ? ' light' : '')}>
      {win === 'quit' && <div className="gone">Launcher isn't running</div>}
      {(win === 'open' || win === 'min') && (
        <div className="winwrap" style={{ transform: `translate(-50%,-50%) scale(${scale})`, visibility: win === 'open' ? 'visible' : 'hidden' }}>
          <iframe key={TRAY[t].journey} title="Launcher window" src={`/journey.html?embed=1&state=${TRAY[t].journey}`} />
          {quitAsk && (
            <div className="J" style={{ position: 'absolute', inset: 0 }}>
              <div className="backdrop">
                <div className="dialog" role="dialog" aria-modal="true" aria-label="Quit while downloading?">
                  <h2>Quit while PD2R is downloading?</h2>
                  <p>Quitting cancels the download at {pct}%. Nothing is installed yet, and alpha 0.4.1 stays playable. You can start the update again later.</p>
                  <div className="row">
                    <button className="btn primary" onClick={() => { setQuitAsk(false); setT('update'); setWin('quit'); setLog('Cancelled the download and quit.'); }}>Cancel download and quit</button>
                    <button className="btn ghost" onClick={() => { setQuitAsk(false); setLog('Kept downloading.'); }}>Keep downloading</button>
                  </div>
                </div>
              </div>
            </div>
          )}
        </div>
      )}

      {menu && <div className="catch" onClick={() => setMenu(null)} onContextMenu={(e) => { e.preventDefault(); setMenu(null); }} />}
      {menu?.kind === 'native' && <NativeMenu items={menus} x={menu.x} light={light} close={() => setMenu(null)} />}
      {menu?.kind === 'flyout' && <Flyout t={t} pct={pct} acts={acts} open={() => openWin()} settings={settings} quit={quit} />}
      {overflow && !pinned && running && <div className={'ovf' + (light ? ' light' : '')}><span className="fake" />{icon}<span className="fake" /></div>}
      <div className="toasts">{toasts.map((n) => <Toast key={n.id} n={n} light={light} onOpen={() => { setToasts((ts) => ts.filter((x) => x !== n)); openWin('Clicked the notification: window opened.'); }} onClose={() => setToasts((ts) => ts.filter((x) => x !== n))} />)}</div>

      <footer className="taskbar">
        <div className="tmid">
          <span className="tb start" aria-hidden><i /><i /><i /><i /></span>
          <span className="tb search" aria-hidden>⌕</span>
          <span className="tb folder" aria-hidden />
          {(win === 'open' || win === 'min') && (
            <button className={'tb app' + (win === 'open' ? ' on' : '')} aria-label={UMBRELLA} onClick={() => (win === 'open' ? (setWin('min'), setLog('Taskbar button: minimised.')) : openWin('Taskbar button: restored.'))}>
              <TrayIcon t="idle" size={24} />
            </button>
          )}
        </div>
        <div className="tright">
          <button className="chev" aria-label="Show hidden icons" onClick={() => setOverflow(!overflow)}>{overflow ? '⌄' : '⌃'}</button>
          {pinned && running && icon}
          <span className="sys" aria-hidden>ENG &nbsp; ᯤ &nbsp; 🔈</span>
          <span className="clock">10:42<br />07/11/2026</span>
        </div>
      </footer>

      {sheet && (
        <div className="sheet" onClick={() => setSheet(false)}>
          {[false, true].map((lt) => (
            <div key={lt} className={'strip' + (lt ? ' light' : '')}>
              {Object.keys(TRAY).map((k) => (
                <div key={k} className="cell"><TrayIcon t={k} pct={60} size={16} /><TrayIcon t={k} pct={60} size={24} /><TrayIcon t={k} pct={60} size={64} /><span>{TRAY[k].name}</span></div>
              ))}
            </div>
          ))}
          <p>16px (100%), 24px (150%) and 64px (zoomed) on dark and light taskbars. Click to close.</p>
        </div>
      )}

      {import.meta.env.DEV && (
        <div style={bar}>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap', justifyContent: 'center' }}>
            <button style={pill} onClick={() => cycle(-1)} aria-label="Previous variant">←</button>
            <b>{v.key} ({v.name})</b>
            <button style={pill} onClick={() => cycle(1)} aria-label="Next variant">→</button>
            <select style={sel} value={t} onChange={(e) => pickState(e.target.value)} aria-label="Tray state">
              {Object.entries(TRAY).map(([k, x]) => <option key={k} value={k}>{x.name}</option>)}
            </select>
            {t === 'running' && <><button style={pill} onClick={() => { setT('idle'); setLog('The game exited normally.'); }}>Game exits</button>
              <button style={pill} onClick={() => pickState('error')}>Game crashes</button></>}
            <button style={pill} onClick={() => { setWin('hidden'); setMenu(null); setToasts([]); setQuitAsk(false); setLog('Signed in to Windows: Start with Windows opened the launcher in the tray. No window, no notification.'); }}>Sign in to Windows</button>
            {win === 'quit' && <button style={pill} onClick={() => openWin('Launched from the Start menu.')}>Launch from Start</button>}
            <button style={pill} onClick={() => setLight(!light)}>Taskbar: {light ? 'light' : 'dark'}</button>
            <button style={pill} onClick={() => { setPinned(!pinned); setParam('pinned', pinned ? '0' : '1'); }}>Icon: {pinned ? 'on taskbar' : 'in ⌃ overflow'}</button>
            <button style={pill} onClick={() => setSheet(true)}>Icon sheet</button>
          </div>
          <div style={{ fontWeight: 400, fontSize: 12, textAlign: 'center' }}>
            Click {v.click} · right-click: {v.right}{v.key === 'B' && ' · Windows notifications while the window is away'}<br />
            <b>window:</b> {win} · <b>tray:</b> {t}{t === 'progress' && ` ${pct}%`} · <b>last:</b> {log}
          </div>
        </div>
      )}
    </div>
  );
}

// Deliberately off-palette so nobody mistakes it for the design. Top of the screen here: the tray lives at the bottom.
const bar = { position: 'fixed', top: 10, left: '50%', transform: 'translateX(-50%)', display: 'grid', justifyItems: 'center', gap: 4, background: '#ffe14d', color: '#111', font: '600 13px system-ui', padding: '6px 14px', borderRadius: 18, boxShadow: '0 4px 16px #0008', zIndex: 99, maxWidth: 'calc(100vw - 24px)' };
const pill = { background: '#111', color: '#ffe14d', borderRadius: 999, padding: '4px 12px', font: 'inherit' };
const sel = { font: 'inherit', color: '#111', colorScheme: 'light', background: '#fff8c4', border: '1px solid #111', borderRadius: 6, padding: '2px 6px' };

createRoot(document.getElementById('root')).render(<App />);
