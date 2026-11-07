// PROTOTYPE for #17: launcher shell compositions in the ready state, switchable via ?variant=A|B|C|D
// and ?size=1280|1024 (window 1280x720 default, 1024x640 minimum). Throwaway: never merge into main.
import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './tokens.css';
import './styles.css';
import { PRODUCTS, UMBRELLA } from './data.js';
import { VARIANTS } from './variants.jsx';

const SIZES = { 1280: [1280, 720], 1024: [1024, 640] };
const param = (k, d) => new URLSearchParams(location.search).get(k) ?? d;
const setParam = (k, v) => { const u = new URL(location.href); u.searchParams.set(k, v); history.replaceState(null, '', u); };

function TitleBar({ mark }) {
  return (
    <header className="titlebar">
      {mark === 'glyph' && <span className="glyph" aria-hidden>/</span>}
      {mark === 'svg' && <img src="/mark.svg" alt="" />}
      {mark !== 'none' && <span className="name">{UMBRELLA}</span>}
      <div className="drag" />
      <button className="win" aria-label="Minimise">&#8211;</button>
      <button className="win" aria-label="Maximise">&#9633;</button>
      <button className="win close" aria-label="Close">&#10005;</button>
    </header>
  );
}

function App() {
  const [vi, setVi] = useState(Math.max(0, VARIANTS.findIndex((v) => v.key === param('variant', 'A'))));
  const [size, setSize] = useState(SIZES[param('size', '1280')] ? param('size', '1280') : '1280');
  const [pid, setPid] = useState(param('product', 'pd2r'));
  const [vw, setVw] = useState([innerWidth, innerHeight]);
  const v = VARIANTS[vi];
  const [w, h] = SIZES[size];
  const scale = Math.min(1, (vw[0] - 24) / w, (vw[1] - 80) / h);

  const cycle = (d) => setVi((i) => { const n = (i + d + VARIANTS.length) % VARIANTS.length; setParam('variant', VARIANTS[n].key); return n; });
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
  const pick = (id) => { setPid(id); setParam('product', id); };
  const toggleSize = () => { const s = size === '1280' ? '1024' : '1280'; setSize(s); setParam('size', s); };

  return (
    <>
      <div className="stage">
        <div className={'window' + (v.mark === 'none' ? ' seamless' : '')} style={{ width: w, height: h, position: 'absolute', left: '50%', top: 'calc(50% - 28px)', transform: `translate(-50%,-50%) scale(${scale})` }}>
          <TitleBar mark={v.mark} />
          <v.C product={PRODUCTS.find((p) => p.id === pid) ?? PRODUCTS[0]} setProduct={pick} />
        </div>
      </div>
      {import.meta.env.DEV && (
        <div style={bar}>
          <button style={pill} onClick={() => cycle(-1)} aria-label="Previous variant">←</button>
          <span style={{ minWidth: 200, textAlign: 'center' }}>{v.key} ({v.name})</span>
          <button style={pill} onClick={() => cycle(1)} aria-label="Next variant">→</button>
          <button style={pill} onClick={toggleSize}>{w}×{h}</button>
        </div>
      )}
    </>
  );
}

// Deliberately off-palette so nobody mistakes it for the design.
const bar = { position: 'fixed', bottom: 12, left: '50%', transform: 'translateX(-50%)', display: 'flex', gap: 8, alignItems: 'center', background: '#ffe14d', color: '#111', font: '600 13px system-ui', padding: '6px 10px', borderRadius: 999, boxShadow: '0 4px 16px #0008', zIndex: 99 };
const pill = { background: '#111', color: '#ffe14d', borderRadius: 999, padding: '4px 12px', font: 'inherit' };

createRoot(document.getElementById('root')).render(<App />);
