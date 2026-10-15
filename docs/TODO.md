# TODO

## Phase 4：设置与模型管理

- [ ] **P4-WINDOW-MODE（issue #1130）**：Windows 普通窗口、可配置纯色背景、OBS 窗口捕获与抠图、旧 v1 配置读取、禁用颜色预览和统一客户区等比例缩放已实现。依赖 ADR-0057、ADR-0054；协议见 ADR-0089。退出条件：配置/UI/runtime/HWND/D3D11 回归、`just check`、Windows product smoke、OBS BitBlt/WGC/抠图验收，以及 Windows 10 和 125/150/200% 跨屏实机验证。当前 Windows 11 OBS 与自动化验收已有证据，缺少的 Windows 10、跨屏与设置布局验收保持未勾选；macOS 不在本任务范围。证据见 `docs/benchmark/windows-window-mode-2026-10-08.md`。
- [ ] **P4-MOTION-OVERLAP（issue #1107）**：默认关闭的动作叠加开关、旧 v1 文档读取、独立计时/淡出/停止和按启动顺序求值已实现，标准模型跨组重复资源重播有自动化回归。依赖 ADR-0061、ADR-0012（动作音效）；协议见 ADR-0088。退出条件：workspace 与独立 config-store contract、`just check`、双平台 CI smoke，以及 Windows/macOS 实机多动作叠加、跨组重播和 800×600/DPI/Retina 目视验收。尚缺的平台实机证据保持未勾选。
- [ ] **P4-NESTED-MODEL-IMPORT（issue #1105）**：嵌套模型发现、单模型自动继续、多模型复用选择弹框、模式选择及顺序导入已实现。依赖 ADR-0037、ADR-0055、ADR-0059；协议与扫描边界见 ADR-0087。退出条件：自动化扫描/服务/GPUI 交互回归、`just check`、macOS smoke，以及 Windows/macOS 实机文件夹选择与拖入、800×600 和 DPI/Retina 目视验收。缺少的平台实机证据仍保持未勾选，不由另一平台编译或 headless 测试代替。


## Together：Windows 双人联机（ADR-0090）

- [x] **TOGETHER-UPDATES（ADR-0091）**：独立公开仓库的预发布版检查与下载页入口，复用现有定时检查设置；版本/来源/错误/禁止安装回归、匿名网络检查、`just check`（1209 passed，6 ignored）、Windows Together 与设置窗口 smoke 已通过。仅提供检查和手动下载，不自动安装；随 0.1.1 打包交付，证据见 `docs/bongocat-together-plan.md`。

- [ ] **TOGETHER-WINDOWS**：Iroh 加密双人房间、GPUI 联机页、授权键鼠/独立文字、好友原生 overlay、模型 ID 与显示提示已实现，完整阶段证据见 `docs/bongocat-together-plan.md`。联机页已改用标准设置分组与整行长字段，中英文小窗口的边界、换行和页面滚动已有 GPUI 布局回归；昵称输入框宽度塌缩已修复，并补充真实点击、编辑与重绘保留的交互回归。DPI/跨屏目视验收仍未完成。依赖原输入可靠性、Cubism 和 Windows 发布门禁；退出条件包括 `just check`、原生 Together/单机 smoke、不同公网双机直连及中继、真实键鼠/前台白名单/锁屏/睡眠/UAC、中文输入法、DPI 与跨屏、性能测量、8 小时 soak 和安装升级卸载验收。未取得外部证据前不勾选整体完成，不发布受限安装包。
