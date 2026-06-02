import { useState } from "react";
import type { Dictionary, Language } from "../i18n";
import type { Theme } from "../theme";
import { SubpageHeader } from "./SubpageHeader";
import { SegmentedControl } from "./SegmentedControl";

const TAB_BASE = "rounded-[9px] border-0 px-[18px] py-2 text-[0.88rem] font-bold transition-colors";

/* 设置子页面：通用(界面语言 + 外观主题) / 关于。 */
export function SettingsView({
  text,
  language,
  onLanguage,
  theme,
  onTheme,
  closeToTray,
  onCloseToTray,
  version,
  onBack,
}: {
  text: Dictionary;
  language: Language;
  onLanguage: (next: Language) => void;
  theme: Theme;
  onTheme: (next: Theme) => void;
  closeToTray: boolean;
  onCloseToTray: (next: boolean) => void;
  version: string;
  onBack: () => void;
}) {
  const [tab, setTab] = useState<"general" | "about">("general");

  return (
    <section className="flex flex-col gap-5">
      <SubpageHeader title={text.app.settings} backLabel={text.app.back} onBack={onBack} />

      <div className="inline-flex gap-1 self-start rounded-xl border border-line bg-muted p-1" role="tablist">
        <button
          type="button"
          role="tab"
          aria-selected={tab === "general"}
          className={`${TAB_BASE} ${tab === "general" ? "bg-primary text-white" : "bg-transparent text-dim hover:text-main"}`}
          onClick={() => setTab("general")}
        >
          {text.app.settingsGeneral}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === "about"}
          className={`${TAB_BASE} ${tab === "about" ? "bg-primary text-white" : "bg-transparent text-dim hover:text-main"}`}
          onClick={() => setTab("about")}
        >
          {text.app.settingsAbout}
        </button>
      </div>

      {tab === "general" && (
        <div className="flex flex-col gap-7 rounded-2xl border border-line bg-card p-6 shadow-[var(--shadow-sm)]">
          <div className="flex flex-col items-start gap-3">
            <p className="m-0 text-base font-extrabold text-main">{text.app.uiLanguage}</p>
            <p className="m-0 text-[0.85rem] text-dim">{text.app.uiLanguageHint}</p>
            <SegmentedControl<Language>
              ariaLabel={text.app.uiLanguage}
              value={language}
              onChange={onLanguage}
              options={[
                { value: "zh-CN", label: "中文" },
                { value: "en", label: "English" },
              ]}
            />
          </div>

          <div className="flex flex-col items-start gap-3">
            <p className="m-0 text-base font-extrabold text-main">{text.app.theme}</p>
            <p className="m-0 text-[0.85rem] text-dim">{text.app.themeHint}</p>
            <SegmentedControl<Theme>
              ariaLabel={text.app.theme}
              value={theme}
              onChange={onTheme}
              options={[
                { value: "light", label: text.app.themeLight },
                { value: "dark", label: text.app.themeDark },
                { value: "system", label: text.app.themeSystem },
              ]}
            />
          </div>

          <label className="flex w-full items-start gap-3 rounded-xl border border-line bg-subtle p-4">
            <input
              type="checkbox"
              className="mt-1 h-[18px] w-[18px] shrink-0"
              checked={closeToTray}
              onChange={(event) => onCloseToTray(event.target.checked)}
            />
            <span className="grid gap-1">
              <span className="text-base font-extrabold text-main">{text.app.closeBehavior}</span>
              <span className="text-[0.85rem] text-dim">{text.app.closeBehaviorHint}</span>
              <span className="text-[0.85rem] font-bold text-main">{text.app.minimizeOnClose}</span>
            </span>
          </label>
        </div>
      )}

      {tab === "about" && (
        <div className="flex flex-col gap-7 rounded-2xl border border-line bg-card p-6 shadow-[var(--shadow-sm)]">
          <div className="flex flex-col items-start gap-3">
            <p className="m-0 text-base font-extrabold text-main">{text.app.title}</p>
            <p className="m-0 text-[0.85rem] text-dim">{text.app.aboutDesc}</p>
            <p className="m-0 text-dim [font-family:var(--font-mono)]">
              {text.app.version} v{version}
            </p>
          </div>
        </div>
      )}
    </section>
  );
}
