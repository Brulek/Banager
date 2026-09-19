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
      className="relative h-6 w-10 shrink-0 rounded-full bg-[var(--color-hover)] outline-none data-[state=checked]:bg-[var(--color-accent)]"
    >
      <RadixSwitch.Thumb className="block h-5 w-5 translate-x-0.5 rounded-full bg-[var(--color-background)] transition-transform duration-150 data-[state=checked]:translate-x-[18px]" />
    </RadixSwitch.Root>
  );
}
