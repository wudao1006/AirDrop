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
- 明暗主题、键盘焦点、语义状态颜色和响应式桌面布局。

桌面本地状态与本机文本剪贴板捕获已经使用真实 Rust 服务。mDNS、QUIC、身份配对、真实远端剪贴板交换和文件传输仍未实现；未发现或未配对设备时界面保持真实空状态，不再使用演示设备冒充发现结果。浏览器预览仍使用隔离的演示客户端，仅用于 UI 开发。

## 运行前端预览

```bash
cd desktop
npm install
npm run dev
```

浏览器预览用于开发 UI。浏览器允许剪贴板权限时可以测试文本写入，但正式桌面能力应使用 Tauri。

Android 触控布局可以在没有 Android SDK 时预览：

```bash
cd desktop
npm run dev:android
```

## 运行桌面程序

先安装 Rust 与 Tauri 在当前平台要求的系统依赖。Linux 通常需要 WebKitGTK、GTK、AppIndicator 和 SVG 开发包，具体包名随发行版变化。

```bash
cd desktop
npm install
npm run tauri dev
```

当前开发机已经安装 Rust、WebKitGTK、GTK、AppIndicator、librsvg 和相关开发库，可以直接编译 Linux 原生程序。已验证产物位于 `desktop/release/`。

## Android

Android 使用同一个 `desktop/` Tauri 工程；目录名是历史名称，不代表仅支持桌面。应用在前台时恢复连接并实时更新设备槽位，进入后台后允许系统暂停，重新前台时重新获取完整 Snapshot。生命周期变化不会读取或写入系统剪贴板。

生成和运行 Android 原生工程需要：

- 受 Tauri 支持的 JDK。
- Android SDK、platform-tools、build-tools 和 NDK。
- Rust 工具链及 Android target。
- 正确设置 `JAVA_HOME`、`ANDROID_HOME` 和 `NDK_HOME`。

工具链准备完成后执行：

```bash
cd desktop
npm run android:init
npm run android:dev
```

生产构建使用：

```bash
npm run android:build
```

当前主机已有 Rust，但仍没有 Java 和 Android SDK/NDK，因此尚未生成 `gen/android` 或 APK。仓库不会包含 iOS 工程和依赖。

## 验证

```bash
cd desktop
npm run typecheck
npm test -- --run
npm run build
```

完整协议与安全边界见 [DESIGN.md](DESIGN.md)。实现计划见 [桌面 UI MVP 计划](docs/superpowers/plans/2026-07-11-desktop-ui-mvp.md)。
