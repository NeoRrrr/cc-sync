import { useEffect, useRef, type ReactNode } from "react";

/* 共享弹窗外壳：用于同步确认、成功提示、保存提示等轻量浮层。
 * 统一处理 Esc 关闭、背景点击关闭、打开时聚焦面板，替代原先复制粘贴的 5 处 modal。 */
export function Modal({
  title,
  kicker,
  onClose,
  size,
  closeLabel,
  children,
  footer,
}: {
  title: string;
  kicker?: string;
  onClose: () => void;
  size?: "narrow" | "success" | "toast";
  closeLabel: string;
  children?: ReactNode;
  footer?: ReactNode;
}) {
  const panelRef = useRef<HTMLElement>(null);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    panelRef.current?.focus();
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const panelClass = size ? `modal-panel modal-panel-${size}` : "modal-panel";

  return (
    <div className="modal-backdrop" onClick={onClose} role="presentation">
      <section
        ref={panelRef}
        className={panelClass}
        onClick={(event) => event.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
      >
        <div className="modal-head">
          <div className="modal-title-block">
            {kicker ? <p className="modal-kicker">{kicker}</p> : null}
            <h2>{title}</h2>
          </div>
          <button type="button" className="ghost" onClick={onClose}>
            {closeLabel}
          </button>
        </div>
        {children ? <div className="modal-body">{children}</div> : null}
        {footer}
      </section>
    </div>
  );
}
