import { PRODUCTS } from "../products";
import type { Badge } from "../product/journey";
import { SettingsDot } from "../selfupdate/SettingsDot";
import { arrowKeys } from "../ui/focus";
import { BRANDING } from "./branding";

export type View = { kind: "product"; id: string } | { kind: "settings" };

const BADGE_NAME: Record<Exclude<Badge["kind"], "progress">, [string, string]> = {
  update: ["↑", "update available"],
  running: ["●", "running"],
  error: ["!", "error"],
};

function BadgeView({ badge }: { badge: Badge }) {
  const [glyph, name] = badge.kind === "progress" ? [badge.text, `${badge.text} done`] : BADGE_NAME[badge.kind];
  return (
    <span className={`badge badge-${badge.kind}`} role="img" aria-label={name}>
      {glyph}
    </span>
  );
}

export interface SidebarProps {
  view: View;
  badges: Record<string, Badge | null>;
  onSelect: (view: View) => void;
}

/** Branding, the products in spec order (arrow keys move between them), and Settings at the bottom. */
export function Sidebar({ view, badges, onSelect }: SidebarProps) {
  const current = view.kind === "product" ? view.id : null;
  return (
    <nav className="sidebar" aria-label="Launcher">
      <div className="brand">
        <span className="glyph" aria-hidden="true">
          {BRANDING.glyph}
        </span>
        <span className="name">{BRANDING.name}</span>
      </div>
      <ul className="products" aria-label="Products" onKeyDown={arrowKeys(".item", "vertical")}>
        {PRODUCTS.map((p, i) => {
          const active = p.id === current;
          const badge = badges[p.id];
          return (
            <li key={p.id}>
              <button
                type="button"
                className="item"
                aria-current={active ? "page" : undefined}
                tabIndex={active || (current === null && i === 0) ? 0 : -1}
                onClick={() => onSelect({ kind: "product", id: p.id })}
              >
                <span className="item-name">{p.name}</span>
                {badge && <BadgeView badge={badge} />}
              </button>
            </li>
          );
        })}
      </ul>
      <button
        type="button"
        className="item settings"
        aria-current={view.kind === "settings" ? "page" : undefined}
        onClick={() => onSelect({ kind: "settings" })}
      >
        <svg className="cog" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
          <circle cx="12" cy="12" r="3.2" />
          <path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1" />
          <circle cx="12" cy="12" r="6.5" />
        </svg>
        Settings
        <SettingsDot />
      </button>
    </nav>
  );
}
