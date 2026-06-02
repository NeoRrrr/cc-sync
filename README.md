# CC Sync

`CC Sync` 是一个本地桌面 GUI，用来把多个 agent 说明文件、技能目录和文档目录汇总后，同步到一个或多个 agent 配置目录。

它的核心模型是“多输入源 -> 多同步目标”：输入源负责收集 Markdown、skills、docs，目标负责决定写入哪个 agent，以及用什么链接/复制模式落盘。

## 当前能力

- 编辑并保存 `cc-sync.config.json`
- 配置工作区、同步范围、输入源、目标启停、同步模式与文本替换
- 添加多个说明文件并按顺序合并
- 添加多个技能源目录，校验 `SKILL.md` 后按文件夹名去重
- 添加多个文档源目录，按直接子项名称去重
- 用勾选方式维护通用技能、额外技能、排除技能
- 校验 `SKILL.md` 并显示可选 skill 的名称、描述和来源路径
- 预览同步计划，并阻止源目录与目标目录互相嵌套的危险路径
- 执行真实同步并查看执行记录
- 通过 Tauri 打包为桌面应用

## 同步模型

配置文件使用 v3 结构：

- `sources.md_files`：多个说明文件，按配置顺序合并到目标说明文件
- `sources.skills_dirs`：多个技能源目录，只接受包含 `SKILL.md` 的直接子目录
- `sources.docs_dirs`：多个文档源目录，读取每个源目录下的直接子项
- `sync.targets`：启用的同步目标，例如 Codex、Gemini

skills 与 docs 都按名称去重；同名项保留第一个输入源，后续重复项会在同步预览里提示并忽略。

## 安全边界

同步前会先生成预览。存在错误时不会执行写盘操作。

路径检查会阻止以下情况：

- 源路径等于目标路径
- 目标路径位于源路径内部
- 源路径位于目标路径内部

## 目录说明

- `sync_agents.py`：同步引擎
- `sync_from_claude.py`：旧入口兼容 wrapper，新调用请使用 `sync_agents.py`
- `sync_config.py`：默认配置与配置加载逻辑
- `cc-sync.config.json`：当前本地配置
- `src/`：React UI
- `src-tauri/`：Tauri 桌面壳
- `run.bat`：一键启动本地桌面调试
- `build.bat`：一键打包桌面安装包

## 本地运行

优先直接用 bat：

- 双击 `run.bat`

或者命令行执行：

```powershell
cd C:\Users\Admin\Dawn\Trunk\tools\AI\agent_sync_gui
npm install
npm run tauri:dev
```

## 打包

优先直接用 bat：

- 双击 `build.bat`

或者命令行执行：

```powershell
cd C:\Users\Admin\Dawn\Trunk\tools\AI\agent_sync_gui
npm install
npm run tauri:build
```

打包输出默认在：

- `src-tauri/target/release/bundle/msi/`
- `src-tauri/target/release/bundle/nsis/`

## Python 命令行验证

使用项目 Python：

```powershell
C:\Users\Admin\Dawn\Trunk\tools\python311\Windows-AMD64\bin\python.exe .\sync_agents.py --config .\cc-sync.config.json --scope all --json
```

常用示例：

```powershell
C:\Users\Admin\Dawn\Trunk\tools\python311\Windows-AMD64\bin\python.exe .\sync_agents.py --config .\cc-sync.config.json --scope md --target codex --json
C:\Users\Admin\Dawn\Trunk\tools\python311\Windows-AMD64\bin\python.exe .\sync_agents.py --config .\cc-sync.config.json --list-skills --json
```

## 仓库说明

仓库只保留源码、锁文件和示例配置。依赖与构建产物由本地命令重新生成：

- `node_modules/`
- `dist/`
- `src-tauri/target/`
- `portable-dist/`
- `__pycache__/`
- `*.tsbuildinfo`

本地真实配置使用 `cc-sync.config.json`，该文件不提交；公开仓库使用 `cc-sync.config.example.json` 作为模板。
