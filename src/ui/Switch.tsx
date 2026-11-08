import { useId } from "react";

export interface SwitchProps {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** Disables the switch; shown next to it so the player knows why. */
  disabledReason?: string;
}

/** An on/off switch (`role="switch"`). Space and Enter toggle it. */
export function Switch({ label, checked, onChange, disabledReason }: SwitchProps) {
  const reason = useId();
  return (
    <span className="switch-wrap">
      <button
        type="button"
        role="switch"
        className="switch"
        aria-checked={checked}
        aria-label={label}
        aria-describedby={disabledReason ? reason : undefined}
        disabled={disabledReason !== undefined}
        onClick={() => onChange(!checked)}
      >
        <span className="knob" aria-hidden="true" />
      </button>
      {disabledReason && (
        <span id={reason} className="why">
          {disabledReason}
        </span>
      )}
    </span>
  );
}
