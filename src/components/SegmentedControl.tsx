export type SegmentOption<T extends string> = {
  value: T;
  label: string;
};

/* cc-switch 风格的分段控件：替代原生 <select>，激活项蓝色实心。 */
export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  ariaLabel,
}: {
  value: T;
  options: SegmentOption<T>[];
  onChange: (next: T) => void;
  ariaLabel?: string;
}) {
  return (
    <div className="segmented" role="group" aria-label={ariaLabel}>
      {options.map((option) => {
        const active = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            className={active ? "segmented-item active" : "segmented-item"}
            aria-pressed={active}
            onClick={() => onChange(option.value)}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
