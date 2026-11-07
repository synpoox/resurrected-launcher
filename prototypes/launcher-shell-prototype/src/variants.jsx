// PROTOTYPE variants for #17. Each variant owns its layout; only data and the title bar are shared.
import { useState } from 'react';
import { PRODUCTS, NEWS, CREDIT } from './data.js';

const Art = ({ p, className = '' }) => (
  <div key={p.id} className={'art ' + className} role="img" aria-label={p.credit + ' (official D2R promotional art)'}
    style={{ backgroundImage: `url("${p.art}")`, backgroundPosition: p.artPos }} />
);
const Headline = ({ p }) => <>{p.headline[0]}<br />{p.headline[1]}</>;

// A: the Riot skeleton - text sidebar, hero top, two news cards, right column holds Play + readiness.
export function VariantA({ product, setProduct }) {
  const p = product;
  const news = NEWS[p.id].slice(0, 2);
  return (
    <div className="A slash">
      <nav aria-label="Products">
        {PRODUCTS.map((x) => (
          <button key={x.id} className="item" aria-current={x.id === p.id || undefined} onClick={() => setProduct(x.id)}>{x.name}</button>
        ))}
        <button className="item foot">Settings</button>
      </nav>
      <main>
        <section className="hero">
          <Art p={p} /><div className="shade" />
          <div className="copy lift" key={p.id}>
            <span className={'ribbon' + (p.playable ? '' : ' soon')}>{p.tag}</span>
            <h1 className="h1"><Headline p={p} /></h1>
            <p>{p.blurb}</p>
          </div>
          <div className="credit">{CREDIT}</div>
        </section>
        <section className="news" aria-label="News">
          {news.length ? news.map((n) => (
            <article key={n.title} className="card">
              <span className="label" style={{ color: 'var(--detail)' }}>{n.tag} · {n.date}</span>
              <span className="t">{n.title}</span>
              <a className="btn ghost" href="#">Read more</a>
            </article>
          )) : <div className="empty">No news for this product yet.</div>}
        </section>
      </main>
      <aside>
        {p.playable ? (
          <>
            <button className="btn primary">Play</button>
            <dl>{p.readiness.map(([k, v]) => <div key={k}><dt>{k}</dt><dd className="ok">{v}</dd></div>)}</dl>
            <div className="small">
              <span className="label">{p.plugins}</span>
              <button className="btn ghost">Plugins</button>
              <button className="btn ghost">Repair</button>
            </div>
          </>
        ) : (
          <>
            <span className="label">Not yet released</span>
            <button className="btn ghost">About {p.short}</button>
          </>
        )}
      </aside>
    </div>
  );
}

// B: cinematic - art fills the whole content area, icon rail, action bottom-left, text feed bottom-right.
export function VariantB({ product, setProduct }) {
  const p = product;
  return (
    <div className="B">
      <nav aria-label="Products">
        {PRODUCTS.map((x) => (
          <button key={x.id} className="tile" aria-label={x.name} aria-current={x.id === p.id || undefined} onClick={() => setProduct(x.id)}>
            {x.short === 'Forever' ? 'F' : x.short}<span className="tip">{x.name}</span>
          </button>
        ))}
        <button className="tile foot" aria-label="Settings">{'⚙'}</button>
      </nav>
      <main>
        <Art p={p} /><div className="shade" />
        <div className="credit">{CREDIT}</div>
        <div className="bottom">
          <div className="lift" key={p.id}>
            <span className={'ribbon' + (p.playable ? '' : ' soon')}>{p.tag}</span>
            <h1 className="h1"><Headline p={p} /></h1>
            <p>{p.blurb}</p>
            <div className="act">
              {p.playable ? (
                <>
                  <button className="btn primary">Play</button>
                  <span className="ready"><span className="dot" />Ready · {p.readiness.map((r) => r[1]).join(' · ')}</span>
                </>
              ) : <button className="btn ghost">About {p.short}</button>}
            </div>
          </div>
          <div className="feed" aria-label="News">
            <span className="label" style={{ padding: '10px 0' }}>News</span>
            {NEWS[p.id].length ? NEWS[p.id].map((n) => (
              <a key={n.title} href="#"><span className="m">{n.tag} · {n.date}</span><span className="t">{n.title} ↗</span></a>
            )) : <span className="m" style={{ padding: '12px 0', color: 'var(--muted)', fontSize: 12 }}>Nothing yet.</span>}
          </div>
        </div>
      </main>
    </div>
  );
}

// C: Home entry + product page with a short banner, a full-width action bar and News | About tabs.
export function VariantC({ product, setProduct }) {
  const [home, setHome] = useState(false);
  const [tab, setTab] = useState('news');
  const go = (id) => { setHome(false); setProduct(id); };
  const p = product;
  return (
    <div className="C">
      <nav aria-label="Products">
        <button className="item" aria-current={home || undefined} onClick={() => setHome(true)}><span className="m">⌂</span>Home</button>
        <hr />
        {PRODUCTS.map((x) => (
          <button key={x.id} className="item" aria-current={(!home && x.id === p.id) || undefined} onClick={() => go(x.id)}>
            <span className="m">{x.short === 'Forever' ? 'F' : x.short}</span>{x.name}
            {!x.playable && <span className="badge">SOON</span>}
          </button>
        ))}
        <button className="item foot"><span className="m">{'⚙'}</span>Settings</button>
      </nav>
      {home ? (
        <main className="home">
          <span className="label">All products</span>
          <div className="tiles">
            {PRODUCTS.map((x) => (
              <button key={x.id} className="tile" onClick={() => go(x.id)}>
                <Art p={x} /><div className="shade" />
                <div className="in">
                  <span className={'ribbon' + (x.playable ? '' : ' soon')}>{x.tag}</span>
                  <b>{x.name}</b>
                </div>
              </button>
            ))}
          </div>
        </main>
      ) : (
        <main>
          <section className="banner">
            <Art p={p} /><div className="shade" />
            <div className="credit">{CREDIT}</div>
            <div className="copy lift" key={p.id}>
              <span className={'ribbon' + (p.playable ? '' : ' soon')}>{p.tag}</span>
              <h1 className="h1"><Headline p={p} /></h1>
            </div>
          </section>
          <div className="bar">
            {p.playable ? (
              <>
                <button className="btn primary">Play</button>
                {p.readiness.map(([k, v]) => <div key={k} className="seg"><span className="label">{k}</span><b className="ok">{v}</b></div>)}
                <div className="seg"><span className="label">Plugins</span><b>{p.plugins}</b></div>
              </>
            ) : <div className="seg"><span className="label">Status</span><b>Not yet released. Follow progress in News.</b></div>}
          </div>
          <section className="lower">
            <div className="tabs" role="tablist">
              {['news', 'about'].map((t) => (
                <button key={t} role="tab" aria-selected={tab === t} onClick={() => setTab(t)}>{t === 'news' ? 'News' : 'About ' + p.short}</button>
              ))}
            </div>
            {tab === 'news' ? (
              NEWS[p.id].length ? NEWS[p.id].map((n) => (
                <a key={n.title} className="row" href="#"><span className="label">{n.tag}</span><b>{n.title}</b><span className="d">{n.date} ↗</span></a>
              )) : <p className="about">No news for this product yet.</p>
            ) : <p className="about">{p.blurb}</p>}
          </section>
        </main>
      )}
    </div>
  );
}

export const VARIANTS = [
  { key: 'A', name: 'Riot skeleton', C: VariantA, mark: 'glyph' },
  { key: 'B', name: 'Cinematic full-bleed', C: VariantB, mark: 'svg' },
  { key: 'C', name: 'Home + tabs', C: VariantC, mark: 'svg' },
];
