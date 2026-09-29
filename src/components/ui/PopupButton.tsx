import type { ChangeEvent } from "react";

export interface PopupOption<T extends string> {
  value: T;
  label: string;
}

export interface PopupButtonProps<T extends string> {
  /** The `<select>`'s id, for the `<label htmlFor>` that names it. */
  id: string;
  value: T;
  options: PopupOption<T>[];
  onChange: (value: T) => void;
}

/**
 * ⌃ over ⌄, as a popup button in a grouped form draws them: 8 wide, 12
 * high, in a light stroke, with 2 between the two so they never meet in a
 * diamond.
 */
function UpDownChevrons() {
  return (
    <svg
      width={8}
      height={12}
      viewBox="0 0 8 12"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.25}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M1.25 4.5L4 1.75L6.75 4.5M1.25 7.5L4 10.25L6.75 7.5" />
    </svg>
  );
}

/**
 * A grouped form's popup button, as System Settings draws one in a row
 * (native-sui-settings, spec §2.7): the chosen value in the body size,
 * then a grey capsule, 20 by 18, holding ⌃⌄ -- no border, no fill behind
 * the value.
 *
 * Underneath it is a plain `<select>`, laid over the whole of it and
 * transparent: pressed, WebKit opens the Mac's own menu, with a check by
 * the value, and the keyboard and screen readers get a select's behaviour.
 * What shows is drawn beside it, so its width is the chosen value's,
 * whatever the longest option is. The keyboard's focus ring goes round
 * the whole, since the select itself is not seen.
 */
export function PopupButton<T extends string>({ id, value, options, onChange }: PopupButtonProps<T>) {
  const current = options.find((option) => option.value === value);
  return (
    <span className="relative mr-1 inline-flex h-5 shrink-0 items-center gap-4 rounded-control has-[:focus-visible]:outline-3 has-[:focus-visible]:outline-focus">
      <span aria-hidden="true" className="text-body text-foreground">
        {current?.label}
      </span>
      <span
        aria-hidden="true"
        className="inline-flex h-4.5 w-5 items-center justify-center rounded-full bg-fill text-foreground"
      >
        <UpDownChevrons />
      </span>
      <select
        id={id}
        value={value}
        onChange={(event: ChangeEvent<HTMLSelectElement>) => onChange(event.target.value as T)}
        className="absolute inset-0 h-full w-full appearance-none opacity-0 outline-none"
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </span>
  );
}

export interface ToolbarPopupButtonProps<T extends string> {
  /** Its accessible name, which no label beside it gives: 「排序方式」/"Sort Order". */
  label: string;
  value: T;
  options: PopupOption<T>[];
  onChange: (value: T) => void;
}

/** ⌄, as a toolbar's popup button draws it after its value: 8 wide, 5 high, in the same light stroke. */
function DownChevron() {
  return (
    <svg
      width={8}
      height={5}
      viewBox="0 0 8 5"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.25}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M1 1L4 4L7 1" />
    </svg>
  );
}

/**
 * A toolbar's popup button, as a Mac toolbar draws one (spec §2.7, §3.2):
 * a regular grey button -- 24 high, a control's corners, the fill, the
 * value in the body size -- with ⌄ after the value: 「按名称 ⌄」.
 *
 * Underneath it, as with `PopupButton`, a transparent `<select>` over
 * the whole: pressed, WebKit opens the Mac's own menu with a check by the
 * value, and the keyboard and a screen reader get a select's behaviour,
 * named by `label`. The fill darkens while it is pressed, as a grey
 * button's does, and the focus ring goes round the whole.
 */
export function ToolbarPopupButton<T extends string>({ label, value, options, onChange }: ToolbarPopupButtonProps<T>) {
  const current = options.find((option) => option.value === value);
  return (
    <span className="relative inline-flex h-6 shrink-0 items-center gap-1.5 rounded-control bg-fill pl-3 pr-2 text-body text-foreground has-[:active]:bg-fill-pressed has-[:focus-visible]:outline-3 has-[:focus-visible]:outline-focus">
      <span aria-hidden="true" className="whitespace-nowrap">
        {current?.label}
      </span>
      <span aria-hidden="true" className="flex text-muted">
        <DownChevron />
      </span>
      <select
        aria-label={label}
        value={value}
        onChange={(event: ChangeEvent<HTMLSelectElement>) => onChange(event.target.value as T)}
        className="absolute inset-0 h-full w-full appearance-none opacity-0 outline-none"
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </span>
  );
}
