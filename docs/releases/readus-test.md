Readus.md 阅读工作区测试版，基于 Quiet.md 0.1.14。此版本用于反馈与验证。

### 下载与安装

- **macOS 13+ / Apple Silicon（M1/M2/M3/M4/M5）**：下载 `readus-dev-macos-arm64.dmg`，打开后把 **Readus.md Dev.app** 拖入 Applications，再从 Applications 启动。
- 也可下载 `readus-dev.zip`，解压后将同一个应用拖入 Applications。
- 包为 ad-hoc 签名，尚未获得 Apple Developer ID 签名与公证；首次打开可能需要在系统设置的“隐私与安全性”中确认。
- `SHA256SUMS.txt` 提供下载校验；`metadata.json` 记录源码提交、构建和 ZIP 校验值。

### 可测试的功能

- 书架：全部、待读、在读、已读，以及继续阅读。
- 书签：当前位置或选中段落添加、全局列表、跳转与删除。
- 阅读记录：当前位置、最远进度、重启恢复，正文保持原样。
- 知识库：工作目录、普通 Markdown 文档链接、@引用和反向引用。
- 简体中文与英文界面，蓝色平面叠书图标。

双击启动默认使用独立目录 `~/Library/Application Support/Readus.md Test/library`；日志位于同级 `logs`。显式设置 `QUIET_MD_HOME` 时使用指定目录。添加工作目录后会直接使用其中的文件，请使用测试副本验证编辑操作。

本测试包启用了开发测试驱动，不作为正式稳定发行版。内置 Agent 对话及 API 管理尚未实现；本包不包含 `quiet` CLI。Windows、Intel Mac 和 Linux 尚未发布本轮可下载测试包，不能用上游 Quiet.md 的安装包代替。

反馈请附复现步骤、系统版本和源码提交，提交到本仓库 Issues。不要附 API 密钥或私人文档。
