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

/** ⌃ over ⌄, as a popup button in a grouped form draws them: 8 wide, 10 high. */
function UpDownChevrons() {
  return (
    <svg
      width={8}
      height={10}
      viewBox="0 0 8 10"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M1 4L4 1L7 4M1 6L4 9L7 6" />
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
