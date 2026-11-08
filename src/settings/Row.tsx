import type { ReactNode } from "react";

/** A Settings row: a label and a hint on the left, the control on the right. */
export function Row({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="srow">
      <div className="srow-text">
        <b>{label}</b>
        {hint && <span className="hint">{hint}</span>}
      </div>
      <div className="ctl">{children}</div>
    </div>
  );
}

/** A titled group of rows. */
export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="ssec" aria-label={title}>
      <h2 className="slabel">{title}</h2>
      {children}
    </section>
  );
}
