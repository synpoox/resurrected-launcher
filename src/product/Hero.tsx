import { useEffect, useRef } from "react";
import { HERO_ART } from "../art";
import type { Product } from "../products";

/** Full-bleed hero art behind the main area, under the top, left and bottom gradient. */
export function HeroArt({ product }: { product: Product }) {
  const art = HERO_ART[product.hero];
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // CSSOM, not a style attribute: the CSP has `style-src 'self'`. The src is a bundled module URL.
    const s = ref.current?.style;
    if (!s) return;
    s.setProperty("background-image", `url("${art.src}")`);
    s.setProperty("background-position", art.position);
    s.setProperty("background-size", art.size ?? "cover");
  }, [art]);
  return (
    <>
      <div ref={ref} className="art" role="img" aria-label={art.accessibleName} />
      <div className="shade" aria-hidden="true" />
    </>
  );
}

/** Ribbon, the two-line title and the description. */
export function HeroCopy({ product }: { product: Product }) {
  return (
    <div className="copy">
      <span className={`ribbon${product.hasRelease ? "" : " soon"}`}>{product.ribbon}</span>
      <h1 className="title">
        <span>{product.title[0]}</span>
        <span>{product.title[1]}</span>
      </h1>
      <p className="description">{product.description}</p>
    </div>
  );
}
