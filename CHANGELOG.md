# Changelog

## 0.3.5 — 2026-11-16

- 大小写不敏感查询不再为每个字面量 term 编译 regex：名字池预存小写名，查询侧用
  `memchr::memmem` 子串扫描；通配符仍走 regex。默认大小写不敏感模式下的短查询会明显变快。
- 默认忽略表扩展：系统目录、缓存、回收站、系统日志、包/依赖/构建目录；并新增按目录名忽略
  （`node_modules`、`target`、`.git`、`__pycache__`、`vendor`、`Cache` 等），可在「包含路径」中覆盖。
- 默认监控根仍为用户主目录；旧版本索引在首次启动时自动重建一次，确保新忽略规则生效。
- README 增加 Everything 风格查询描述（中英双语）。

## 0.3.4 — 2026-11-16

- 仓库整理：仅保留构建必需的 vendored 内核 crates；移除未使用的 CLI/图标工具与上游文档（`doc/`、`PORTING.md` 等）
- README 改为中英双语单文件；关于/打包文案去掉上游品牌，MIT 版权归属保留在 `NOTICE`
- 版本号同步到 0.3.4

## 0.3.3 — 2026-11-16

- 右键菜单「在 Finder 中显示」改为「在文件管理器中显示」（其余语言同步改为通用“文件管理器”）。

## 0.3.2 — 2026-11-16

UOS 实机反馈的 4 个修复：

- 右键菜单在浅色主题下文字与背景同色、无法阅读：菜单显式设置前景色与背景色，hover 项保持对比度。
- 设置弹窗补左上角「×」关闭按钮（原先只能 Esc 或点遮罩退出）。
- 设置页「默认监控路径」改为后端解析的真实主目录（UOS 上显示 `/home/<用户>`）。
- 「关于秒搜」简介精简，并将「Linux 移植版」改为「UOS 移植版」。

## 0.3.1 — 2026-11-16

- 实时监控改为部分覆盖：inotify 额度不足或目录注册失败时保留已装好的 watch（广度优先覆盖浅层目录），
  未覆盖目录由周期全量对账兜底；inotify 始终是主要更新机制。

## 0.3.0 — 2026-11-16

- 预览策略：不做内置预览，双击 / Ctrl+O / 右键统一走系统默认程序
  （`dde-open` → `xdg-open` → `gio open`）；非 macOS 平台移除 Quick Look 入口
- 标签：暂不支持 `tag:`
- 安全加固：CSP 收紧、`freezePrototype`、`withGlobalTauri=false`、移除未使用的 shell allowlist
- 打包：新增 AppStream metainfo、desktop `GenericName`；安装/升级/回滚与 inotify 要求写入文档
- 工程债：`search-cache/src/query.rs`、`cardinal-syntax/src/lib.rs` 按职责拆分（公开 API 不变）

## 0.2.1 — 2026-11-16

- 修复编辑 / 视图菜单为空：Tauri v1 在 Linux 上不支持预定义编辑项，改为自定义菜单并实现动作
- 修复选中结果无法打开：`dde-open` → `xdg-open` → `gio open` 逐级回退；定位改用 DDE `?selectUrl=` 形式

## 0.2.0 — 2026-11-16

首个 秒搜 / Miaosou 信创版本（Linux / 统信 UOS V20 / x86_64）：

- 应用壳改为 Tauri v1（UOS V20 只有 webkit2gtk 4.0）；内核 crate 与索引格式复用
- 真实索引管线：`fswalk` 全量扫描 → `search-cache` 建索引 → zstd slab 持久化到 `~/.config/miaosou/index.db`
- inotify 实时监控；freedesktop 图标与缩略图；剪贴板文件复制；原生菜单/托盘；全局快捷键
- 对外身份统一为 `秒搜` / `Miaosou`（包名 `miaosou`、identifier `cn.miaosou.app`、`MIAOSOU_*` 环境变量）
