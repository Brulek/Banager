import * as RadixSwitch from "@radix-ui/react-switch";

export interface SwitchProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  id?: string;
  "aria-label"?: string;
  /** Points at a description element; Task 15 uses it to keep the Switch's accessible name to the label text alone. */
  "aria-describedby"?: string;
  /**
   * A switch that cannot be changed now, because another setting it
   * depends on is off: drawn faded, and neither a click nor the keyboard
   * changes it. Settings' 「有可更新时通知我」 under 「每天自动检查」.
   */
  disabled?: boolean;
}

export function Switch({
  checked,
  onCheckedChange,
  id,
  "aria-label": ariaLabel,
  "aria-describedby": ariaDescribedBy,
  disabled = false,
}: SwitchProps) {
  return (
    <RadixSwitch.Root
      id={id}
      aria-label={ariaLabel}
      aria-describedby={ariaDescribedBy}
      checked={checked}
      onCheckedChange={onCheckedChange}
      disabled={disabled}
      // Off, a grey that still reads as a control on a white card (the
      // quiet fill alone all but vanished there); on, the accent. The
      // keyboard's focus ring is the page's own (index.css), round as the
      // switch is. Disabled, faded, with no pointer.
      className="relative h-6 w-10 shrink-0 rounded-full bg-muted/45 disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-accent"
    >
      <RadixSwitch.Thumb className="block h-5 w-5 translate-x-0.5 rounded-full bg-white shadow-sm shadow-black/20 transition-transform duration-150 data-[state=checked]:translate-x-[18px]" />
    </RadixSwitch.Root>
  );
}
