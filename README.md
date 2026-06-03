# CC Sync

> 一处维护，按需同步：把多个 Agent 的说明文件、skills 和 docs 统一成输入源，
> 再同步到你勾选的目标里。**只同步你真正在用的那几个 skill，避免把一大堆低频技能
> 塞进上下文、浪费 token、干扰模型选择。**

CC Sync 是一个本地桌面同步工具（Tauri + React 界面，Python 同步引擎驱动）。
它适合同时使用 **Claude Code / Codex / Gemini**、并且长期沉淀 Agent 工作流的项目：
排查手册、运行说明、业务文档、常用命令都可以作为上下文资产统一维护，而不是散落在
各个 Agent 的私有目录里。

## 它解决什么问题

平时用 Agent，最难的不是"不会写配置"，而是内容会慢慢分散、失控：

| 痛点 | 真实表现 | 后果 |
| --- | --- | --- |
| 说明文档漂移 | Claude 用 `CLAUDE.md`、Codex 用 `AGENTS.md`、Gemini 用 `GEMINI.md`，改了一份忘了另一份 | 同一项目不同 Agent 理解不一致，排查成本上升 |
| skills 分散 | 一些技能在 `.claude/skills`，一些在 `.codex/skills`，本地改了一个忘了同步 | 沉淀过的工作流换个 Agent 又失效 |
| 无用 skills 挤占上下文 | 技能目录里攒了几十个，日常高频的可能只有几个 | 低频技能被一起扫到，浪费 token、增加噪音，模型更难命中该用的 skill |
| docs 复制混乱 | 排查手册、运行说明被手动复制到多个目标目录 | 重复文件越来越多，不知道哪份才是最新 |

CC Sync 做的事情是把这些内容统一成"输入源"，再同步到你勾选的 Agent 目标里。

## 核心理念：只同步你真正在用的

工具最大的价值不是"少点手动复制"，而是**帮模型聚焦**：

- 把常用技能收敛成一个小集合（通用技能），一次勾选，所有目标都收到。
- 某个 Agent 需要的特殊能力，单独追加为它的"额外技能"。
- 某个 Agent 不适合的技能，单独排除。
- 同名技能只保留第一个输入源，重复项在预览里提示并忽略。

结果是每个 Agent 的技能目录里只有真正会用到的内容，token 和上下文都更干净。

## 同步模型

配置文件使用 v3 结构：

- `sources.md_files`：多个说明文件，按配置顺序合并到目标说明文件。
- `sources.skills_dirs`：多个技能源目录，只接受包含 `SKILL.md` 的直接子目录。
- `sources.docs_dirs`：多个文档源目录，读取每个源目录下的直接子项。
- `sync.targets`：当前启用的同步目标，例如 Codex、Gemini、Claude Code。
- `endpoints.*.skills_mode` / `endpoints.*.docs_mode`：技能目录和文档目录可以分别选择
  目录映射（junction）、符号链接或复制。

skills 和 docs 都按名称去重。同名项保留第一个输入源，后续重复项会在同步预览里提示并忽略。

> ⚠️ **说明文件的命名替换是「全文字面替换」**：把 md 同步到 Codex/Gemini 时，`replacements`
> 里的规则（如 `.claude` → `.codex`、`Claude Code` → `Codex`）会替换合并后正文里的**所有**
> 出现位置，包括叙述文字、代码块和 URL。如果正文里有不想被替换的字面量，请留意这一点。

CC Sync 会在配置文件同目录维护 `cc-sync.state.json`，记录自己上一次写入过的 skills/docs。
后续如果从配置里取消某个已管理项，只会移除这个状态文件中记录过、且当前没有被手动改动的项；
目标目录里手动添加的内容不会被清理。

## 安装与运行

### 便携包运行

便携包是自包含的：保持整个文件夹完整，双击启动即可。目录里至少应包含：

```text
CC Sync/
├─ CC Sync.exe
├─ cc-sync.config.json
├─ sync_agents.py
├─ sync_config.py
├─ sync_from_claude.py
├─ python\          # 内嵌 Python 运行时
└─ README.txt
```

> 不要只复制 `CC Sync.exe`。`python\` 目录、同步脚本和配置文件必须和 exe 放在一起。
> 文件夹放在哪里都可以——它通过界面里选择的工作区来决定同步到哪个项目。

### 源码开发运行

```powershell
cd cc-sync
npm install
npm run tauri:dev
```

### 源码打包

```powershell
cd cc-sync
npm install
npm run tauri:build
```

生成便携包：

```powershell
.\build-portable.ps1
```

打包结果在：

```text
portable-dist\CC Sync\
portable-dist\CC Sync portable.zip
```

## 推荐使用流程

第一次使用不要一次性打开所有目标，建议按这个顺序跑通：

1. 启动 `CC Sync.exe`。
2. 选择要同步的项目工作区（你的项目根目录）。
3. 先添加一个说明文件源，例如项目级 Agent 说明。
4. 先只启用一个目标，例如 Codex。
5. 点击执行同步，先看预览计划。
6. 确认 md、skills、docs 的数量和目标路径都对，再确认写盘。
7. 再逐步加入 skills 源、docs 源和其他目标；如有需要，在目标的「高级配置」里分别调整
   技能和文档的同步模式。

这样可以先验证路径和预览结果，避免第一次配置时把范围铺得太大。

## 什么内容值得同步

最值得同步的不是临时文件，而是已经被反复验证过的工作流资产：

- 日志/数据查询、问题排查类 skill。
- 报表、脚本、运行流程的说明。
- 调试、复现、回放类说明。
- 各业务模块的排查手册、模块地图。
- 项目级约定、常用命令、验证方式。

建议把"稳定复用"的内容放进共享输入源，把"只对某个 Agent 有用"的内容放到目标的额外 skill 里。

## 安全边界

同步前会先生成预览。存在错误时不会执行写盘操作。

路径检查会阻止以下情况：

- source path 等于 target path。
- target path 位于 source path 内部。
- source path 位于 target path 内部。

这能挡住最危险的目录嵌套问题，尤其适合目录层级深、工具目录多的项目。

## 命令行验证

在源码仓库里验证：

```powershell
python -m py_compile sync_config.py sync_agents.py sync_from_claude.py
npm run build
cd src-tauri
cargo check
```

如果本地有真实配置，还可以先跑 dry-run：

```powershell
python .\sync_agents.py --config .\cc-sync.config.json --scope all --dry-run --json
```

在便携目录里验证：

```powershell
.\python\bin\python.exe .\sync_agents.py --config .\cc-sync.config.json --scope all --dry-run --json
```

常用检查：

```powershell
.\python\bin\python.exe .\sync_agents.py --config .\cc-sync.config.json --list-skills --json
.\python\bin\python.exe .\sync_agents.py --config .\cc-sync.config.json --scope md --target codex --dry-run --json
```

## 目录说明

- `sync_agents.py`：同步引擎，行为以它为准。
- `sync_from_claude.py`：旧入口兼容 wrapper，不作为新逻辑入口。
- `sync_config.py`：默认配置与配置迁移逻辑。
- `cc-sync.config.example.json`：公开示例配置。
- `cc-sync.config.json`：本地真实配置，不提交。
- `cc-sync.state.json`：本地同步状态，不提交。
- `src\`：React UI。
- `src-tauri\`：Tauri 桌面壳。
- `build-portable.ps1`：生成便携运行目录和 zip。

## 不要提交的本地状态

这些内容只属于本机运行或构建产物，不要提交：

- `cc-sync.config.json`
- `cc-sync.state.json`
- `.codegraph\`
- `dist\`
- `src-tauri\target\`
- `portable-dist\`
- `node_modules\`
- `__pycache__\`
- `*.tsbuildinfo`
