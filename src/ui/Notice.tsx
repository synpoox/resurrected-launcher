import type { ReactNode } from "react";

/** Explains a state in words: a panel with a 2px left rule, muted for information, accent for errors. */
export function Notice({ tone = "info", children }: { tone?: "info" | "error"; children: ReactNode }) {
  return (
    <div className={`notice ${tone}`} role={tone === "error" ? "alert" : "status"}>
      {children}
    </div>
  );
}
