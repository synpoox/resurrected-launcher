import { useEffect, useRef, useState } from "react";
import { HERO_ART } from "../art";
import type { Product } from "../products";
import { motionAllowed } from "../ui/motion";

/** The art on screen now. A product switch renders the new HeroArt while the old one is still mounted
 *  (cleared on unmount, so coming back from Settings finds nothing and just appears). */
let shown: { src: string; position: string; size: string } | null = null;

function paint(el: HTMLDivElement | null, a: { src: string; position: string; size?: string }) {
  // CSSOM, not a style attribute: the CSP has `style-src 'self'`. The src is a bundled module URL.
  const s = el?.style;
  if (!s) return;
  s.setProperty("background-image", `url("${a.src}")`);
  s.setProperty("background-position", a.position);
  s.setProperty("background-size", a.size ?? "cover");
}

/** Full-bleed hero art behind the main area, under the top, left and bottom gradient. */
export function HeroArt({ product }: { product: Product }) {
  const art = HERO_ART[product.hero];
  const ref = useRef<HTMLDivElement>(null);
  const underRef = useRef<HTMLDivElement>(null);
  // The old product's art stays opaque beneath while the new art fades in over it: the app background never shows.
  const [under] = useState(() => {
    return shown && shown.src !== art.src && motionAllowed() ? shown : null;
  });
  const [crossing, setCrossing] = useState(under !== null);
  useEffect(() => {
    const top = ref.current;
    paint(top, art);
    // Native listener: the old layer goes once the new art is fully in.
    const end = () => setCrossing(false);
    top?.addEventListener("animationend", end);
    if (under) paint(underRef.current, under);
    const mine = { src: art.src, position: art.position, size: art.size ?? "cover" };
    shown = mine;
    return () => {
      top?.removeEventListener("animationend", end);
      if (shown === mine) shown = null;
    };
  }, [art, under]);
  return (
    <>
      {crossing && under && <div ref={underRef} className="art" aria-hidden="true" />}
      <div
        ref={ref}
        className={`art${under ? " art-in" : ""}`}
        role="img"
        aria-label={art.accessibleName}
      />
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
