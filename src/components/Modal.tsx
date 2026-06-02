import { useEffect, useRef, type ReactNode } from "react";

const SIZE = {
  narrow: "w-[min(600px,100%)]",
  success: "w-[min(520px,100%)]",
  toast: "w-[min(420px,100%)]",
} as const;

/* 共享弹窗外壳：Esc 关闭、背景点击关闭、打开聚焦。 */
export function Modal({
  title,
  kicker,
  onClose,
  size,
  closeLabel,
  showHeaderClose = true,
  children,
  footer,
}: {
  title: string;
  kicker?: string;
  onClose: () => void;
  size?: "narrow" | "success" | "toast";
  closeLabel: string;
  showHeaderClose?: boolean;
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

  return (
    <div
      className="fixed inset-0 z-[100] grid place-items-center p-6 bg-[rgba(15,23,42,0.45)] backdrop-blur-[6px] [overscroll-behavior:contain] [animation:fadeIn_0.18s_ease-out]"
      onClick={onClose}
      role="presentation"
    >
      <section
        ref={panelRef}
        className={`max-h-[calc(100vh-48px)] overflow-y-auto rounded-[22px] border border-line bg-card no-scrollbar shadow-[var(--shadow-lg)] [animation:slideUp_0.26s_cubic-bezier(0.16,1,0.3,1)] ${size ? SIZE[size] : "w-[min(720px,100%)]"}`}
        onClick={(event) => event.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
      >
        <div className="flex items-center justify-between gap-5 border-b border-line px-7 pb-[18px] pt-6">
          <div className="grid gap-0.5">
            {kicker ? <p className="m-0 text-[0.78rem] font-bold uppercase tracking-[0.05em] text-dim">{kicker}</p> : null}
            <h2 className="m-0 text-2xl font-extrabold tracking-[-0.02em] text-main">{title}</h2>
          </div>
          {showHeaderClose ? (
            <button
              type="button"
              onClick={onClose}
              className="rounded-xl bg-muted px-4 py-2 text-[0.85rem] font-bold text-main transition-colors hover:bg-line hover:text-danger"
            >
              {closeLabel}
            </button>
          ) : null}
        </div>
        {children ? <div className="flex flex-col gap-4 px-7 pb-7 pt-6">{children}</div> : null}
        {footer}
      </section>
    </div>
  );
}
