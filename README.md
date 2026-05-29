# CC Sync

`CC Sync` 是一个本地桌面 GUI，用来驱动当前目录下的 Python 同步引擎，把 Claude 侧的说明文件、技能目录、文档目录同步到其他 agent 目标。

## 当前能力

- 编辑并保存 `cc-sync.config.json`
- 配置工作区、同步范围、目标启停、同步模式与文本替换
- 用勾选方式维护通用技能、额外技能、排除技能
- 显示可选 skill 的来源路径
- 执行真实同步并查看执行记录
- 通过 Tauri 打包为桌面应用

## 目录说明

- `sync_from_claude.py`：同步引擎
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
C:\Users\Admin\Dawn\Trunk\tools\python311\Windows-AMD64\bin\python.exe .\sync_from_claude.py --config .\cc-sync.config.json --scope all --json
```

常用示例：

```powershell
C:\Users\Admin\Dawn\Trunk\tools\python311\Windows-AMD64\bin\python.exe .\sync_from_claude.py --config .\cc-sync.config.json --scope md --target codex --json
C:\Users\Admin\Dawn\Trunk\tools\python311\Windows-AMD64\bin\python.exe .\sync_from_claude.py --config .\cc-sync.config.json --list-skills --json
```

## 提交说明

提交 SVN 前不应提交这些中间产物：

- `node_modules/`
- `dist/`
- `src-tauri/target/`
- `__pycache__/`
- `*.tsbuildinfo`
- 临时验收文件，例如 `tmp_acceptance_*.json`

应保留这些文件：

- `src/`
- `src-tauri/src/`
- `package.json`
- `package-lock.json`
- `Cargo.toml`
- `Cargo.lock`
- `cc-sync.config.json`
- `sync_from_claude.py`
- `sync_config.py`
- `run.bat`
- `build.bat`
