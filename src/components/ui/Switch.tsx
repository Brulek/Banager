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
   * changes it. Settings' 「有更新时通知我」 under 「检查更新」.
   */
  disabled?: boolean;
}

/**
 * A Mac's switch in the size a grouped form uses (System Settings' rows,
 * measured on macOS 27: native-sui-settings): a 36 by 16 track, and in it
 * a 20 by 12 capsule, 2 in from the track's edges, on the left while off
 * and the right while on: `switch-knob`, white, but a light grey in dark
 * mode, where white would be the brightest thing on the page. Off, the
 * track is `switch-off`, the grey a group's fill shows through; on, the
 * accent. The knob has a hairline edge
 * and the slightest shadow under it, as AppKit's has, so that it reads on
 * the light track. Disabled, the whole is drawn as it would be enabled, at
 * half opacity, in either appearance. The keyboard's focus ring is the
 * page's own (index.css), round as the track is.
 */
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
      className="relative inline-flex h-4 w-9 shrink-0 items-center rounded-full bg-switch-off disabled:opacity-50 data-[state=checked]:bg-accent"
    >
      {/* Slides only once it is pressed, in under 200 ms (spec §2.8), and
          jumps with Reduce motion on. */}
      <RadixSwitch.Thumb className="block h-3 w-5 translate-x-0.5 rounded-full bg-switch-knob shadow-[0_0_0_0.5px_rgb(0_0_0/0.12),0_1px_1.5px_rgb(0_0_0/0.18)] transition-transform duration-150 motion-reduce:transition-none data-[state=checked]:translate-x-3.5" />
    </RadixSwitch.Root>
  );
}
