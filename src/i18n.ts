export type Language = "zh-CN" | "en";

const LANGUAGE_STORAGE_KEY = "cc-sync-language";

export function loadLanguage(): Language {
  try {
    const raw = localStorage.getItem(LANGUAGE_STORAGE_KEY);
    if (raw === "zh-CN" || raw === "en") {
      return raw;
    }
  } catch {
    // localStorage 不可用时退回默认
  }
  return "zh-CN";
}

export function saveLanguage(language: Language): void {
  try {
    localStorage.setItem(LANGUAGE_STORAGE_KEY, language);
  } catch {
    // 忽略写入失败
  }
}

export type Dictionary = {
  app: {
    title: string;
    loading: string;
    unsaved: string;
    workspace: string;
    workspaceHint: string;
    scope: string;
    saveConfig: string;
    runSync: string;
    skillCount: string;
    extraSkillCount: string;
    noSkills: string;
    syncErrors: string;
    syncWarnings: string;
    enabled: string;
    disabled: string;
    mode: string;
    mdTarget: string;
    skillsDir: string;
    docsDir: string;
    commonSkills: string;
    activeCommonSkills: string;
    inputSources: string;
    configure: string;
    mdSources: string;
    skillSourceDirs: string;
    docsSourceDirs: string;
    chooseFile: string;
    remove: string;
    add: string;
    noSources: string;
    pathPlaceholder: string;
    extraSkills: string;
    excludeSkills: string;
    skillSources: string;
    selectedSkills: string;
    executionLog: string;
    entries: string;
    language: string;
    advancedConfig: string;
    replacements: string;
    openLocation: string;
    saveSucceeded: string;
    saveFailed: string;
    syncSuccessTitle: string;
    emptyLog: string;
    close: string;
    replacementPlaceholder: string;
    confirmSyncTitle: string;
    willSync: string;
    confirmRun: string;
    cancel: string;
    previewHasErrors: string;
    settings: string;
    back: string;
    settingsGeneral: string;
    settingsAbout: string;
    uiLanguage: string;
    uiLanguageHint: string;
    theme: string;
    themeHint: string;
    themeLight: string;
    themeDark: string;
    themeSystem: string;
    version: string;
    aboutDesc: string;
    help: string;
    helpTitle: string;
    helpIntro: string;
    helpPoints: string[];
    skills: string;
    skillLink: string;
    skillCopy: string;
    skillBroken: string;
    source: string;
    syncTarget: string;
    notSyncTarget: string;
    chooseFolder: string;
  };
  modeOptions: {
    junction: string;
    symlink: string;
    copy: string;
  };
  logs: {
    configLoaded: (mode: string) => string;
    loadFailed: (message: string) => string;
    syncRequested: (scope: string) => string;
    syncFinished: (success: boolean) => string;
    previewReady: (success: boolean) => string;
    configSaved: string;
  };
};

export const dictionaries: Record<Language, Dictionary> = {
  "zh-CN": {
    app: {
      title: "CC Sync",
      loading: "正在加载 CC Sync...",
      unsaved: "未保存",
      workspace: "工作区",
      workspaceHint: "工具目录在 Trunk/tools/AI/agent_sync_gui，默认工作区应指向 Trunk",
      scope: "范围",
      saveConfig: "保存配置",
      runSync: "执行同步",
      skillCount: "技能数",
      extraSkillCount: "额外技能",
      noSkills: "暂无技能",
      syncErrors: "执行错误",
      syncWarnings: "同步提示",
      enabled: "启用",
      disabled: "停用",
      mode: "模式",
      mdTarget: "说明文件",
      skillsDir: "技能目录",
      docsDir: "文档目录",
      commonSkills: "通用技能",
      activeCommonSkills: "当前通用 SKILL",
      inputSources: "输入源",
      configure: "配置",
      mdSources: "说明文件源",
      skillSourceDirs: "技能源目录",
      docsSourceDirs: "文档源目录",
      chooseFile: "选择文件",
      remove: "删除",
      add: "添加",
      noSources: "暂无输入源",
      pathPlaceholder: "输入文件或目录路径",
      extraSkills: "额外技能",
      excludeSkills: "排除技能",
      skillSources: "来源路径",
      selectedSkills: "已勾选",
      executionLog: "执行记录",
      entries: "条记录",
      language: "语言",
      advancedConfig: "高级配置",
      replacements: "文本替换",
      openLocation: "打开",
      saveSucceeded: "配置已保存",
      saveFailed: "保存失败",
      syncSuccessTitle: "同步完成",
      emptyLog: "还没有执行记录。",
      close: "关闭",
      replacementPlaceholder: "每行一条，格式：源文本 => 目标文本",
      confirmSyncTitle: "确认同步",
      willSync: "将要同步",
      confirmRun: "确认执行",
      cancel: "取消",
      previewHasErrors: "存在错误，请先修复后再执行",
      settings: "设置",
      back: "返回",
      settingsGeneral: "通用",
      settingsAbout: "关于",
      uiLanguage: "界面语言",
      uiLanguageHint: "切换后立即生效。",
      theme: "外观主题",
      themeHint: "选择应用的外观主题，立即生效。",
      themeLight: "浅色",
      themeDark: "深色",
      themeSystem: "跟随系统",
      version: "版本",
      aboutDesc: "把多个说明文件、文档目录与技能目录汇总后，同步到一个或多个 agent 目标。",
      help: "说明",
      helpTitle: "使用说明",
      helpIntro: "CC Sync 把多个输入源汇总后，同步到一个或多个 agent 目标。",
      helpPoints: [
        "输入源：可以配置多个说明文件、多个技能目录、多个文档目录。",
        "目标：每张卡片上的开关决定是否把输入源同步到该 agent。",
        "工作区是项目根目录；范围决定同步内容（全部 / 说明文件 md / 技能 / 文档）。",
        "技能和文档按名称去重；同名项保留第一个输入源，后续重复项会提示并忽略。",
        "通用技能发给所有目标；每个目标可在「高级配置」里追加 / 排除技能，或改该目标的文本替换。",
        "执行同步：先预览将要执行的操作，确认后才写盘；有错误时不会执行。",
        "配置改动自动保存（首次从旧版升级会先备份为 cc-sync.config.v1.bak）。",
      ],
      skills: "技能",
      skillLink: "链接",
      skillCopy: "复制",
      skillBroken: "失效",
      source: "来源",
      syncTarget: "同步目标",
      notSyncTarget: "不同步",
      chooseFolder: "选择",
    },
    modeOptions: {
      junction: "目录映射",
      symlink: "符号链接",
      copy: "复制目录",
    },
    logs: {
      configLoaded: (mode) => `配置已加载，当前模式：${mode}`,
      loadFailed: (message) => `加载配置失败：${message}`,
      syncRequested: (scope) => `请求同步，scope=${scope}`,
      syncFinished: (success) => `同步结束，success=${String(success)}`,
      previewReady: (success) => `已生成预览，success=${String(success)}`,
      configSaved: "配置已保存",
    },
  },
  en: {
    app: {
      title: "CC Sync",
      loading: "Loading CC Sync...",
      unsaved: "Unsaved",
      workspace: "Workspace",
      workspaceHint: "The tool lives in Trunk/tools/AI/agent_sync_gui and the default workspace should point to Trunk",
      scope: "Scope",
      saveConfig: "Save Config",
      runSync: "Run Sync",
      skillCount: "Skills",
      extraSkillCount: "Extra skills",
      noSkills: "No skills",
      syncErrors: "Sync errors",
      syncWarnings: "Sync warnings",
      enabled: "Enabled",
      disabled: "Disabled",
      mode: "Mode",
      mdTarget: "Guide file",
      skillsDir: "Skills dir",
      docsDir: "Docs dir",
      commonSkills: "Common skills",
      activeCommonSkills: "Active common skills",
      inputSources: "Input sources",
      configure: "Configure",
      mdSources: "Guide file sources",
      skillSourceDirs: "Skill source dirs",
      docsSourceDirs: "Docs source dirs",
      chooseFile: "Browse file",
      remove: "Remove",
      add: "Add",
      noSources: "No input sources",
      pathPlaceholder: "Enter a file or directory path",
      extraSkills: "Extra skills",
      excludeSkills: "Exclude skills",
      skillSources: "Source paths",
      selectedSkills: "Selected",
      executionLog: "Execution log",
      entries: "entries",
      language: "Language",
      advancedConfig: "Advanced config",
      replacements: "Text replacements",
      openLocation: "Open",
      saveSucceeded: "Config saved",
      saveFailed: "Save failed",
      syncSuccessTitle: "Sync complete",
      emptyLog: "No execution log yet.",
      close: "Close",
      replacementPlaceholder: "one per line, format: source => target",
      confirmSyncTitle: "Confirm sync",
      willSync: "Will sync",
      confirmRun: "Confirm & run",
      cancel: "Cancel",
      previewHasErrors: "Resolve the errors before running",
      settings: "Settings",
      back: "Back",
      settingsGeneral: "General",
      settingsAbout: "About",
      uiLanguage: "Interface language",
      uiLanguageHint: "Applies immediately.",
      theme: "Theme",
      themeHint: "Choose the app appearance. Applies immediately.",
      themeLight: "Light",
      themeDark: "Dark",
      themeSystem: "System",
      version: "Version",
      aboutDesc: "Merge guide files, docs directories, and skills directories, then sync them to one or more agent targets.",
      help: "Help",
      helpTitle: "How to use",
      helpIntro: "CC Sync merges configured input sources and syncs them to one or more agent targets.",
      helpPoints: [
        "Input sources: configure multiple guide files, skill directories, and docs directories.",
        "Targets: the toggle on each card decides whether the inputs sync to that agent.",
        "Workspace is the project root; Scope picks what to sync (all / guide md / skills / docs).",
        "Skills and docs are deduplicated by name; the first matching source wins and later duplicates are ignored with a warning.",
        "Common skills go to every target; per target you can add/exclude skills or edit target text replacements under Advanced config.",
        "Run sync: previews the operations first and writes only after you confirm; it won't run if there are errors.",
        "Config edits auto-save (the first upgrade from the old layout backs up cc-sync.config.v1.bak).",
      ],
      skills: "Skills",
      skillLink: "Linked",
      skillCopy: "Copied",
      skillBroken: "Broken",
      source: "Source",
      syncTarget: "Target",
      notSyncTarget: "Off",
      chooseFolder: "Browse",
    },
    modeOptions: {
      junction: "Directory link",
      symlink: "Symbolic link",
      copy: "Copy directory",
    },
    logs: {
      configLoaded: (mode) => `config loaded in ${mode} mode`,
      loadFailed: (message) => `load config failed: ${message}`,
      syncRequested: (scope) => `sync requested for scope=${scope}`,
      syncFinished: (success) => `sync finished, success=${String(success)}`,
      previewReady: (success) => `preview ready, success=${String(success)}`,
      configSaved: "config saved",
    },
  },
};
