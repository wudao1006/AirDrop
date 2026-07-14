# AirDrop

一个以设备独立剪贴板槽位为核心的局域网协作工具。目前目标平台为 Windows、macOS、Linux 和 Android；iOS 暂不考虑。桌面端负责长期在线，Android 只保证应用前台期间实时同步。

## 当前已实现

- Tauri 2 + React + TypeScript 桌面程序结构。
- 桌面与 Android 共享的 Tauri 2、React、Snapshot 和 Import 模型。
- 首页、设备、同步组、剪贴板、传输和设置页面。
- Clipboard Switcher：按设备展示最新槽位、格式、同步组和可用状态。
- `ready`、`metadata_only`、`stale`、`blocked` 等状态与按需获取/二次确认交互。
- 用户明确确认后写入本机文本剪贴板；载入或更新远端槽位不会写入。
- Android 原生 Tauri 工程、ARM64 Debug APK 构建，以及前台实时、后台暂停、恢复重连的生命周期状态。
- Android 首版只声明并处理纯文本与 URL；可信 Hello 会交换剪贴板能力，避免 v0.1.8 设备向 Android 主动发送富文本、图片或文件正文。
- Android 进入后台后停止读取剪贴板、关闭 mDNS 会话和现有 QUIC 连接；回到前台重新开放传输、发现设备并推送完整 Snapshot。
- Android Activity 在前台持有 Wi-Fi MulticastLock，后台释放，以支持 `_localdrop._udp.local` 发现而不申请永久前台服务。
- Android 底部导航、safe-area、44px 触控目标和全屏选择器布局。
- 用户明确点击后读取并发布当前文本剪贴板。
- 桌面 Tauri 通过 IPC 连接进程内 Rust 状态服务，Snapshot、暂停状态、设置和剪贴板捕获由 Rust 持有并通过事件推送。
- 桌面 Rust 服务在独立线程监听文本剪贴板变化，避免剪贴板 API 阻塞 UI；界面不再填充伪造设备。
- 持久化 Ed25519 设备身份，使用 mDNS 自动发现同一局域网设备。
- 通过六位短验证码由双方显式确认配对；可信设备身份和证书固定保存在本机 SQLite。
- 使用 Owner Ed25519 签名的同步组清单管理成员、发布/订阅方向和文本、图片、HTML/RTF、文件策略；直接配对本身不授予剪贴板读取权限。
- 同步组邀请需要目标设备确认；移出、退出、撤销和 Owner 删除均会收紧现有槽位，离线成员重连后会补收清单或永久删除声明。
- 使用强制双方出示 Ed25519 设备证书的双向 TLS 1.3/QUIC 传输文本、HTML/RTF、图片和文件剪贴板槽位；图片使用有界单向流，文件使用带偏移协商与最终确认的可续传双向流，正文不嵌入控制 JSON。
- Windows/Linux 通过原生剪贴板适配读取和写入 HTML、RTF 与文件列表；富文本同时保留纯文本降级格式。
- 独立识别单一 HTTP(S)、FTP 与 mailto URL，本机可以在保留组级“文本/URL”边界的同时单独关闭 URL 发布、订阅和缓存。
- 文件剪贴板传输的是文件和目录的受控静态快照，不是无效的源机器路径：逐项限制路径、数量和总大小，拒绝符号链接，校验 SHA-256 后才允许显式取用；流中断或断线重连时按已落盘文件偏移续传。
- 远端复制只更新独立设备槽位；必须经过“使用”和最终确认才写入本机系统剪贴板，并抑制导入后的反馈回环。
- 每台可信设备可以独立停用或恢复剪贴板同步；解除配对会写入本机撤销表并立即断开现有连接。
- Windows Credential Manager 或 Linux Secret Service 可用时，远端文本使用 XChaCha20-Poly1305 加密缓存 24 小时；凭据存储不可用时自动退化为仅内存缓存。
- SQLite WAL、单实例、每日文件日志、日志保留清理、崩溃记录，以及 Windows/Linux GitHub Actions 构建验证。
- 明暗主题、键盘焦点、语义状态颜色和响应式桌面布局。

桌面本地状态、设备发现、配对、签名同步组以及文本、URL、HTML/RTF、图片、可续传文件剪贴板交换已经使用真实 Rust 服务。当前实现仍属于 v0.1：普通文件投送中心、安全的应用私有格式注册表、真实 Windows/Linux 多机矩阵和 Windows 正式代码签名仍未完成。浏览器预览继续使用隔离的演示客户端，仅用于 UI 开发，不具备局域网能力。

从 v0.1.7 起，QUIC 握手强制客户端设备证书；不再支持与 v0.1.6 及更早版本混合运行，特定拨号方向即使偶然完成握手也不具备新的双向认证保证。已有配对设备双方升级后会继续使用原长期身份，不需要重新配对。

从 v0.1.8 起，仓库包含可构建的 Android 原生工程。Android 采用前台实时的文本/URL 客户端模型，不宣称后台常驻，也不宣称支持富文本、图片和文件剪贴板。当前已完成编译与 APK 权限核验，但仍需要 Android 真机与 Windows/Linux 设备完成同一 Wi-Fi 下的发现、配对、切后台和双向文本互传矩阵。

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

Android 使用同一个 `app/` Tauri 工程，原生项目保存在 `app/src-tauri/gen/android/`。应用在前台时恢复连接并实时更新设备槽位，进入后台后停止剪贴板读取、mDNS 和现有 QUIC 会话，重新前台时恢复服务并获取完整 Snapshot。生命周期变化本身不会读取或写入系统剪贴板。

生成和运行 Android 原生工程需要：

- 受 Tauri 支持的 JDK。
- Android SDK、platform-tools、build-tools 和 NDK。
- Rust 工具链及 Android target。
- 正确设置 `JAVA_HOME`、`ANDROID_HOME` 和 `NDK_HOME`。

首次克隆若仓库已经包含 `gen/android`，无需重复初始化，直接执行：

```bash
cd app
npm run android:dev
```

生成与 CI 一致、可直接安装的 ARM64 Debug APK：

```bash
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_STRIP=debuginfo \
  npm run android:build -- --debug --target aarch64 --apk --ci
```

需要重新生成工程时才执行 `npm run android:init`。当前主机已经使用 JDK 17、Android SDK Platform 36、Build Tools 36、NDK 27 和 Rust Android targets 成功构建 ARM64 Debug APK。构建产物位于 `app/src-tauri/gen/android/app/build/outputs/apk/`，不会提交到仓库；主分支 CI 也会上传同配置的 APK artifact。正式 Android 分发仍需要配置稳定的发布签名密钥。仓库不会包含 iOS 工程和依赖。

## 项目结构

```text
app/
├── src/
│   ├── app/                  # 共享应用外壳
│   ├── features/             # 共享页面与业务组件
│   └── platform/
│       ├── desktop/          # 桌面标题栏、导航与窗口交互
│       └── android/          # Android 导航与生命周期
├── src-tauri/src/
    ├── core/                 # 跨平台状态、同步模型与 Tauri 命令
    └── platform/
        ├── windows/          # Windows 原生适配入口
        ├── macos/            # macOS 原生适配入口
        ├── linux/            # Linux/Wayland/X11 适配入口
        └── android/          # Android 权限与前台生命周期适配入口
└── src-tauri/gen/android/    # Tauri Android/Gradle 原生工程
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

推送 `v*` 标签后，GitHub Actions 会创建正式 GitHub Release 并分别构建：

- Windows：NSIS `.exe` 与 MSI 安装包。
- Linux：`.deb` 与 `.AppImage`。

发布前应在真实的两台设备上完成配对、重连、连续复制、暂停/恢复和显式导入验收。Windows 正式分发还需要配置代码签名证书；没有签名的安装包会触发 SmartScreen 提示。
