import { useState } from "react";
import type { Dictionary, Language } from "../i18n";
import type { Theme } from "../theme";
import { SubpageHeader } from "./SubpageHeader";
import { SegmentedControl } from "./SegmentedControl";

/* 设置子页面：通用(界面语言 + 外观主题) / 关于。对齐 cc-switch 的设置页结构。 */
export function SettingsView({
  text,
  language,
  onLanguage,
  theme,
  onTheme,
  version,
  onBack,
}: {
  text: Dictionary;
  language: Language;
  onLanguage: (next: Language) => void;
  theme: Theme;
  onTheme: (next: Theme) => void;
  version: string;
  onBack: () => void;
}) {
  const [tab, setTab] = useState<"general" | "about">("general");

  return (
    <section className="subpage">
      <SubpageHeader title={text.app.settings} backLabel={text.app.back} onBack={onBack} />

      <div className="settings-tabs" role="tablist">
        <button
          type="button"
          role="tab"
          aria-selected={tab === "general"}
          className={tab === "general" ? "settings-tab active" : "settings-tab"}
          onClick={() => setTab("general")}
        >
          {text.app.settingsGeneral}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === "about"}
          className={tab === "about" ? "settings-tab active" : "settings-tab"}
          onClick={() => setTab("about")}
        >
          {text.app.settingsAbout}
        </button>
      </div>

      {tab === "general" && (
        <div className="settings-body">
          <div className="settings-section">
            <p className="settings-label">{text.app.uiLanguage}</p>
            <p className="settings-hint">{text.app.uiLanguageHint}</p>
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

          <div className="settings-section">
            <p className="settings-label">{text.app.theme}</p>
            <p className="settings-hint">{text.app.themeHint}</p>
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
        </div>
      )}

      {tab === "about" && (
        <div className="settings-body">
          <div className="settings-section">
            <p className="settings-label">{text.app.title}</p>
            <p className="settings-hint">{text.app.aboutDesc}</p>
            <p className="about-version">
              {text.app.version} v{version}
            </p>
          </div>
        </div>
      )}
    </section>
  );
}
