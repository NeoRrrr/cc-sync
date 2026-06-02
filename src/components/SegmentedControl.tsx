export type SegmentOption<T extends string> = {
  value: T;
  label: string;
};

/* cc-switch 风格的分段控件：替代原生 <select>，激活项实心强调色。 */
export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  ariaLabel,
  disabled = false,
}: {
  value: T;
  options: SegmentOption<T>[];
  onChange: (next: T) => void;
  ariaLabel?: string;
  disabled?: boolean;
}) {
  return (
    <div role="group" aria-label={ariaLabel} className="inline-flex gap-1 rounded-xl border border-line bg-muted p-1">
      {options.map((option) => {
        const active = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            aria-pressed={active}
            onClick={() => onChange(option.value)}
            disabled={disabled}
            className={[
              "rounded-[9px] border-0 px-4 py-[7px] text-[0.85rem] font-bold transition-colors",
              active ? "bg-primary text-white shadow-sm" : "bg-transparent text-dim hover:text-main",
            ].join(" ")}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
