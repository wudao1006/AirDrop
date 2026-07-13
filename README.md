# AirDrop

一个以设备独立剪贴板槽位为核心的局域网协作工具。目前目标平台为 Windows、macOS、Linux 和 Android；iOS 暂不考虑。桌面端负责长期在线，Android 只保证应用前台期间实时同步。

## 当前已实现

- Tauri 2 + React + TypeScript 桌面程序结构。
- 桌面与 Android 共享的 Tauri 2、React、Snapshot 和 Import 模型。
- 首页、设备、同步组、剪贴板、传输和设置页面。
- Clipboard Switcher：按设备展示最新槽位、格式、同步组和可用状态。
- `ready`、`metadata_only`、`stale`、`blocked` 等状态与按需获取/二次确认交互。
- 用户明确确认后写入本机文本剪贴板；载入或更新远端槽位不会写入。
- Android 前台实时、后台暂停、恢复重连的生命周期状态。
- Android 底部导航、safe-area、44px 触控目标和全屏选择器布局。
- 用户明确点击后读取并发布当前文本剪贴板。
- 桌面 Tauri 通过 IPC 连接进程内 Rust 状态服务，Snapshot、暂停状态、设置和剪贴板捕获由 Rust 持有并通过事件推送。
- 桌面 Rust 服务在独立线程监听文本剪贴板变化，避免剪贴板 API 阻塞 UI；界面不再填充伪造设备。
- 持久化 Ed25519 设备身份，使用 mDNS 自动发现同一局域网设备。
- 通过六位短验证码由双方显式确认配对；可信设备身份和证书固定保存在本机 SQLite。
- 使用 TLS 1.3/QUIC 传输可信设备的文本剪贴板槽位，连接恢复后补发本次运行期间最新槽位。
- 远端复制只更新独立设备槽位；必须经过“使用”和最终确认才写入本机系统剪贴板，并抑制导入后的反馈回环。
- 每台可信设备可以独立停用或恢复剪贴板同步；解除配对会写入本机撤销表并立即断开现有连接。
- Windows Credential Manager 或 Linux Secret Service 可用时，远端文本使用 XChaCha20-Poly1305 加密缓存 24 小时；凭据存储不可用时自动退化为仅内存缓存。
- SQLite WAL、单实例、每日文件日志、日志保留清理、崩溃记录，以及 Windows/Linux GitHub Actions 构建验证。
- 明暗主题、键盘焦点、语义状态颜色和响应式桌面布局。

桌面本地状态、设备发现、配对和直接配对设备间的纯文本槽位交换已经使用真实 Rust 服务。当前实现仍属于 v0.1：同步组授权、富文本/图片/文件格式、文件传输和完整的协议范围恢复机制尚未完成。浏览器预览继续使用隔离的演示客户端，仅用于 UI 开发，不具备局域网能力。

## 运行前端预览

```bash
cd app
npm install
npm run dev
```

浏览器预览用于开发 UI。浏览器允许剪贴板权限时可以测试文本写入，但正式桌面能力应使用 Tauri。

Android 触控布局可以在没有 Android SDK 时预览：

```bash
cd app
npm run dev:android
```

## 运行桌面程序

先安装 Rust 与 Tauri 在当前平台要求的系统依赖。Linux 通常需要 WebKitGTK、GTK、AppIndicator 和 SVG 开发包，具体包名随发行版变化。

```bash
cd app
npm install
npm run tauri dev
```

当前开发机已经安装 Rust、WebKitGTK、GTK、AppIndicator、librsvg 和相关开发库，可以直接编译 Linux 原生程序。已验证产物位于 `app/release/`。

## Android

Android 使用同一个 `app/` Tauri 工程。应用在前台时恢复连接并实时更新设备槽位，进入后台后允许系统暂停，重新前台时重新获取完整 Snapshot。生命周期变化不会读取或写入系统剪贴板。

生成和运行 Android 原生工程需要：

- 受 Tauri 支持的 JDK。
- Android SDK、platform-tools、build-tools 和 NDK。
- Rust 工具链及 Android target。
- 正确设置 `JAVA_HOME`、`ANDROID_HOME` 和 `NDK_HOME`。

工具链准备完成后执行：

```bash
cd app
npm run android:init
npm run android:dev
```

生产构建使用：

```bash
npm run android:build
```

当前主机已有 Rust，但仍没有 Java 和 Android SDK/NDK，因此尚未生成 `gen/android` 或 APK。仓库不会包含 iOS 工程和依赖。

## 项目结构

```text
app/
├── src/
│   ├── app/                  # 共享应用外壳
│   ├── features/             # 共享页面与业务组件
│   └── platform/
│       ├── desktop/          # 桌面标题栏、导航与窗口交互
│       └── android/          # Android 导航与生命周期
└── src-tauri/src/
    ├── core/                 # 跨平台状态、同步模型与 Tauri 命令
    └── platform/
        ├── windows/          # Windows 原生适配入口
        ├── macos/            # macOS 原生适配入口
        ├── linux/            # Linux/Wayland/X11 适配入口
        └── android/          # Android 权限与前台生命周期适配入口
```

Windows、macOS 和 Linux 共享 React 界面与 Rust Core，仅把剪贴板、窗口材质、托盘、权限和系统集成放入各自的平台目录。Android 共享协议与状态模型，但保留独立的生命周期和触控导航适配。

## 验证

```bash
cd app
npm run typecheck
npm test -- --run
npm run build
```

完整协议与安全边界见 [DESIGN.md](DESIGN.md)。实现计划见 [桌面 UI MVP 计划](docs/superpowers/plans/2026-07-11-desktop-ui-mvp.md)。

## Windows 与 Linux 发布

推送 `v*` 标签后，GitHub Actions 会创建草稿预发布并分别构建：

- Windows：NSIS `.exe` 与 MSI 安装包。
- Linux：`.deb` 与 `.AppImage`。

发布前应在真实的两台设备上完成配对、重连、连续复制、暂停/恢复和显式导入验收。Windows 正式分发还需要配置代码签名证书；没有签名的安装包会触发 SmartScreen 提示。
