import type { ReactNode } from "react";

/* cc-switch 风格的子页头：返回箭头 + 标题 + 右侧动作槽。 */
export function SubpageHeader({
  title,
  backLabel,
  onBack,
  actions,
}: {
  title: string;
  backLabel: string;
  onBack: () => void;
  actions?: ReactNode;
}) {
  return (
    <div className="flex items-center gap-[14px]">
      <button
        type="button"
        onClick={onBack}
        aria-label={backLabel}
        className="grid h-10 w-10 place-items-center rounded-xl border border-line bg-card p-0 text-[1.15rem] text-main transition-colors hover:border-line-strong hover:bg-muted"
      >
        <span aria-hidden="true">←</span>
      </button>
      <h2 className="m-0 flex-1 text-[1.4rem] font-extrabold text-main">{title}</h2>
      {actions ? <div className="flex items-center gap-[10px]">{actions}</div> : null}
    </div>
  );
}
