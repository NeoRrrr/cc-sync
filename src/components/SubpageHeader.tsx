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
    <div className="subpage-head">
      <button type="button" className="back-btn" onClick={onBack} aria-label={backLabel}>
        <span aria-hidden="true">←</span>
      </button>
      <h2 className="subpage-title">{title}</h2>
      {actions ? <div className="subpage-actions">{actions}</div> : null}
    </div>
  );
}
