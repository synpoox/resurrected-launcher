import { useApi } from "../api/context";
import { PRODUCTS } from "../products";
import { asError } from "../product/store";
import { useToast } from "../ui/Toast";
import type { Badge } from "../product/journey";
import { SettingsDot } from "../selfupdate/SettingsDot";
import { arrowKeys } from "../ui/focus";
import { BRANDING } from "./branding";

const SOCIAL = [
  {
    id: "github",
    label: "GitHub",
    path: "M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z",
    box: "0 0 16 16",
  },
  {
    id: "discord",
    label: "Discord",
    path: "M6 5.5C7.9 4.6 9.900 4.200 12 4.200s4.100.4 6 1.300c1.600 2.600 2.400 5.500 2.300 8.800-1.400 1.100-2.800 1.800-4.300 2.300l-1-1.700c.5-.2 1-.4 1.500-.7l-.4-.3c-2.700 1.200-5.400 1.200-8.200 0l-.4.300c.5.300 1 .5 1.500.7l-1 1.700c-1.500-.5-2.900-1.200-4.300-2.300C3.600 11 4.400 8.100 6 5.500zM9.200 9.600a1.600 1.700 0 1 0 0 3.400 1.600 1.700 0 1 0 0-3.400zM14.800 9.600a1.600 1.700 0 1 0 0 3.400 1.600 1.700 0 1 0 0-3.400z",
    box: "0 0 24 24",
    evenodd: true,
  },
  {
    id: "kofi",
    label: "Ko-fi",
    path: "M3 6.500h13v7.500a4 4 0 0 1-4 4H7a4 4 0 0 1-4-4zM9.500 15.700 7.300 13.500a1.500 1.500 0 0 1 2.200-2 1.500 1.500 0 0 1 2.200 2z",
    box: "0 0 24 24",
    evenodd: true,
    handle: "M16 8.500h1.500a2.500 2.500 0 0 1 0 5H16",
  },
] as const;

export type View = { kind: "product"; id: string } | { kind: "settings" };

const BADGE_NAME: Record<
  Exclude<Badge["kind"], "progress">,
  [string, string]
> = {
  update: ["↑", "update available"],
  running: ["●", "running"],
  error: ["!", "error"],
};

function BadgeView({ badge }: { badge: Badge }) {
  const [glyph, name] =
    badge.kind === "progress"
      ? [badge.text, `${badge.text} done`]
      : BADGE_NAME[badge.kind];
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
  const api = useApi();
  const toast = useToast();
  const current = view.kind === "product" ? view.id : null;
  return (
    <nav className="sidebar" aria-label="Launcher">
      <div className="brand">
        <span className="glyph" aria-hidden="true">
          {BRANDING.glyph}
        </span>
        <span className="name">{BRANDING.name}</span>
      </div>
      <ul
        className="products"
        aria-label="Products"
        onKeyDown={arrowKeys(".item", "vertical")}
      >
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
      <div className="social" role="group" aria-label="Community links">
        {SOCIAL.map((s) => (
          <button
            key={s.id}
            type="button"
            className="social-link"
            aria-label={s.label}
            title={s.label}
            onClick={() =>
              void api
                .openSocialLink(s.id)
                .catch((e) =>
                  toast.show({ tone: "error", text: asError(e).message }),
                )
            }
          >
            <svg
              className="social-icon"
              viewBox={s.box}
              aria-hidden="true"
              focusable="false"
            >
              <path
                d={s.path}
                fillRule={"evenodd" in s ? "evenodd" : undefined}
              />
              {"handle" in s && (
                <path
                  d={s.handle}
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                />
              )}
            </svg>
          </button>
        ))}
      </div>
      <button
        type="button"
        className="item settings"
        aria-current={view.kind === "settings" ? "page" : undefined}
        onClick={() => onSelect({ kind: "settings" })}
      >
        <svg
          className="cog"
          viewBox="0 0 24 24"
          aria-hidden="true"
          focusable="false"
        >
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
