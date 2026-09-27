import * as RadixSwitch from "@radix-ui/react-switch";

export interface SwitchProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  id?: string;
  "aria-label"?: string;
  /** Points at a description element; Task 15 uses it to keep the Switch's accessible name to the label text alone. */
  "aria-describedby"?: string;
}

export function Switch({
  checked,
  onCheckedChange,
  id,
  "aria-label": ariaLabel,
  "aria-describedby": ariaDescribedBy,
}: SwitchProps) {
  return (
    <RadixSwitch.Root
      id={id}
      aria-label={ariaLabel}
      aria-describedby={ariaDescribedBy}
      checked={checked}
      onCheckedChange={onCheckedChange}
      // Off, a grey that still reads as a control on a white card (the
      // quiet fill alone all but vanished there); on, the accent. A ring
      // for the keyboard's focus.
      className="relative h-6 w-10 shrink-0 rounded-full bg-muted/45 outline-none transition-colors focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-surface data-[state=checked]:bg-accent"
    >
      <RadixSwitch.Thumb className="block h-5 w-5 translate-x-0.5 rounded-full bg-white shadow-sm shadow-black/20 transition-transform duration-150 data-[state=checked]:translate-x-[18px]" />
    </RadixSwitch.Root>
  );
}
