# CC Sync 食用指南

CC Sync 是一个本地桌面同步工具，核心价值很简单：

> 一处维护，不用手动同步；自己管理实际在用的 skills、docs 和说明文件。

它适合像 Dawn 这种长期沉淀 Agent 工作流的项目：ES 排查、留存报表、replay 调试、业务说明、常用命令都可以作为上下文资产维护，而不是散落在不同 Agent 的目录里。

## 放在 Dawn 里的位置

当前推荐放置位置：

```powershell
C:\Users\Admin\Dawn\Trunk\tools\AI\CC Sync
```

在 Dawn 项目内，相对位置就是：

```text
tools\AI\CC Sync
```

这个目录是便携运行目录，里面应该至少包含：

- `CC Sync.exe`
- `cc-sync.config.json`
- `sync_agents.py`
- `sync_config.py`
- `sync_from_claude.py`
- `python\`
- `README.txt`

不要只复制 `CC Sync.exe`，Python 目录、同步脚本和配置文件都要跟 exe 放在一起。

## 它解决什么问题

你平时用 Agent 时，最容易遇到的不是“不会写配置”，而是这些内容会慢慢分散：

- `AGENTS.md`、`CLAUDE.md`、`GEMINI.md` 改了其中一份，另一份忘了同步。
- 某个 skill 在 Codex 能用，换到 Claude Code 或 Gemini 又找不到。
- 排查手册、运行说明、业务文档被复制成多份，不知道哪份才是最新。
- 技能越攒越多，但真正常用的只有一部分，每次同步都靠记忆挑。

CC Sync 做的事情是把这些内容统一成“输入源”，再同步到你勾选的 Agent 目标里。

## 同步模型

配置文件使用 v3 结构：

- `sources.md_files`：多个说明文件，按配置顺序合并到目标说明文件。
- `sources.skills_dirs`：多个技能源目录，只接受包含 `SKILL.md` 的直接子目录。
- `sources.docs_dirs`：多个文档源目录，读取每个源目录下的直接子项。
- `sync.targets`：当前启用的同步目标，例如 Codex、Gemini、Claude Code。
- `endpoints.*.skills_mode` / `endpoints.*.docs_mode`：技能目录和文档目录可以分别选择目录映射、符号链接或复制。

skills 和 docs 都按名称去重。同名项保留第一个输入源，后续重复项会在同步预览里提示并忽略。

CC Sync 会在配置文件同目录维护 `cc-sync.state.json`，记录自己上一次写入过的 skills/docs。后续如果从配置里取消某个已管理项，只会移除这个状态文件中记录过、且当前没有被手动改动的项；目标目录里手动添加的内容不会被清理。

## 推荐使用方式

第一次使用不要一次性打开所有目标，建议按这个顺序跑通：

1. 打开 `CC Sync.exe`。
2. 选择 Dawn 工作区：`C:\Users\Admin\Dawn\Trunk`。
3. 先添加一个说明文件源，例如项目级 Agent 说明。
4. 先只启用一个目标，例如 Codex。
5. 点击执行同步，先看预览计划。
6. 确认 md、skills、docs 数量和目标路径都对，再确认写盘。
7. 再逐步加入 skills 源、docs 源和其他目标；如有需要，在目标的「高级配置」里分别调整技能和文档同步模式。

这样可以先验证路径和预览结果，避免第一次配置时把范围铺得太大。

## Dawn 场景下怎么用

对 Dawn 来说，最值得同步的不是一堆临时文件，而是已经被反复验证过的工作流资产：

- ES 查询和日志排查 skill。
- 海外次留、留存报表相关脚本说明。
- replay / video 调试说明。
- 房间、战斗、公会等业务模块排查手册。
- 项目级约定、常用命令、验证方式。

建议把“稳定复用”的内容放进共享输入源，把“只对某个 Agent 有用”的内容放到目标的额外 skill 里。

## 安全边界

同步前会先生成预览。存在错误时不会执行写盘操作。

路径检查会阻止以下情况：

- source path 等于 target path。
- target path 位于 source path 内部。
- source path 位于 target path 内部。

这能挡住最危险的目录嵌套问题，尤其适合 Dawn 这种目录层级比较深、工具目录比较多的项目。

## 运行方式

### 便携包运行

在 Dawn 目录下直接双击：

```powershell
C:\Users\Admin\Dawn\Trunk\tools\AI\CC Sync\CC Sync.exe
```

### 源码开发运行

如果是在源码仓库里开发：

```powershell
cd C:\Users\Admin\Documents\cc-sync
npm install
npm run tauri:dev
```

### 源码打包

```powershell
cd C:\Users\Admin\Documents\cc-sync
npm install
npm run tauri:build
```

生成便携包：

```powershell
cd C:\Users\Admin\Documents\cc-sync
.\build-portable.ps1
```

打包结果在：

```text
portable-dist\CC Sync\
portable-dist\CC Sync portable.zip
```

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

在 Dawn 便携目录里验证：

```powershell
cd "C:\Users\Admin\Dawn\Trunk\tools\AI\CC Sync"
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
