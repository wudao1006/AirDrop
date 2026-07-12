# AirDrop：局域网文件投送与多设备剪贴板镜像设计

日期：2026-07-11

## 1. 项目定位

AirDrop 是一个使用 Rust 实现的跨平台局域网设备协作工具。设备在同一局域网内自动发现，经过一次人工确认后建立可信关系；用户可以把若干可信设备加入同步组，实时查看这些设备各自最新的剪贴板内容，并在需要时选择任意设备的剪贴板取用，也可以主动发送文件和目录。

项目目标不是复制苹果 AirDrop 的所有系统级体验，而是实现一个开源、自托管、跨平台、无需中心服务器的日常传输工具，并借此完整练习 Rust 的异步网络、文件流、并发状态、错误处理和跨平台桌面开发。

项目的首要产品体验是“远端剪贴板状态实时可用，但不打扰当前设备”。完成一次配对和同步组配置后，任意成员设备复制内容，其他设备会实时更新该设备的远端剪贴板槽位；本机系统剪贴板保持不变，只有用户在设备选择器中明确选择某个槽位后，内容才会被取入本机剪贴板。

因此，“实时同步”指远端状态、元数据和允许缓存的正文持续更新，不表示自动覆盖接收设备正在使用的系统剪贴板。文件投送仍保留明确的接收与冲突处理流程，不因剪贴板镜像而放宽文件落盘安全边界。

核心体验：

- 打开应用后自动显示同一局域网内的可用设备。
- 第一次建立信任时双方比较短验证码并确认配对。
- 用户从已配对设备中创建同步组，默认互相发布和订阅，也可以按设备配置仅发布、仅订阅或禁用。
- 复制文本、富文本、图片、文件列表等内容后，其他设备的选择器立即更新对应设备卡片，但不改变本机剪贴板。
- 用户通过托盘面板、快捷键或主窗口选择任意设备的最新剪贴板；正文准备完成并再次校验后，才写入本机系统剪贴板。
- 设备休眠或短暂离线后重新连接，可以补发仍在有效期内的每台设备最新槽位，但默认不保存和回放完整剪贴板历史。
- 拖入文件即可主动发送，接收方可接受、拒绝或设置可信设备自动接收。
- 大文件支持断点续传和完整性校验。
- 实时镜像只对显式加入同步组的可信设备生效，并提供内容类型、发布/订阅方向、预取、大小、有效期和敏感内容策略。

## 2. 目标

- 支持 Windows、macOS、Linux 和 Android 之间的局域网直连；Android 第一阶段只保证应用前台期间实时同步。
- 使用 mDNS 自动发现同网段设备。
- 使用成熟的 TLS 1.3/QUIC 实现加密传输，不设计自有加密算法。
- 通过人工比较验证码完成首次设备身份确认，并固定已配对设备身份。
- 支持由多个可信设备组成的同步组，组内数据直连，不依赖中心服务器或常驻中心设备。
- v1.0 支持多设备剪贴板实时镜像和按需取用，默认互相发布/订阅，但绝不自动覆盖接收设备的系统剪贴板。
- 对标准剪贴板表示进行跨平台映射；对应用私有格式只在双方声明兼容且用户允许时透传。
- 使用每设备单写者槽位模型处理多设备同时复制、重复投递、离线补发和网络重连；不同设备之间不竞争同一个“胜出剪贴板”。
- 支持单文件、多文件和目录传输。
- 支持大文件流式传输、取消、失败恢复和断点续传。
- 提供托盘桌面应用、Android 前台客户端和 CLI，共享同一个 Rust Core 与协议模型。
- 所有配对关系、组配置、远端最新槽位和传输状态只保存在参与设备本机，不上传中心服务器。

## 3. 非目标

- 第一版不通过互联网中继，不解决跨 NAT 传输。
- 第一版不做账号系统、云端设备列表或跨公网同步。
- 第一版不支持 iOS；Android 纳入范围，但不承诺后台永久驻留、后台持续捕获剪贴板或应用被系统挂起后仍实时在线。
- 第一版不承诺与苹果原生 AirDrop 协议兼容。
- 第一版不做文件夹双向持续同步。
- 第一版不把文件永久存储在应用自己的内容仓库。
- 第一版不自动执行收到的文件。
- 第一版不承诺任意应用私有剪贴板格式都能跨操作系统解释；不兼容格式必须跳过或退化到同一剪贴板项中的标准表示。
- 第一版不保存可浏览的完整剪贴板历史，只保留每个授权设备的最新槽位和短期离线补发状态。
- 第一版不因远端槽位更新而在后台自动写入或覆盖任何接收设备的本机系统剪贴板。
- 第一版不通过模拟键盘、临时替换后恢复剪贴板等方式实现“直接粘贴到任意应用”；v1 的取用动作明确写入本机剪贴板，之后由用户正常粘贴。
- 第一版不把“复制文件”扩展为文件夹持续同步；文件剪贴板只复制该次事件引用的静态快照。
- 第一版不使用自研密码学协议。
- 第一版不保证能够识别所有密码管理器或应用私有的敏感标记；无法识别时由用户配置的来源、类型和大小策略继续约束。

## 4. 方案比较

### 4.1 中心服务器中转

所有设备连接服务器，文件经服务器转发。

优点：

- 设备发现和连接逻辑简单。
- 容易扩展到公网和离线消息。

缺点：

- 文件需要离开局域网。
- 需要维护服务器、账号和存储。
- 无法体现项目“本地直连”的核心价值。

### 4.2 无配对的局域网广播

设备发现后直接传输，接收时仅弹窗确认。

优点：

- 开发和使用都简单。
- 首次传输步骤少。

缺点：

- 无法稳定识别设备身份。
- 容易被同网段恶意设备冒充。
- 难以安全实现自动接收和剪贴板同步。

### 4.3 mDNS 发现 + 人工配对 + QUIC 直连

本项目采用此方案。

mDNS 只负责发现可连接端点，不承担信任判断。QUIC 提供加密和多流传输；首次连接时双方比较由会话与身份材料导出的短验证码，确认后保存对方的长期身份公钥。后续连接必须与已固定身份匹配。

### 4.4 剪贴板镜像拓扑

多设备剪贴板镜像有三种候选拓扑：

- 逐设备独立配置：实现简单，但设备增多后配置重复且容易不一致。
- 指定中心设备：选择器状态统一，但中心设备休眠会中断整个镜像体验。
- 同步组内直连全互联：每个设备独立发布自己的槽位，数据仍在成员间直接流动，任一普通成员离线不影响其他成员。

项目采用同步组内直连全互联。组 Owner 只管理成员和共享策略，不承担数据中转；每个 device_id 是自己槽位的唯一写入者，不需要对不同设备的剪贴板做全局排序。

## 5. 总体架构

    +------------------------- Device A -------------------------+
    |  Desktop UI / CLI -> Local IPC -> Daemon                  |
    |                                  |                         |
    |  Identity  Discovery  Connection Manager  Storage          |
    |  Pairing   Sync Group Clipboard           Transfer         |
    +--------------------------+---------------------------------+
                               | mDNS + QUIC/TLS 1.3
                 +-------------+-------------+
                 |                           |
        +--------+--------+         +--------+--------+
        |    Device B     |<------->|    Device C     |
        | Same Core/Daemon|  QUIC   | Same Core/Daemon|
        +-----------------+         +-----------------+

同步组数据采用成员间直连全互联。图中的 A、B、C 各自发布独立剪贴板槽位；Owner 只签发组配置，不处在数据路径中心。

主要组件：

1. Identity：生成和保存本机身份，管理已配对设备。
2. Discovery：通过 mDNS 发布和发现设备端点。
3. Transport：维护 QUIC 连接、控制流和数据流。
4. Pairing：执行首次验证码确认和身份固定。
5. Sync Group：管理同步组成员、方向、内容类型和本地限制策略。
6. Clipboard：捕获多表示剪贴板项、发布本机槽位、镜像远端槽位、能力协商、缓存和显式取用。
7. Transfer：文件清单、分块、校验、恢复和原子落盘；同时为文件剪贴板提供受限的内容传输能力。
8. Daemon：持有长期网络状态，对 UI 和 CLI 提供本地 API。
9. Desktop UI：托盘、设备列表、同步组、传输进度和隐私设置。
10. Android UI：前台实时设备槽位、触控选择器、当前剪贴板显式发布和应用恢复同步。
11. CLI：脚本化发现、配对、同步组管理、发送、接收和诊断。

## 6. 进程模型

第一版采用单守护进程模型：

- airdropd：后台常驻，负责发现、连接、传输和本地状态。
- airdrop：CLI，通过本地 IPC 调用守护进程。
- Desktop：Tauri 应用，通过同一 IPC 调用守护进程。

Android 不建立独立的永久系统 Daemon。Tauri Android 应用在前台时由同一进程内的 Rust Core 维护发现、连接、槽位和缓存；进入后台后允许操作系统挂起进程与网络，不申请默认输入法、辅助功能、设备管理或常驻前台服务来规避平台限制。回到前台后必须重新连接并先获取完整 Snapshot，不能假设后台期间 event_revision 连续。

Android 应用生命周期至少包括：

    ColdStart / Resume -> Connecting -> SnapshotLoading -> ForegroundLive
    ForegroundLive -> Pausing -> Suspended
    Suspended -> Connecting

Suspended 是正常生命周期而非故障。UI 保存最后同步时间并把需要在线授权检查的操作标记为暂不可用；已经缓存的正文也必须由 Rust Core 重新检查有效 TTL、Lease、成员状态和本地策略后才能 Import。

Daemon 是唯一能够修改身份、配对数据库和传输状态的进程，避免 UI 与 CLI 同时操作导致竞态。

airdropd 是当前登录用户会话内的 Daemon，不作为跨用户系统服务运行。若平台要求剪贴板调用必须位于图形会话、主线程或 Portal 生命周期内，platform crate 可以启动最小化 Clipboard Bridge；Bridge 只负责枚举剪贴板，以及凭一次性 AdapterWriteCapability 执行 Daemon 已确认的本机写入，通过受保护 IPC 接收有界命令。所有信任、策略、槽位状态、ImportIntent 和网络访问仍由 Daemon 决定。关闭主窗口不等于退出托盘会话或 Clipboard Bridge。

本地 IPC 优先使用：

- Unix：Unix Domain Socket。
- Windows：Named Pipe。

IPC 协议采用固定 32 位大端长度前缀加 JSON 消息，首条消息必须完成 IPC 协议版本协商。单帧默认上限 4 MiB，文件正文和大型剪贴板正文不经过 IPC JSON 帧，而使用受控的本地文件句柄、临时对象引用或分块流。

每个请求包含 request_id，响应回显 request_id；Daemon 事件包含全局单调递增 event_revision。客户端重连时先请求完整 Snapshot，再订阅 revision 之后的增量事件，不能只依赖可能丢失的实时通知。

Import 使用两个本地 IPC 命令：`CreateImportIntent` 包含 import_id 和固定 SlotSelection，用于持久化用户选择并获取缺失正文；`ConfirmImport` 用于内容就绪后的最终写入确认。如果创建 Intent 时全部正文已 Ready，同一次用户 IPC 动作可以直接完成确认。

只有 Daemon 能在最终授权检查通过后创建短生命周期、单次消费、绑定 import_id 与 SlotSelection hash 的 AdapterWriteCapability，并通过受保护的本地调用或 Clipboard Bridge IPC 交给 Adapter。capability 只存在于当前用户会话内存，网络协议、数据库记录和远端事件都不能携带、恢复或延长。

IPC 端点仅允许当前用户访问：Unix 使用目录与 Socket 权限并校验 peer credentials；Windows 使用当前用户 SID 的 Named Pipe ACL，并拒绝远程 Pipe 客户端。Daemon 使用进程锁和端点探测保证每个系统用户只有一个实例。Desktop 和 CLI 发现 Daemon 未运行时可以启动它，但版本不兼容时必须明确报错，不能同时启动第二个状态写入者。

## 7. 设备身份

### 7.1 长期身份

首次启动时生成长期 Ed25519 自签名 TLS 证书和私钥：

- 私钥保存在操作系统凭据存储；不可用时使用权限受限的本地密钥文件。
- 证书的 SPKI 公钥用于计算稳定设备 ID。
- 展示名称、平台和设备能力不是身份依据，可以随时变化。

device_id 固定定义为 `ld1_` 加无填充小写 Base32 编码的 `SHA-256(DER-encoded SubjectPublicKeyInfo)` 完整 32 字节结果。编码比较按 ASCII 字节进行，不允许缩写参与协议判断。不使用 MAC 地址、主机名或 IP 地址作为身份。

### 7.2 连接证书

QUIC 只允许 TLS 1.3。可信传输固定 ALPN 为 localdrop/1，首次配对固定 ALPN 为 localdrop-pair/1。v1 不使用短期证书，也不在“证书或握手声明”之间保留二选一：

- 每台设备直接使用长期 Ed25519 自签名证书进行双向 TLS。
- 已认证模式下，Rustls 自定义证书验证器要求对端 SPKI 与本机的直接配对记录或有效同步组成员记录完全一致，并返回对应 AuthScope。直接配对和组成员授权不能混为一个布尔值。
- 首次配对模式下，Initiator 只对用户明确选择的单个候选端点临时接受未知自签名证书；Responder 只在用户打开“允许配对”窗口时接受 localdrop-pair/1 的未知客户端证书。
- TLS 握手本身证明双方持有证书对应私钥，应用层不再设计第二套自定义签名协议。
- v1 对配对、控制消息和文件传输全部禁用 QUIC 0-RTT，避免重放语义复杂化。

本项目不自行实现加密原语，只组合成熟库提供的 Ed25519、TLS 1.3 和 QUIC 能力。

### 7.3 密钥轮换

信任固定对象明确为 SPKI，而不是完整证书字节：

- 使用同一 Ed25519 密钥重新签发自签名证书时 device_id 不变，不要求重新配对。
- Ed25519 密钥变化导致 SPKI 和 device_id 变化，必须重新配对。
- 新证书仍需通过有效期、签名算法、自签名完整性和持钥证明检查。
- 第一版不提供自动密钥轮换；私钥丢失、损坏或主动更换都生成新身份并重新配对。

## 8. 设备发现

设备通过 mDNS 发布服务：

    _localdrop._udp.local

广播信息只包含连接所需的非敏感元数据：

- protocol_version
- service_instance_id：Daemon 每次启动生成的随机实例标识
- 完整 device_id，作为连接提示而非认证结果
- device_name
- QUIC 端口
- platform
- capabilities

mDNS capabilities 只使用版本化的粗粒度位集，例如 file-transfer、clipboard-slots 和 pairing，不广播应用私有格式、同步组、文件名或剪贴板类型。详细 ClipboardCapabilities 只在身份验证后的 Hello 中交换。

未认证发现记录不得仅按 device_id 或短指纹合并。Discovery 规则：

- 同一个 DNS-SD Service Instance 自带的多个地址可以组成一个候选端点集合。
- 不同 service_instance_id 即使宣称相同 device_id，也作为不同未认证候选展示。
- 建立 TLS 连接并完成 SPKI 验证后，地址才能归并到已认证设备。
- mDNS 中的 device_id 只用于帮助 UI 展示“可能是已配对设备”，不能跳过证书固定校验。
- 候选记录按 TTL 过期，不能永久污染在线设备列表。

发现不等于信任：

- 未配对设备显示为“附近设备”。
- 已配对且身份匹配的设备显示为“可信设备”。
- 设备 ID 相同但身份验证失败时显示明确安全告警。

第一版仅保证同一二层网络或 mDNS 可达网段。手动输入 IP 和二维码连接可以作为后续补充。

### 8.1 已认证连接仲裁

同一设备可能因 IPv4、IPv6、多网卡、双方同时拨号或网络切换产生多条 QUIC 连接。完成 TLS 身份验证后，Transport 必须先执行连接仲裁，只有一条连接可以成为该 device_id 的 active connection。

每个 Daemon 启动时生成随机 128 位 daemon_instance_id；每条连接的 Initiator 生成随机 128 位 connection_nonce。Hello 明确携带 local_daemon_instance_id、peer_daemon_instance_id 回显、initiator_device_id 和由 Initiator 唯一生成的 connection_nonce。

双方按 device_id 从小到大排列设备，并构造完全相同的连接排名：

    (
      daemon_instance_id_of_lower_device,
      daemon_instance_id_of_higher_device,
      initiator_device_id,
      connection_nonce
    )

在当前仍存活且完成身份验证的连接集合中，无符号字节序最大的排名成为 active。随机 instance ID 不表达时间先后；设计不使用“ID 改变意味着更新”规则。旧进程真正退出后其连接自然离开候选集合，剩余连接重新仲裁。短暂重叠期间即使旧实例胜出也只影响可用性，不改变业务正确性。

其余连接进入 draining，不得发起新的状态修改。仲裁算法使用固定二进制编码并提供测试向量。session_epoch 由域分离 BLAKE3 对上述完整排名编码计算并截取 128 位，双方独立得到相同结果。控制消息携带 session_epoch；旧连接迟到的消息不能覆盖新连接状态。连接管理器以通过证书验证得到的真实 device_id 为键，绝不使用 mDNS 声明的 ID 进行去重。

## 9. 配对流程

首次配对流程：

1. Responder 用户主动打开 120 秒“允许配对”窗口。窗口外拒绝所有未知证书的 localdrop-pair/1 握手；窗口内仍执行每接口和全局尝试限速。
2. 发送方选择一个具体 mDNS 候选端点，并以 Initiator 角色发起禁用 0-RTT 的双向 TLS 连接；监听方为 Responder。
3. 双方只接受 ALPN localdrop-pair/1、Ed25519 自签名证书和协议 v1，不允许静默降级。
4. Initiator 生成 16 字节 pairing_session_id 和 32 字节 initiator_nonce，发送 PairInit。
5. Responder 校验请求后原样回显 pairing_session_id 和 initiator_nonce，并生成 32 字节 responder_nonce，发送 PairHello。
6. 双方从 TLS exporter 使用标签 EXPORTER-localdrop-pairing-v1 导出 32 字节 exporter_secret。
7. 双方使用固定字段顺序构造 pairing_context：协议版本、pairing_suite_id、实际 TLS cipher suite ID、证书签名算法 ID、ALPN、pairing_session_id、initiator_nonce、responder_nonce、Initiator device_id、Responder device_id。v1 的 pairing_suite_id 固定代表 TLS exporter + HKDF-SHA256 + HMAC-SHA256 + 六位拒绝采样。字段采用长度前缀字节编码，不依赖 JSON 序列化顺序。
8. 验证码派生固定为 HKDF-SHA256：IKM 为 exporter_secret，salt 为 SHA-256(initiator_nonce || responder_nonce)，info 为字符串 localdrop-sas-v1 加 pairing_context，输出 32 字节 sas_key。
9. 六位数字通过确定性拒绝采样获得：依次计算 HMAC-SHA256(sas_key, "code" || big-endian counter)，读取前 32 位；只接受小于 floor(2^32 / 1000000) * 1000000 的首个值，再对 1000000 取模并补足六位。
10. 两端显示设备名称、完整指纹的短展示形式和六位验证码。
11. 用户确认一致后发送 PairConfirm，消息包含 pairing_session_id、SHA-256(pairing_context) 和 accept=true。
12. 双方都收到对端 PairConfirm 且上下文完全一致后，把对端 SPKI 写入 pending trust，发送 PairComplete。
13. 收到对端 PairComplete 后把信任记录原子提升为 trusted；如果该 device_id 曾被本地撤销，只有本次重新完成双端验证码确认才能在同一事务中清除撤销记录。连接中断留下的 pending trust 有短 TTL，不授予传输权限。
14. 后续连接改用 ALPN localdrop/1，并使用固定 SPKI 校验。

TLS exporter 把验证码绑定到本次完整 TLS 握手，固定角色顺序、算法套件和协议版本避免反射、unknown-key-share 和降级歧义。如果攻击者处于中间位置，两端建立的是不同 TLS 会话，验证码应不同，用户不得确认。验证码和 pairing_session_id 仅在当前连接和配对窗口内有效，失败多次后进入冷却时间。

“解除配对”是本机设备级撤销动作：删除直接信任记录，并把该 device_id 写入本地撤销表，使现有 GroupManifest 也不能重新授予 group-scoped 权限。设备级撤销只能通过与该 device_id 重新完成验证码配对来清除，GroupInvite 不能清除。

GroupLeave 是独立的组级状态：它不撤销设备直接配对。仍保持 direct-paired 且未进入设备级撤销表的设备，可以通过接受更高 revision 的新 GroupInvite 重新加入该组。若希望继续保留组内同步、只禁止普通文件投送，应修改权限而不是解除配对。

## 10. 同步组

同步组是剪贴板槽位发布、订阅、转发和取用的授权与策略边界。只有已经完成身份固定的设备才能被邀请加入同步组；配对本身不自动授予远端槽位读取或正文缓存权限。

### 10.1 组模型

每个同步组包含：

- 随机 128 位 group_id。
- 展示名称。
- owner_device_id。
- 单调递增 group_revision 和 membership_epoch。
- 成员 device_id、完整 SPKI、加入时间及状态：invited、active、removed。
- 默认同步方向、内容类型、大小上限和离线补发 TTL。
- 每个成员的方向覆盖：bidirectional、send_only、receive_only、disabled。
- 每个成员是否允许 relay_cached_events；默认开启，可由本机关闭。

第一版每组最多同时存在 16 个 active 成员。已 removed 成员的旧 epoch 快照只保留到本组最大槽位 TTL，过期后可以清理，不因成员历史无限扩大运行状态。组 Owner 只负责成员和共享默认策略的管理，不转发数据，也不是运行时中心；Owner 离线时，已有 active 成员仍然直接同步。第一版若 Owner 身份永久丢失，已有组可以继续按最后配置运行，但需要创建新组才能重新管理成员；自动 Owner 选举不进入 v1。

v1 中 Owner 不能把自己移出组或转移所有权。正常结束同步组时，Owner 发布带更高 group_revision 和 membership_epoch 的签名 GroupTombstone；成员持久化 Tombstone 并拒绝旧 GroupManifest 回滚。Owner 私钥丢失时无法生成 Tombstone，各成员只能执行本地退出或撤销。

### 10.2 邀请与成员变更

Owner 只能向与自己直接配对的设备发送有过期时间和目标 device_id 的 GroupInvite。目标用户必须显式接受一次，之后同步自动进行。接受结果通过已认证连接返回，Owner 增加 group_revision，并发布包含所有 active 成员 device_id 与完整 SPKI 的新 GroupManifest。

组内不要求每两台设备都再次比较验证码。成员通过 Owner 签名的 GroupManifest 获得其他成员的 group-scoped trust grant，并据此验证组内直连证书。该授权只允许对应 group_id 的配置交换、剪贴板事件和文件剪贴板正文，不能自动授予普通文件投送、创建其他组或修改本机直接配对记录的权限。

GroupManifest 使用版本化、规范化二进制编码，并由 Owner 的长期 Ed25519 身份密钥签名。成员通过已固定的 Owner SPKI 验证签名；这只用于让成员在 Owner 暂时离线或配置经其他成员转发时验证组配置来源，不替代 TLS 连接认证。

删除成员会增加 membership_epoch。收到更新后立即停止向被删除设备发送事件，并拒绝该设备使用旧 revision 请求剪贴板正文。Owner 记录仍未确认新 epoch 的离线成员，UI 显示“撤销同步中”；局域网分区期间无法让尚未收到更新的离线设备遗忘旧配置，也不能承诺远程擦除它已经取得的内容。

为验证仍在 TTL 内的旧 epoch ClipboardSlotEvent，每台设备保留覆盖本组最大离线 TTL 的签名 GroupManifest 快照；快照只用于验证事件创建时的成员、SPKI 和 audience，不得恢复已经被新 epoch 撤销的发布权限。所有正文请求仍同时检查当前 Manifest，旧 epoch 事件不能发送给后来加入或当前已移除的成员。

本机撤销表的优先级高于任何 GroupManifest；解除设备配对会立即禁用其全部组权限，即使尚未收到新的 GroupManifest。

非 Owner 成员可以执行本地 GroupLeave：生成并持久化随机 128 位 leave_id，立即把组状态标记为 left，删除该组授予的全部 SPKI trust grant、远端槽位缓存和待传输正文，并在 Owner 可达时发送 GroupLeaveNotice。重试必须沿用 leave_id。Owner 收到后发布移除该成员的新 Manifest；即使通知失败，本地也不能被旧 Manifest 自动重新加入，只能再次接受更高 revision 的新 GroupInvite。Owner 结束组只能使用 GroupTombstone。

如果成员在本地撤销组 Owner，则该 Owner 管理的全部组自动执行本地 GroupLeave，因为后续 GroupManifest 已失去可信锚点。接受 GroupInvite 本身不能清除 device_id 撤销；必须先重新完成直接配对，随后再显式接受邀请。

### 10.3 策略合并

同步是否允许由三层策略共同决定：

1. GroupManifest 中的组默认策略；
2. GroupManifest 中的成员方向和类型覆盖；
3. 每台设备的本地隐私策略。

本地策略只能进一步收紧远端策略，不能静默扩大权限。例如组允许图片同步，但某设备本地禁用图片时，该设备既不发布图片，也不应用收到的图片。发送端和接收端在每次事件传输前都独立执行策略检查，不能只信任对端已经过滤。

成员方向的语义相对于“发布本机槽位”和“订阅远端槽位”定义：

- bidirectional：可以发布本机槽位，也可以订阅和缓存远端槽位。
- send_only：可以发布本机槽位，不订阅远端槽位。
- receive_only：不发布本机槽位，可以订阅和缓存远端槽位。
- disabled：两者都禁止。

从原始来源 O 到订阅者 T 的组级有向投递边只有在 GroupManifest 中 O 允许 publish、T 允许 subscribe 且对应内容类型策略允许时成立。创建槽位事件时，来源再应用自己的本地发布限制，为每个 Representation 计算 signed representation_audience，并把它们的并集写入事件 signed audience；后来加入的成员不能收到旧事件。目标本地订阅、预取和取用策略不对外公开，在收到 Offer 后独立决定缓存哪些正文。

v1 的事件描述对 signed audience 中的成员整体可见：收到事件的成员会看到该事件所含全部 Representation 的 type_id、byte_length 和 content_hash，即使自己不在某个 representation_audience 中；representation_audience 只控制正文请求。若用户连某种格式的存在、大小或哈希都不希望某成员获知，必须在发布端对整个同步组禁用该格式，或使用成员边界不同的同步组。UI 在配置成员级类型策略时必须提示这一元数据边界。

转发与发布分开定义。receive_only 成员可以在 `relay_cached_events=true` 时把自己已获授权并缓存的槽位事件转交给 signed audience 中的其他目标，因为这不会把转发者自己的剪贴板冒充为来源；send_only 或 disabled 成员若未订阅正文，自然不能充当正文转发者。任一设备都可以本地关闭转发。转发时仍按当前 GroupManifest 检查来源与目标是否 active，策略收紧或成员移除立即阻止旧缓存继续发送。

## 11. 传输协议

### 11.1 流与帧

TLS 握手完成后，QUIC 连接发起方立即打开唯一的双向控制流。控制流以固定 magic、协议主版本和 framing 版本开头，并先交换 Hello；连接仲裁完成前只允许 Hello 和仲裁错误消息，不能修改业务状态。胜出的连接继续把该流作为正式控制流，失败连接进入 draining。同一连接上的第二条控制流视为协议错误。

双方都在正式双向流上发送控制消息，确认和进度消息进行批量合并，避免为每个块单独创建确认流。

连接中使用以下流类型：

- 一条双向控制流：请求、决定、取消、状态、组配置和剪贴板事件描述。
- Manifest 单向流：大型文件清单和分块哈希表。
- Blob 单向流：图片、富文本、私有格式或其他剪贴板正文。
- Range 单向流：文件和文件剪贴板的连续块范围。

控制消息使用 32 位大端长度前缀 JSON，必须包含 type、schema_version 和 message_id；除用于派生 session_epoch 的首个 Hello 外，其他消息都必须包含 session_epoch。单帧默认上限 1 MiB。未知 type 或 schema_version 必须拒绝；扩展的 critical 语义只通过 11.7 定义的 extensions 数组表达。Manifest、哈希表和正文不嵌入大型 JSON 数组，而使用有长度上限的规范化二进制流。

### 11.2 消息集合

配对 ALPN `localdrop-pair/1` 使用独立消息集合：PairInit、PairHello、PairConfirm、PairComplete 和 PairAbort。

可信连接 ALPN `localdrop/1` 的核心消息包括：

- Hello：协议版本、daemon_instance_id、connection_nonce、设备 ID 和能力；本地根据证书固定记录计算 direct-paired 或 group-scoped AuthScope，不能采信对端自报权限。
- GroupInvite、GroupAccept、GroupLeaveNotice、GroupManifestUpdate、GroupTombstone：同步组管理与删除。
- ClipboardSlotOffer、ClipboardSlotRequest、ClipboardSlotCached、ClipboardSkipped：远端槽位镜像、正文请求和缓存结果。
- ClipboardLeaseAcquire、ClipboardLeaseGranted、ClipboardLeaseRelease：用户在槽位到期前为按需正文申请、确认和释放有界保留租约。显式写入本机仍是本地 IPC 操作。
- TransferOffer：Manifest 摘要、文件数量和总大小。
- TransferDecision：接受、拒绝和冲突策略；接收方本地目标路径绝不发送给对端。
- ManifestStreamHeader：绑定 transfer_id、manifest_hash、编码版本和长度。
- ResumeState：接收方已有的 durable 块范围。
- RangeStreamHeader：绑定 transfer_id、manifest_hash、file_id 和一个或多个有界块范围。
- ChunkHeader：块序号、偏移、长度和 BLAKE3 块哈希，随后紧跟原始块字节。
- FileComplete、FileVerified、Cancel：完成、校验和取消。

### 11.3 幂等与重放

message_id 是 128 位随机值，重试同一逻辑操作必须沿用原 message_id。仅靠 message_id 不足以定义业务幂等，持久化状态修改还使用语义键：

| 操作 | 语义键 | 更新规则 |
| --- | --- | --- |
| GroupManifestUpdate/Tombstone | `(owner_device_id, group_id, group_revision)` | revision 必须严格递增 |
| GroupInvite | `(owner_device_id, group_id, invite_id, target_device_id)` | 邀请内容不可变 |
| GroupAccept | `(group_id, invite_id, target_device_id)` | Owner 只增加一次成员；重复请求返回首次产生的 group_revision/Manifest |
| GroupLeaveNotice | `(group_id, member_device_id, leave_id)` | Owner 只移除一次成员；成员已 removed 时返回当前结果，不再次增加 revision |
| ClipboardSlotOffer | `(group_id, origin_device_id, event_id)` | 槽位事件描述不可变；只接受更高 origin_sequence |
| ClipboardLeaseAcquire | `(requester_device_id, group_id, event_id, lease_id)` | representation_id 集合和 requested_duration_ms 不可变；有效期内重复请求返回首次 Grant |
| ClipboardLeaseRelease | `(requester_device_id, group_id, event_id, lease_id)` | 重复释放无副作用，来源最迟仍按 source_expires_at 清理 |
| TransferOffer | `(sender_device_id, transfer_id)` | manifest_hash 和 Offer 内容不可变 |
| TransferDecision | `(receiver_device_id, transfer_id)` | 接受或拒绝决定只产生一次；后续停止使用 Cancel |
| ResumeState | `(receiver_device_id, transfer_id, receiver_state_revision)` | revision 单调递增，旧 revision 可忽略 |
| FileComplete | `(sender_device_id, transfer_id, file_id)` | manifest_hash 固定，声明不可变 |
| FileVerified | `(receiver_device_id, transfer_id, file_id, verification_revision)` | revision 单调递增，可从校验失败推进到最终 committed |
| Cancel | `(actor_device_id, transfer_id, cancel_id)` | 任一有效 Cancel 使对应方向进入终态，重复取消无副作用 |

receiver_state_revision 和 verification_revision 都由接收端在同一 SQLite 事务中持久化递增，不能由发送端指定。没有 revision 的不可变操作不得通过发送新 message_id 修改原内容。

接收端为每个语义键保存规范化 payload_hash。payload_hash 对消息类型定义的语义字段按 11.7 公共原语和固定字段表编码后计算，不对原始 JSON 文本计算。相同语义键和相同 payload_hash 视为重试并返回第一次结果；相同键出现不同 payload_hash，或同一 group_id/来源的相同 origin_sequence 出现不同 capture 身份或语义内容，视为 ProtocolConflict，冻结该来源槽位并拒绝覆盖。

会触发用户弹窗、自动接收、文件提交或组成员变化的操作必须在 SQLite 中保存处理结果。重复请求返回第一次的结果，不重复弹窗、重复创建目标映射或重复提交。终态传输的去重记录至少保留到历史清理期限；纯查询消息可以只在当前 session_epoch 内缓存。

### 11.4 规范化哈希

Manifest 哈希不对 JSON 文本计算。固定输入为：

    "localdrop-manifest-v1\0" || canonical_manifest_bytes

manifest_hash 固定为上述输入的 BLAKE3-256。GroupManifest hash 固定为 `BLAKE3-256("localdrop-group-manifest-hash-v1\0" || 完整规范化对象含签名)`；控制消息 payload_hash 固定为 `BLAKE3-256("localdrop-control-payload-v1\0" || type 的规范化 ASCII 字符串 || canonical_semantic_fields)`。不同对象类型不得复用域分离前缀。

representation_id 固定为 `BLAKE3-256("localdrop-clipboard-representation-v1\0" || CaptureRepresentationDescriptor)`。同一 Bundle 或 SlotEvent 中 representation_id 必须唯一，收到重复 ID、ID 与描述不匹配或无法形成严格排序的事件时拒绝。

capture_bundle_hash 固定为 `BLAKE3-256("localdrop-clipboard-capture-v1\0" || 按 representation_id 排序的 (representation_id, CaptureRepresentationDescriptor) 数组)`，在按同步组策略过滤表示之前计算。该规范化布局明确排除 representation_audience 和其他组字段；哈希只用于确认多个组事件来自同一次本机捕获，不授权接收被过滤的表示。

canonical_manifest_bytes 使用版本化二进制编码：整数按字段定义固定宽度与符号并使用大端序，修改时间使用可选的 UTC Unix 纳秒 `i64`；枚举使用固定数值；字符串为 NFC UTF-8 并带长度；顶层项按逻辑名称/top_level_id 排序，条目按 top_level_id、内部相对路径和 file_id 排序；禁止浮点数和 map；字段顺序写入协议规范。哈希覆盖编码版本以及全部发送方声明的语义字段，包括 transfer_id、Offer 展示名称、top_level_id、顶层逻辑名称、类型、tree_hash、内部相对路径、file_id、类型、大小、修改时间、块大小、整体哈希和块哈希表；只排除接收方本地目标路径映射、UI 状态和传输统计。

GroupManifest、ClipboardSlotEvent 描述和需要签名的对象使用相同原则，但各自使用不同域分离前缀。协议仓库必须保存跨语言固定测试向量，任何编码变化都必须提升编码版本。

### 11.5 范围恢复

每个 Manifest 项具有稳定随机 file_id。新连接上的数据流必须先发送 RangeStreamHeader，接收方验证 transfer_id、manifest_hash、file_id、范围边界和当前授权后才接受块数据。

ResumeState 使用已持久化的块位图或等价范围列表。范围数量和编码字节数都有上限；过度碎片化时接收方使用压缩位图或分页响应。发送端调度器可以把相邻缺失范围合并到同一 Range Stream，每条流携带的范围数量有上限，不依赖旧连接 Stream ID，也不按“一个缺失范围一条流”无限创建 QUIC Stream。

### 11.6 v1 协议硬上限

解析器在分配内存或创建文件前执行硬上限检查。本机设置可以进一步收紧，但不能放宽协议硬上限：

| 项目 | v1 硬上限 |
| --- | ---: |
| 控制消息帧 | 1 MiB |
| JSON 嵌套深度 | 16 |
| 单字符串字段 | 64 KiB |
| 同步组同时 active device_id | 16 |
| 单 ClipboardBundle 表示数 | 64 |
| 单对端活动 Clipboard Lease | 64 |
| 剪贴板 inline 正文 | 64 KiB |
| 单 Blob Stream | 512 MiB |
| 单 Manifest Stream | 256 MiB |
| 单次 Manifest 条目 | 1,000,000 |
| 单条 ResumeState 范围 | 4,096 |
| 单连接应用数据流并发 | 256 |

Manifest 和 Blob 即使允许较大上限也必须流式解析到受控临时对象，不能按声明长度一次性分配内存。图片 Adapter 在解码前还要检查像素尺寸和解码后大小上限。

### 11.7 版本兼容与规范化对象布局

v1 的版本层次固定如下：

| 层次 | v1 值 | 不兼容处理 |
| --- | --- | --- |
| 配对 ALPN | `localdrop-pair/1` | TLS 协商失败 |
| 可信连接 ALPN | `localdrop/1` | TLS 协商失败 |
| framing_version | `1` | 关闭连接 |
| control schema_version | `1` | 拒绝该消息；核心消息不允许静默猜测 |
| Manifest encoding_version | `1` | 拒绝 Manifest |
| GroupManifest encoding_version | `1` | 拒绝组更新 |
| ClipboardSlotEvent encoding_version | `1` | 跳过事件且不修改远端槽位 |

Hello 包含 `supported_feature_ids` 和 `required_feature_ids`。任一方不支持对端 required feature 时关闭连接；optional feature 只取交集。JSON 消息的未来扩展只能放入 `extensions` 数组，每项包含数值 id、critical 布尔值和有界 value；未知 critical 扩展必须拒绝消息，未知 non-critical 扩展可以忽略。普通未知 JSON 字段不表达“必需”语义。

所有规范化对象共用以下原语：

| 类型 | 固定编码 |
| --- | --- |
| device_id | SHA-256 SPKI 的原始 32 字节，不编码 `ld1_` 文本 |
| group_id、event_id、capture_id、import_id、lease_id、invite_id、leave_id、transfer_id、top_level_id、file_id、message_id、cancel_id | 16 字节 |
| BLAKE3/SHA-256 hash | 原始 32 字节 |
| revision、membership_epoch、origin_sequence、TTL、size | u64 大端 |
| encoding/schema/framing version | u16 大端 |
| enum | u8 |
| count、byte/string length | u32 大端 |
| timestamp | UTC Unix 纳秒 i64 大端 |
| bool/presence | 单字节 0/1，其他值非法 |
| UTF-8 string | u32 字节长度 + NFC UTF-8；type_id 等 ASCII 子集仍按此编码 |
| byte string/SPKI DER | u32 长度 + 原始字节 |
| array | u32 元素数 + 连续元素 |

v1 枚举值固定为：member_state invited=0、active=1、removed=2；direction disabled=0、send_only=1、receive_only=2、bidirectional=3；data_mode inline=0、blob=1、file_manifest=2；file_type regular=0、directory=1；conflict_policy reject=0、rename=1；signature_algorithm ed25519=1。未知枚举值按对应对象版本不兼容处理。

ContentPolicy 编码顺序固定为：offline_ttl_ms(u64)、representation_rule_count(u32)、按 type_id 排序的 rules、max_file_count(u32)、max_file_total_bytes(u64)。每条 rule 为 type_id、enabled(bool)、max_bytes(u64)。GroupPolicy 固定为 default_direction(u8)、default_relay_cached_events(bool)、ContentPolicy。成员 content_policy_override 先写 presence，再写一个完整 ContentPolicy；缺省表示继承组内容策略，方向和 relay 始终使用成员字段。

禁止 map、浮点数、重复字段、非最短长度或未规范化 Unicode。签名 bytes 本身不进入待签名字节，但 signature_algorithm 进入。

规范化字段顺序固定为：

- Manifest：encoding_version、transfer_id、offer_display_name、按逻辑名称/top_level_id 排序的 top_levels、按 top_level_id/内部路径/file_id 排序的 entries；每个 top_level 依次为 top_level_id、logical_name、type、tree_hash，每个 entry 依次为 top_level_id、relative_path、file_id、file_type、size、mtime presence/value、block_size、whole_hash、chunk_hash_count、chunk_hashes。
- Tree commitment：普通文件为 `localdrop-file-root-v1\0`、top_level_id、logical_name、type、size、whole_hash；目录为 `localdrop-tree-v1\0`、top_level_id、logical_name、type、entry_count，以及按 relative_path/file_id 排序的条目，每条为 relative_path、file_id、file_type、size、whole_hash。目录条目的 size 固定为 0、whole_hash 固定为 32 字节零值。
- GroupManifest：encoding_version(u16)、group_id、owner_device_id、group_revision、membership_epoch、name、GroupPolicy、按 device_id 排序的 members；每个 member 依次为 device_id、SPKI DER、joined_at、member_state(u8)、direction(u8)、relay_cached_events(bool)、content_policy_override。最后写入 signature_algorithm，再对截至该字段的字节签名并追加 signature bytes。
- GroupTombstone：encoding_version(u16)、group_id、owner_device_id、group_revision、membership_epoch、deleted_at、signature_algorithm，随后追加 Owner signature bytes；签名覆盖 signature_algorithm 及之前全部字段。
- ClipboardSlotEvent：encoding_version(u16)、group_id、membership_epoch、event_id、capture_id、capture_bundle_hash、origin_device_id、origin_sequence、audience、created_at、expires_at、Representation descriptors、signature_algorithm，随后追加 origin signature bytes。audience 是按 device_id 排序的 device_id 数组。
- CaptureRepresentationDescriptor：type_id、encoding_version、byte_length、content_hash、data_mode、platform_family presence/value、native_format_id presence/value、application_scope presence/value。该布局不含 representation_id、group_id、audience 或任何组策略字段。
- Group Representation descriptor：representation_id、完整 CaptureRepresentationDescriptor，随后追加按 device_id 排序的 representation_audience；解析时重新派生并校验 representation_id。
- SlotSelection：origin_device_id、capture_id、origin_sequence、按 representation_id 排序且 ID 不重复的 selected representations；每项依次为 representation_id、type_id、content_hash、授权 SourceRef 的 group_id、event_id、membership_epoch。selection_hash 使用域分离前缀 `localdrop-slot-selection-v1` 的 BLAKE3-256。

这些布局必须在 protocol crate 中以常量字段表和固定十六进制向量体现，不能只依赖 Rust struct 声明顺序或 Serde 行为。

ClipboardSlotEvent 解析时必须验证事件 audience 恰好等于全部 representation_audience 的集合并集，且所有目标都是该 membership_epoch 的 active 成员；不一致视为 ProtocolConflict。

TransferOffer、TransferDecision、ResumeState、FileComplete、FileVerified 和 Cancel 同样必须各自定义用于 payload_hash 的语义字段顺序；message_id、session_epoch、发送时间和诊断字段不参与语义哈希。没有字段表和固定向量的状态修改消息不得进入 v1 网络实现。

剪贴板正文消息的 v1 语义字段顺序固定为：ClipboardSlotRequest = requester_device_id、group_id、event_id、按 representation_id 排序的 requested_ids；ClipboardSlotCached = requester_device_id、group_id、event_id、按 representation_id 排序的 cached_ids。正文流 Header 同样绑定 group_id、event_id、representation_id、content_hash 和声明长度。

组控制消息的 v1 payload_hash 字段顺序固定为：GroupInvite = owner_device_id、group_id、group_revision、invite_id、target_device_id、expires_at、GroupManifest hash；GroupAccept = group_id、invite_id、target_device_id、accept(bool)；GroupLeaveNotice = group_id、member_device_id、leave_id。GroupAccept 和 GroupLeaveNotice 的首次处理结果分别持久化 resulting_group_revision 与 resulting_manifest_hash，重试直接返回这两个值。

租约消息字段顺序固定为：ClipboardLeaseAcquire = requester_device_id、group_id、event_id、membership_epoch、lease_id、selection_hash、按字节序排序的 requested_representation_ids、requested_duration_ms；ClipboardLeaseGranted = responder_device_id、requester_device_id、group_id、event_id、lease_id、selection_hash、granted_representation_ids、granted_duration_ms、source_expires_at；ClipboardLeaseRelease = requester_device_id、group_id、event_id、lease_id。Grant 只覆盖 responder 实际持有并验证过的 representation_id 集合。

## 12. 文件与目录传输

### 12.1 传输清单

发送前生成 Manifest：

- transfer_id
- Offer 展示名称
- 每个用户选中顶层项的稳定 top_level_id、逻辑名称、类型和 tree_hash
- 条目所属 top_level_id 及其内部相对路径
- 稳定 file_id
- 文件类型
- 文件大小
- 修改时间
- BLAKE3 整体哈希
- 固定块大小
- 每个块的 BLAKE3 哈希

Manifest 摘要放在 TransferOffer 中，完整条目和原始 32 字节块哈希通过 Manifest Stream 发送。第一版默认块大小为 4 MiB，并设置单文件最大块数、单次最大条目数和 Manifest 总字节数；超过限制时在发送前明确失败，而不是产生超大 JSON 控制消息。限制可以在本机配置中调低，但不能超过协议硬上限。

每个顶层提交单元定义 tree_hash：普通文件使用带域分离的文件描述哈希；目录使用 `BLAKE3("localdrop-tree-v1\0" || canonical_tree_entries)`，其中 canonical_tree_entries 按内部相对路径和 file_id 排序，覆盖目录条目、空目录、文件类型、大小和整体内容哈希。修改时间因目标文件系统精度不同不进入 tree_hash，只按 Manifest 尽力恢复。tree_hash 用于验证提交树结构，不替代逐文件内容哈希。

第一版只发送普通文件和目录：

- 目录本身作为 Manifest 条目，因此保留空目录。
- 符号链接默认忽略并提示。
- 设备文件、Socket、FIFO 等特殊文件拒绝发送。
- 硬链接按多个独立普通文件发送；稀疏文件按逻辑内容传输，第一版不保证保留稀疏布局。
- 修改时间尽力恢复；ACL、扩展属性、macOS Resource Fork、Windows ADS 和 Unix 所有者信息不在 v1 保真范围内。
- 相对路径必须规范化，拒绝绝对路径和父目录跳转。

发送方在 Offer 前完成预扫描和哈希，因此大目录会有准备阶段。扫描模块必须通过平台安全文件句柄读取文件：

- Unix 优先使用 openat/openat2、O_NOFOLLOW 和目录句柄约束在发送根目录下。
- Windows 使用拒绝跟随 Reparse Point 的句柄式打开方式。
- 扫描时保留发送根目录句柄和稳定文件标识，不要求为整个大目录长期持有所有文件句柄。正式读取时从根目录句柄重新执行无跟随打开，并校验文件标识、类型、大小和扫描快照。
- 每个块发送前再次计算并比较 Manifest 中的块哈希；即使文件在同一 inode 内被原地修改，也会停止该文件并报告 SourceChanged，而不是发送与 Manifest 不一致的内容。
- 平台无法提供所需无跟随语义时对该条目失败关闭，而不是退化为普通路径读取。

### 12.2 流式传输

文件不会整体载入内存。Transfer 模块按固定大小读取并写入 QUIC Stream。多个文件可以并发，但总并发数和缓冲区大小有明确上限。

默认策略：

- 小文件可以并发发送。
- 大文件限制同时活跃数量，防止磁盘随机读写恶化。
- 发送端根据 QUIC 背压自然降速，不创建无界队列。

### 12.3 临时对象与原子完成

接收内容先写入应用管理的对象目录，不通过在用户文件名后追加 `.part` 构造临时路径：

    <target-root>/.localdrop-staging/<transfer-id>/objects/<file-id>.part
    <target-root>/.localdrop-staging/<transfer-id>/meta/manifest.bin
    <target-root>/.localdrop-staging/<transfer-id>/commit/

临时区必须创建在最终目标目录所在的同一文件系统中：

    <target-root>/.localdrop-staging/<transfer-id>/

接收端使用以目标根目录句柄为锚点的无跟随创建方式，拒绝已有符号链接、Windows Junction/Reparse Point 和路径组件替换。仅做字符串规范化不视为足够安全。

.localdrop-staging 是接收端保留名称。Manifest、目标路径映射和用户选择的顶层名称都不得占用该路径；应用创建带随机标识和版本的管理标记，启动时验证目录所有者、权限、文件类型和标记，不接管来源不明的同名目录。

objects 目录只使用协议生成并验证过的 file_id，不包含用户路径，因此文件 `a` 与目录 `a.part` 等合法名称不会在临时区发生结构冲突。目标逻辑路径只存在于 Manifest 和持久化路径映射中。

提交语义：

- Transfer 在协议中有一个虚拟根，但虚拟根本身不直接落盘；每个 top_level_id 是独立提交单元。
- 顶层普通文件：校验通过后使用平台原子 no-replace 操作把对应对象提交到映射后的最终路径。
- 顶层目录：该目录的全部对象验证后，在 `commit/<top-level-id>/` 中建立完整树并把对象移动到位，随后用原子 no-replace 操作提交整个映射后的顶层目录。
- 多个目录、多个文件或目录与文件混合输入：逐 top_level_id 原子提交，不承诺整个 Transfer 原子性。数据库分别记录每个顶层单元的 CommitIntent、目标映射 revision 和终态，恢复时不得重做已经 Committed 的单元。

接受 Offer 时一次性预检全部顶层逻辑名称的大小写、Unicode 和目标冲突，并为每个 top_level_id 持久化 reject/rename 结果。v1 对普通文件和目录都不提供 overwrite 或目录 merge。多个顶层项发生同名映射冲突时必须在接受前解决或拒绝，不能在提交过程中静默改名。

预检不构成提交锁。最终提交必须使用平台原子 no-replace primitive：Linux 使用 `renameat2(RENAME_NOREPLACE)`，macOS 使用 `renameatx_np(RENAME_EXCL)`，其他 BSD 使用其 exclusive rename，Windows 使用 `MoveFileW` 或不带 `MOVEFILE_REPLACE_EXISTING` 的 `MoveFileExW`。普通文件可在必要时使用同文件系统的 link/no-replace 方案。平台无法提供目录或文件所需的原子 no-replace 语义时，对该目标文件系统失败关闭。

若目标在预检后被其他进程创建，no-replace 提交返回 TargetChanged，绝不覆盖该目标。用户可以取消，或显式生成新的本地 target_mapping_revision 和 rename 名称后重试；恢复流程不得自行重新命名。

若无法在目标文件系统中创建 Staging，则在接受传输前失败，不退化到跨文件系统复制后删除。

最终提交使用显式事务状态：

    Verified
      -> CommitIntent
      -> Materializing
      -> TreePrepared
      -> Renamed
      -> FilesystemApplied
      -> Committed

    CommitIntent/TreePrepared -> TargetChanged
    TargetChanged --explicit remap revision--> CommitIntent

顶层普通文件可以跳过 Materializing/TreePrepared，从 CommitIntent 直接进入 Renamed；顶层目录必须经过全部状态。

1. 在 SQLite 中持久化 CommitIntent，包括 top_level_id、目标映射、target_mapping_revision、冲突策略、预期 tree_hash 和逐条目放置状态。
2. 目录单元逐条目把 `objects/<file-id>.part` 移入 `commit/<top-level-id>/<mapped-relative-path>`。每次恢复都检查对象路径与 commit 路径：对象存在则继续移动；commit 路径存在则验证类型、大小和文件整体哈希后补记 placed；两处同时存在、出现额外条目或内容不符时进入 RecoveryRequired。
3. 全部条目 placed 后，扫描 commit 树并重新计算 tree_hash，确认空目录和结构完全匹配；随后刷新文件，并从叶到根刷新目录，写入 TreePrepared。
4. 使用原子 no-replace primitive 把准备好的顶层文件或目录提交到最终映射，写入 Renamed；目标已经存在时进入 TargetChanged，不执行覆盖。
5. Unix 重新打开最终目标并对目标及父目录执行 fsync；Windows 使用平台可用的 FlushFileBuffers 和不覆盖目标的 MoveFile 等等价 Adapter，并记录实际保证等级。
6. 文件系统操作确认后写入 FilesystemApplied，再原子提升为 Committed。

如果在 Materializing 期间崩溃，按逐条目规则继续构建，不重新下载已验证对象。如果目录 Rename 后、数据库写入 Renamed 前崩溃，恢复要求 staging 顶层不存在、最终目标存在且逐条目验证及 tree_hash 完全匹配；随后重新执行最终目标和父目录持久化屏障。staging 与最终目标同时存在、两者都不存在或 tree_hash 不符时，不猜测用户文件，进入 RecoveryRequired。

普通文件恢复使用同样原则检查对象路径和最终路径的唯一性及 whole_hash。数据库不能在文件系统持久化前直接写入 Committed。

恢复过程即使发现目标哈希匹配，也必须重新打开目标、执行文件刷新和父目录持久化屏障，再写入 FilesystemApplied/Committed；哈希匹配只能证明内容存在，不能证明崩溃前 Rename 已经耐久。平台无法重新建立所需持久化保证时保持 RecoveryRequired，不发送成功确认。

FileComplete 只表示发送端已经发送某文件当前 Manifest 的全部数据，不是传输终态。接收端完成整体校验和提交事务后才发送 `FileVerified(result=committed, verification_revision)`；接收端文件终态叫 Committed，发送端收到该确认后把对应文件和整体传输提升为 Completed。

### 12.4 断点续传

每个传输在 SQLite 中保存：

- transfer_id
- 发送和接收设备 ID
- Manifest 哈希
- 已完成块范围
- 临时目录
- 发送端源路径、稳定文件标识和扫描快照
- 接收端固定目标路径映射和提交状态
- 更新时间

接收块的一致性顺序：

1. 按 ChunkHeader 校验块索引、偏移、长度和哈希。
2. 使用位置写入把块写入对应 file_id 对象。
3. 按大小或时间窗口批量执行 fdatasync/平台等价操作。
4. 只有文件数据确认持久化后，才在 SQLite 事务中标记对应块为 durable。
5. SQLite 提交完成后才向发送端确认这些块。

崩溃发生在步骤 3 和 4 之间时，最多重复发送已落盘但未记账的块；绝不允许数据库声称尚未持久化的块已经完成。全部块完成后再计算整体 BLAKE3 哈希；若整体失败，按分块哈希定位并清除损坏块状态。

连接恢复后，接收方只根据 SQLite 中 durable 块生成 ResumeState。只有 manifest_hash、file_id、文件大小、块大小和哈希表完全匹配时才能继续；源文件发生变化则该文件重新建立 Manifest 或重新发起传输。

Daemon 重启后，发送方也必须从已持久化 Manifest 恢复，并重新通过安全根目录句柄打开源文件、验证稳定标识和块哈希。只持久化接收端位图而无法重建原 Manifest 的传输不能声称支持双端重启恢复。

第一版采用固定块大小和块位图，避免设计复杂的内容定义分块。内容定义分块和跨传输去重属于其他类型产品，不进入本项目。

### 12.5 跨平台路径模型

Manifest 中的逻辑路径统一使用 UTF-8、NFC 规范化和正斜杠分隔，并拒绝空组件、点组件、绝对路径和重复逻辑路径。

接收方在用户确认前完成目标平台预检：

- Windows 拒绝保留设备名、尾随点或空格、ADS 冒号、非法字符和超限路径。
- 所有平台检测 Unicode 规范化碰撞、大小写折叠碰撞、文件与目录前缀冲突、组件长度和总路径长度。
- 任何条目无法表示时默认拒绝整个 TransferOffer，并返回具体冲突列表；第一版不静默清洗文件名。
- 普通文件和目录冲突都只允许 reject 或 rename。第一版不覆盖已有目标，也不执行目录合并。
- rename 策略在接受时计算完整目标路径映射并持久化；普通断线/重启恢复不得重新生成名称。只有原子 no-replace 返回 TargetChanged 后，用户显式确认才能增加 target_mapping_revision 并生成新名称。
- 目录传输的自动 rename 只修改顶层目录名；多个独立文件分别固定目标映射。

## 13. 实时剪贴板镜像与按需取用

实时剪贴板镜像是 v1.0 核心能力。后台只更新远端设备槽位，任何网络事件都不得直接写入本机系统剪贴板。写入本机必须来自用户在选择器、快捷键或 CLI 中发起的一次显式 Import 操作。

### 13.1 多表示剪贴板项

一次本地应用产生的系统剪贴板变化被捕获为 ClipboardBundle。Adapter 应尽可能枚举本次剪贴板项提供的全部可读取表示，而不是只读取第一个文本或图片格式。每个 Representation 包含：

- representation_id：由完整 CaptureRepresentationDescriptor 规范化派生的稳定 32 字节 ID。
- type_id：稳定的标准类型或受约束的私有类型描述。
- encoding_version。
- byte_length。
- BLAKE3 内容哈希。
- inline、blob 或 file_manifest 数据方式。
- 可选的 platform_family、native_format_id 和 application_scope。

跨平台标准类型至少包括：

- `text/plain;charset=utf-8`。
- `text/html`。
- `text/rtf`。
- `image/png`；平台原生位图生成 PNG 兼容表示，同时可以保留兼容的原始表示。
- `text/uri-list`、URL 和文件列表。

同一个 Bundle 可以同时包含纯文本、HTML、RTF 和图片等多个表示。远端先保存描述和允许预取的正文；只有用户选择该设备槽位时，接收端才根据能力和策略选择最佳组合写入本机剪贴板。

应用私有格式只有满足以下条件才能透传：双方 capability 声明相同的平台族、原生格式标识和兼容版本；Adapter 能把它读取为有界字节序列，而不是进程指针、句柄或延迟回调；用户显式允许该私有类型。无法安全序列化或解释的格式跳过，并保留同一 Bundle 中的标准降级表示。

### 13.2 能力协商与预取

Hello 中发布 ClipboardCapabilities：标准类型集合、可透传私有格式描述、最大 inline/blob/file 大小、文件剪贴板能力、Adapter 版本和平台限制。能力只说明技术可处理，不代表用户授权。

发布端先按本地策略过滤表示，再根据目标 capability 生成可请求描述。订阅端收到描述后按本地策略选择以下预取模式：

- metadata_only：只保存类型、大小、哈希、预览信息和来源，用户选择后再请求正文。
- eager_small：自动缓存低于类型阈值的正文，大内容按需获取。
- eager_all_within_limit：在缓存配额内尽量预取全部允许内容。

预取只写入应用缓存，不写入系统剪贴板。能力或策略不匹配时记录 ClipboardSkipped 及机器可读原因，本机剪贴板始终保持不变。

### 13.3 槽位事件与签名

每次非 Import 引起的本地剪贴板变化生成一次全局本机捕获：

- 随机 128 位 capture_id。
- 该设备身份下持久化单调递增的 origin_sequence。
- ClipboardBundle、规范化 capture_bundle_hash 和创建时间。

对于每个允许本机 publish 的同步组，再生成不可变 ClipboardSlotEvent：

- group_id 和 membership_epoch。
- 随机 128 位 event_id。
- capture_id、capture_bundle_hash、origin_device_id 和 origin_sequence。
- signed audience：至少允许接收一种表示的目标 device_id 排序并集。
- 每个 Representation descriptor 的 signed representation_audience。
- created_at 和 expires_at。
- ClipboardBundle 中允许该组查看的 Representation 描述与哈希。
- 规范化事件描述的 Ed25519 签名。

签名输入使用域分离前缀 `localdrop-clipboard-slot-event-v1` 和规范化二进制编码。正文通过 Representation 哈希验证。签名允许其他已授权成员在来源暂时离线时补发槽位状态，同时保持可验证来源；它不替代每条 QUIC 连接上的双向 TLS 身份认证。

一次真实本地复制在多个同步组中共享 capture_id 和 origin_sequence，但每组拥有不同 event_id、audience、过滤后的表示集合和签名。网络授权始终绑定 group_id；实现可以在本机按内容哈希复用加密缓存，不能跨组扩大正文可见范围。

origin_sequence 必须在发布任何组事件之前与 capture_id 一起原子持久化，并在同一 device_id 生命周期内永不回退或复用。若身份私钥仍在但序列状态丢失或损坏，Daemon 禁止继续 publish，只允许订阅；用户必须恢复数据库备份或生成新设备身份并重新配对，不能用 sequence=0 猜测恢复。

### 13.4 每设备单写者槽位

远端槽位的协议主键为 `(group_id, origin_device_id)`。只有 origin_device_id 对应私钥才能发布该槽位，origin_sequence 更大的事件替换同一来源的旧事件。不同来源的事件互不覆盖，因此三台设备同时复制会产生三个独立可选择槽位，不存在全组胜出者、因果竞争或时钟排序。

同一组内相同 `(group_id, origin_device_id, origin_sequence)` 出现不同 capture_id、capture_bundle_hash 或事件语义内容，视为 origin equivocation/ProtocolConflict：冻结该来源槽位、保留已经缓存的上一合法版本并显示安全告警，不采用最后到达者覆盖。

设备可以同时加入多个组。底层仍按 `(group_id, origin_device_id)` 隔离授权；UI 可以按 origin_device_id 聚合为一张设备卡片。因为 origin_sequence 对设备身份全局单调，可以标出该设备在所有本机有权访问组中的最高合法 sequence；同一 sequence 的多组事件必须具有相同 capture_id 和 capture_bundle_hash，但允许因组策略而携带不同 Representation 子集，UI 可以合并本机实际获权的表示集合。

不同组停留在不同 sequence 时不能静默合并或让较旧可用内容冒充设备最新剪贴板。设备卡片显示最高已知 sequence 及其 ready/blocked 状态，并允许展开选择具体组槽位；CLI 在无法唯一确定最新可 Import 事件时要求 `--group`。跨组发现相同 sequence 的 capture 身份不一致时显示安全告警。

每个远端槽位记录：event descriptor、availability、已缓存表示、预览、首次收到时间、有效截止和错误。availability 至少包括 metadata_only、partial、ready、stale、expired、blocked 和 protocol_conflict。

### 13.5 传播与离线补发

在线时，来源向所有符合发布/订阅策略的 active 组员发送 ClipboardSlotOffer。订阅者在 TTL 内保存该来源最新事件的描述和允许缓存的正文；启用 relay 的成员可以在来源离线时向 signed audience 中的其他订阅者补发。

转发者的 Offer 必须区分“已验证事件描述”和“本机实际持有的 representation_id 集合”。事件描述元数据按 10.3 对整个 signed audience 可见；接收端只有同时位于事件 audience 和该 Representation 的 representation_audience 中时才能请求正文。多个成员持有同一 representation_id 时，可以优先选择 RTT 较低者并在失败后切换。

连接建立后双方交换每个共同同步组的 slot cursors：

    origin_device_id -> (origin_sequence, event_id, availability)

- revision 或 membership_epoch 过旧时先同步组配置。
- 对端缺少某来源槽位或 sequence 较旧，且事件未过期时发送 Offer。
- 对端已有相同或更高 sequence 时不发送旧事件正文。
- 过期槽位只显示 stale/expired 状态或清理，不回放完整历史。

默认每组为每个 origin_device_id 只保存一个最新事件，不形成历史列表。同一来源更高 sequence 到达后，旧事件进入 Superseded；不再需要的预取可以取消，但已经被用户 Import 的文件缓存按引用规则继续保留。

Superseded 事件若存在尚未到期的出站或入站 Lease，可以只作为不可见的 lease object 保留所需正文到 source_expires_at/import_lease_deadline；它不能重新出现在设备最新槽位，也不能接受新 Lease。

事件过期不单独信任远端墙上时钟。接收端以签名 expires_at、首次接收时间和组策略最大 TTL 共同计算本地截止，并用单调时钟驱动运行期清理；明显时钟偏移不能无限延长寿命。

有效截止后不得为该槽位创建新的 ImportIntent。过期槽位可以保留不含正文的 stale/expired 卡片用于说明设备最后状态，但 Use 操作禁用，缓存正文按策略删除。

在槽位有效期内创建 ImportIntent 时，Daemon 按 SlotSelection 中不同 SourceRef 分组，为每组生成独立 lease_id，并向实际持有正文的来源或中继发送 ClipboardLeaseAcquire。来源只有在以下条件同时成立时才持久化并返回 Grant：事件尚未过本地有效截止、请求者当前仍在对应 representation_audience、membership_epoch/当前成员状态有效、所请求哈希确实已缓存、配额允许。

granted_duration_ms 由来源取 requested_duration_ms、本地最大 Lease 和资源策略的最小值，v1 默认最长 10 分钟，可以让 source_expires_at 晚于事件普通 TTL。source_expires_at 使用来源设备时间并由来源作为保留正文的最终权威；请求者收到 Grant 时同时用本地单调时钟建立不晚于 granted_duration_ms 的 import_lease_deadline。来源把出站 Grant 和 representation_id 集合持久化，在 source_expires_at 前保留对应正文；取消或完成后请求者发送 Release，但来源不能依赖 Release 才清理。

Lease 只延长缓存寿命，不冻结授权。成员被移除、设备被本地撤销、组 Tombstone 或类型策略收紧时，来源立即终止相关 Lease 和正文流；请求者的最终 ConfirmImport 也会失败。

本机 ImportLease 是 SlotSelection 中全部 Representation 所需 Grant 的集合，不绑定单一 event_id。ConfirmImport 只有在每个选中表示已 Ready、其 Grant 仍有效且当前成员/策略检查通过时才允许。任一必要 Lease 到期后 Import 进入 Unavailable，不能通过 Confirm 延长。

重连时请求者使用相同 lease_id 和不可变 Acquire payload 重试同一 responder；来源在 source_expires_at 前从持久化记录重放原 Grant，过期后返回 LeaseExpired。若来源丢失 Grant 或正文，请求者只能在原槽位仍未过期时向另一个声明持有相同哈希的成员申请新 lease_id；槽位已过期后不得新建或迁移 Lease。

### 13.6 设备选择器与显式 Import

Desktop 提供快速 Clipboard Switcher，按设备展示：设备名称、在线/离线状态、内容类型、大小、可安全展示的预览、更新时间、缓存就绪度和所属同步组。用户可以使用托盘面板、全局快捷键或主窗口选择任意有权限的设备槽位。

预览默认只显示截断文本和类型摘要；图片缩略图、文件名和富文本正文需要用户在隐私设置中开启。锁屏、系统通知和诊断报告不显示正文预览，私有格式只显示类型标识。

SlotSelection 不是模糊的“设备 ID”，而是固定的授权清单：origin_device_id、capture_id、origin_sequence，以及每个选中 Representation 的 representation_id、type_id、content_hash 和 SourceRef。SourceRef 固定为 `(group_id, event_id, membership_epoch)`；从多个组聚合表示时，每个表示分别携带自己的 SourceRef，不能用一个 event_id 代替整次 Import 的授权来源。SlotSelection 计算规范化 selection_hash。

选择槽位后执行一次 ImportTransaction：

1. 本地 UI、快捷键或 CLI 生成随机 import_id，向 Daemon 发送 CreateImportIntent(import_id, SlotSelection)。
2. Daemon 验证当前用户 IPC、SlotSelection 中每个 SourceRef、槽位有效期和当前策略，并持久化 ImportIntent；此时不创建 AdapterWriteCapability。
3. 获取缺失正文，逐表示验证对应 group_id、membership_epoch、事件签名、长度和内容哈希。新槽位到达不会悄悄改变已固定的 SlotSelection。
4. 如果创建 Intent 时全部表示已经 Ready，同一次用户动作可以继续最终确认；否则内容准备完成后进入 AwaitingConfirmation，必须由用户再次执行 ConfirmImport(import_id)。Daemon 重启后，Fetching 只有在所需 Grant 仍有效时继续 Fetching，正文全部 Ready 后进入 AwaitingConfirmation；Lease 无效则进入 Unavailable。原本 AwaitingConfirmation 的 Intent 保持等待，任何状态都不能后台续写剪贴板。
5. ConfirmImport 时再次检查每个 SourceRef 的当前成员状态、本地订阅/类型策略、有效 TTL 或 ImportLease，并确认 selection_hash 未改变。通过后由 Daemon 创建只存内存、数秒有效、单次消费的 AdapterWriteCapability。
6. 在本地事务中把 Import 标记为 Committing，并先持久化 before_generation、可读取的 before_hashes、预期写入哈希和 pending ImportedRemote SuppressionRecord；随后消费 capability 调用 Clipboard Adapter。
7. Adapter 必须返回 MutationOutcome：Unchanged、Committed(new_generation) 或 ChangedOrUnknown(observed_generation, optional_hashes)。只有平台证明 generation 和内容均未变化时才能返回 Unchanged。
8. Committed 时补记 generation 并把 Import 提升为 Imported。Unchanged 时标记 Failed 并删除 pending suppression。ChangedOrUnknown 时标记 FailedAfterMutation，保留 suppression 和受污染 generation 范围，显示“本机剪贴板可能已部分改变”；不得把观察到的内容发布为本机槽位。
9. 若 Adapter 调用期间 Daemon 崩溃，重启时用 before/expected 哈希和平台 generation 协调 pending 记录。无法证明 Unchanged 时一律按 ChangedOrUnknown 处理；AdapterWriteCapability 永不恢复。

后台 Offer、预取完成、设备上线和槽位 sequence 变化都不能创建 AdapterWriteCapability。换言之，最终写入一定可以追溯到当前用户 IPC 动作；大型或按需下载内容准备完成后不会在用户离开时突然覆盖剪贴板。

Import 产生的系统剪贴板通知在所有同步组中被抑制，不自动成为本机发布槽位，也不跨组桥接。此时本机 UI 可以显示“当前系统剪贴板取自设备 B”，但其他设备看到的本机槽位仍是本机最近一次真实应用复制。用户可以另行执行“作为本机槽位发布”，生成新的 capture_id 和 origin_sequence。

SuppressionRecord 包含 import_id、全部 SourceRef、before_generation、预期表示哈希集合、observed/committed generation 和短期截止时间。系统为一次写入产生的多次匹配通知全部被抑制。

ChangedOrUnknown 记录只有在 Adapter/Bridge 重新读取后证明当前 generation 未被调用改变，或观察到明确晚于受污染范围的新外部 generation 时才能清理。受污染 generation 永不发布；之后用户在应用中产生的明确新 generation，包括复制相同内容，仍被视为新的本地捕获。

v1 不模拟粘贴快捷键、不向其他应用注入按键，也不采用“保存本机剪贴板→临时替换→模拟粘贴→恢复”的竞态方案。用户 Import 后使用系统正常粘贴行为。

### 13.7 文件剪贴板

文件和目录剪贴板不能直接把发送端路径写入接收端，因为该路径在另一台设备上不存在。ClipboardSlotEvent 使用 Transfer 模块生成只读快照 Manifest。槽位描述和文件摘要实时同步；正文可以按策略预取，也可以在用户选择槽位后再传入受管理缓存：

    <cache-root>/clipboard/<group-id>/<event-id>/

本机系统剪贴板在文件全部下载、校验和路径预检完成前保持不变。随后每个 top_level_id 根据跨平台路径预检得到固定 mapped_name，Import 写入的路径精确为 `<event-id>/<mapped-name>`；单文件、单目录和混合事件分别写入一个或多个本地路径。缓存根本身不放入剪贴板。

`file://` 本地 URI 按文件剪贴板处理，HTTP 等非本地 URI 保留为 URI 表示。即使源文件管理器声明“剪切”，跨设备也一律降级为复制语义，绝不因远端粘贴删除或移动发送端原文件。

文件缓存使用权限受限目录，不自动执行。成功 Import 后，该 event_id 的缓存被 pin 到本机剪贴板 generation；当本机剪贴板再次变化或达到用户配置的最大 pin TTL 后才允许清理。文件剪贴板默认关闭，用户可以按同步组启用，并选择 metadata_only、按需下载或限定大小预取。

### 13.8 隐私与本地缓存

每个同步组和成员可以配置：

- publish、subscribe、bidirectional 或 disabled。
- metadata_only、eager_small 或 eager_all_within_limit 预取策略。
- 文本、富文本、图片、URL、文件列表和私有格式开关。
- 单项大小、文件总大小、文件数量和缓存配额。
- 离线槽位 TTL。
- 可识别时的来源应用允许/拒绝列表。
- 是否允许计费网络、电池模式或锁屏状态下预取。

建议初始默认值：组内方向为 bidirectional；纯文本、HTML、RTF、PNG 和非本地 URL 开启；小文本和小图片采用 eager_small；大内容 metadata_only；文件列表和私有格式关闭；离线槽位 TTL 为 5 分钟。用户可以在协议硬上限内调整，也可以为某台设备单独收紧。

密码管理器或平台标记为敏感的内容默认不发布；私有格式和文件剪贴板默认关闭。跨平台无法可靠识别所有敏感内容，UI 必须明确提示风险，并提供托盘“一键暂停发布/订阅”和缓存清理入口。

为了支持快速选择和 Daemon 重启后的短期槽位恢复，小型正文可以加密持久化。缓存密钥保存在操作系统凭据存储，正文使用成熟 AEAD 实现并绑定 group_id、event_id、origin_device_id 和类型作为附加认证数据。凭据存储不可用时退化为仅内存缓存，不能把剪贴板正文以明文写入 SQLite。文件缓存因需要被其他应用按路径读取，依靠文件权限、短 TTL、Import pin 和显式配额控制。

“发布当前本机剪贴板”可以保留为诊断或显式重新发布入口；它不会改变“远端更新永不自动覆盖本机剪贴板”的基本不变量。

## 14. 文件接收策略

普通 TransferOffer 只接受 direct-paired AuthScope。仅通过同步组获得 group-scoped trust 的成员不能借剪贴板权限发起普通文件投送；文件剪贴板必须携带有效 group_id、membership_epoch 和 ClipboardSlotEvent，并受文件剪贴板策略及配额约束。

每个已配对设备可以单独配置：

- 每次询问。
- 自动接收小于指定大小的文件。
- 始终自动接收。
- 拒绝所有传输。

默认始终询问。自动接收时文件仍只写入固定下载目录，不接受发送方指定绝对路径。

接收弹窗显示：

- 真实已验证设备身份。
- 文件数量和总大小。
- 文件类型摘要。
- 是否包含隐藏文件。
- 目标磁盘剩余空间。

## 15. 本地数据

使用 SQLite 保存结构化状态，文件本体不进入数据库。

核心表：

- local_identity_meta：非密钥身份信息。
- paired_devices：设备公钥、名称、权限和最后连接时间。
- local_revocations：用户明确撤销的 device_id，优先于远端组配置。
- sync_groups：group_id、Owner、revision、membership_epoch、active/left/tombstoned 本地状态和默认策略。
- sync_group_members：成员 device_id、SPKI、状态、方向、relay 开关和类型覆盖。
- sync_group_manifest_history：在最大槽位 TTL 内保留的签名 epoch 快照，用于验证旧事件而非恢复旧权限。
- local_clipboard_state：本机全局 origin_sequence、最近本地 capture_id 和最近 Import 来源。
- clipboard_slots：按 group_id/origin_device_id 保存最新 event_id、origin_sequence、availability、预览和过期时间。
- clipboard_slot_representations：每个槽位事件的类型、长度、哈希、缓存状态和加密对象引用，不保存可浏览历史。
- clipboard_imports：import_id、用户动作来源、规范化 SlotSelection、selection_hash、本地 import_lease_deadline、本机 clipboard generation 和结果。
- clipboard_leases：按 SourceRef 保存入站/出站 lease_id、requester/responder、representation_id 集合、selection_hash、granted_duration_ms、source_expires_at 和状态。
- clipboard_suppressions：Import Committing 前持久化的 before/expected 哈希、SourceRef、受污染 generation 范围和状态，用于崩溃协调与防止重新发布。
- transfers：传输状态、方向、统计和错误。
- transfer_files：文件清单、源快照、durable 块和完成状态。
- target_mappings：按 top_level_id 保存接收端固定路径映射、target_mapping_revision、冲突策略和提交状态。
- operation_results：需要跨重启去重的语义操作键和首次处理结果。
- settings：下载目录、端口和限制。

私钥不保存到普通 SQLite 字段。优先使用系统凭据存储。

文件传输历史默认只保存元数据和本地路径，不复制用户文件。用户删除历史记录不会删除已接收文件。

剪贴板正文不进入普通 SQLite 字段。允许离线补发的小型正文保存为加密缓存对象；过期、退出同步组、解除配对或用户执行“清除剪贴板缓存”时立即删除对应密钥引用和缓存。数据库迁移必须保持单写者、事务化和可回滚备份，不能因升级丢失身份与配对记录。

## 16. 桌面应用

桌面端使用 Tauri，Rust Core 承担身份、权限、状态机、缓存和剪贴板写入等可信逻辑。前端只展示 Daemon 提供的不可变 ViewModel，并发送带预期 revision 的用户命令；前端不得自行推断授权、拼接 SlotSelection、直接读取缓存正文或调用系统剪贴板 API。

桌面产品由三个互补入口组成：

- Clipboard Switcher：全局快捷键呼出的轻量浮层，是日常跨设备取用剪贴板的主入口。
- 主窗口：管理设备、同步组、传输、剪贴板策略和诊断信息。
- 托盘菜单：显示全局状态并提供暂停、清理缓存和打开其他入口等高频操作。

实时同步的含义是“设备卡片和可用正文实时更新”，不是自动切换或覆盖本机剪贴板。除用户明确执行“使用此剪贴板”外，前端不得制造任何看起来像已写入本机的状态。

### 16.1 信息架构

主窗口使用固定侧边导航，页面分为：

- 首页：当前系统剪贴板来源、最近可用设备槽位、在线设备和异常摘要。
- 设备：附近设备、配对状态、在线状态、设备级权限和快速发送。
- 同步组：创建组、邀请成员、成员方向、类型权限、TTL、预取方式和缓存配额。
- 剪贴板：完整的设备槽位列表、聚合来源、缓存状态和策略跳过原因，不提供可浏览历史正文。
- 传输：普通文件投送的等待确认、进行中、完成、失败和恢复。
- 设置：本机名称、下载目录、快捷键、网络、缓存、隐私、外观和诊断。

Clipboard Switcher 与主窗口“剪贴板”页消费同一份 ClipboardSlotViewModel。浮层只保留搜索、设备选择、Import 和状态反馈；复杂配置通过“在主窗口中管理”进入对应页面，避免把快速操作变成设置面板。

### 16.2 Clipboard Switcher 布局

Switcher 默认宽度约 520 px，最大高度为当前工作区高度的 70%，内容超出后内部滚动；在小屏设备上退化为贴近屏幕边缘的单列面板。窗口失焦时关闭，但 Fetching、AwaitingConfirmation 和 Committing 操作由 Daemon 继续管理，重新打开后按 import_id 恢复准确状态。

    ┌─ Clipboard Switcher ──────────────────────────────────┐
    │ 搜索设备或类型…                        发布暂停  ⚙     │
    ├─ 当前系统剪贴板 ──────────────────────────────────────┤
    │ 来自本机应用 / 取自 Work PC · 12 秒前   文本、HTML    │
    │ “当前内容的安全预览……”                 [仅状态信息]   │
    ├─ 设备剪贴板 ──────────────────────────────────────────┤
    │ ● MacBook Pro                         在线 · 8 秒前    │
    │   “selected text preview…”             文本、HTML     │
    │   已就绪 · Personal + Work             [使用]         │
    ├────────────────────────────────────────────────────────┤
    │ ○ Work PC                              离线 · 2 分前   │
    │   PNG · 3.2 MiB                        仅元数据        │
    │   缓存后需再次确认                     [获取并使用]    │
    └────────────────────────────────────────────────────────┘

顶部区域包含搜索框、暂停状态和设置入口。搜索只匹配已验证设备名、用户备注、同步组名和类型摘要，不搜索未展示的剪贴板正文。暂停发布和暂停订阅必须分开显示；任一暂停时使用文字和图标提示，不能只改变颜色。

“当前系统剪贴板”是独立只读区域，不参与远端设备卡片排序：

- 真实本机应用复制显示“来自本机应用”；可识别来源应用时仅在本地显示其名称。
- Import 后显示“取自设备 B”，直到本机剪贴板发生下一次可证明的外部变化。
- “本机最近发布槽位”与“当前系统剪贴板”可能不同，UI 不得将两者合并。用户从远端 Import 后，其他设备仍看到本机最近一次真实复制，除非用户另行选择“作为本机槽位发布”。
- 前端拿不到正文读取权限时只展示 Daemon 已生成的安全预览和表示摘要。

设备区域按 origin_device_id 每台设备一张卡，而不是把所有设备的事件混成一个全局剪贴板列表。卡片至少展示：

- 已验证设备名、用户备注、平台图标和 identity 状态。
- 在线、离线、连接中或已撤销状态；在线状态与内容可用性分开表达。
- 内容类型、总大小、安全预览、捕获时间和相对时间。
- metadata_only、partial、ready、stale、expired、blocked 或 protocol_conflict。
- 表示所在同步组；来自多个组时显示聚合标签和可展开明细。
- 主操作、当前进度和不可操作原因。

默认排序为：安全异常、正在进行的 Import、可用且较新的设备、仅元数据/部分就绪设备、stale/expired 设备。用户可以固定常用设备；固定只改变排序，不改变授权和来源选择。时间排序使用 Daemon 已校正的槽位顺序与接收信息，不能用远端 created_at 在不同设备间宣称全局先后关系。

### 16.3 卡片状态与操作

卡片状态由 Daemon 明确给出，前端不得根据“是否在线”猜测：

| 状态 | 展示 | 主操作 |
| --- | --- | --- |
| `metadata_only` | 已收到类型、大小和来源，正文未缓存 | “获取并使用”；创建 ImportIntent 并显示下载 |
| `partial` | 部分允许表示已缓存，完整选择尚未 Ready | “继续获取”；允许展开查看缺少的表示和原因 |
| `ready` | 当前 SlotSelection 所需表示已验证并在有效 Lease 内可用 | “使用”；同一次明确点击可直接 ConfirmImport |
| `stale` | 来源离线或状态较旧，但事件仍在本地有效截止内 | 若正文可用则“使用”，否则“尝试获取” |
| `expired` | 已超过可创建新 ImportIntent 的截止时间 | 禁用操作，显示“已过期，等待该设备再次复制” |
| `blocked` | 类型、成员权限、大小或本地策略拒绝 | 禁用操作并提供可定位的策略原因；有权限时可进入设置 |
| `protocol_conflict` | 相同 sequence 或跨组捕获身份发生冲突 | 冻结操作，显示安全告警和诊断入口 |

卡片不能用模糊的“已同步”描述 metadata_only；正文是否已缓存、能否立即使用、是否还需确认必须分别表达。设备离线但已缓存且授权仍有效时仍可使用；设备在线也不代表正文已经可用。

同一槽位含多个可用 Representation 时，默认由 Daemon 根据本机 capability 和用户策略选择能保留最多语义的兼容组合。卡片的展开区域列出纯文本、HTML、RTF、图片、URL、文件列表和私有格式等所有本机可读取表示，并允许用户为本次 Import 取消不需要的表示。每种表示都显示允许、被策略关闭、平台不兼容、超限或尚未缓存等状态；任何用户可配置的类型都来自 capability/type registry，前端不硬编码一个封闭格式列表。

用户可以在同步组默认策略和设备覆盖策略中选择允许发布、订阅和预取的具体类型。新发现但本机已支持安全读取的类型自动出现在配置列表中；应用私有格式仍需满足 13.1 的安全序列化和兼容条件。策略变化只影响后续发布、获取和最终 ConfirmImport，不伪装成协议能力变化。

### 16.4 取用交互

Switcher 支持完整键盘操作：全局快捷键打开后焦点位于搜索框；上下方向键移动卡片；左右方向键或展开键查看表示；Enter 执行主操作；Esc 先取消当前选择，再关闭窗口。快捷键都可配置，平台默认值必须经过冲突检测，不能强占系统或常见辅助功能快捷键。

选择设备槽位时遵循以下流程：

1. 前端从当前卡片获取 Daemon 签发的 selection_token 和 object_revision，发送 CreateImportIntent；不能自行采用“该设备此刻最新事件”。
2. Daemon 将 token 解析为精确 SlotSelection 并返回 import_id、固定来源摘要和 Import 状态。用户操作发生后到命令被处理前若槽位已经变化，Daemon 返回 RevisionChanged，前端展示新旧摘要并要求重新选择，不能静默改取更新后的内容。
3. 如果所选表示已 Ready，该次点击可连续完成确认和写入；按钮短暂进入“正在写入”，禁止重复提交。
4. 如果需要下载，卡片显示逐表示进度、来源设备和“取消获取”。完成后状态变为 AwaitingConfirmation，并通过浮层内提示和应用内通知告知“内容已就绪，确认使用”；不会后台写入本机剪贴板。
5. 用户再次点击“使用已就绪内容”时发送 ConfirmImport。若授权、策略、TTL 或 Lease 已改变，显示 Unavailable 原因并保持本机剪贴板不变。
6. Imported 后显示“已取入本机剪贴板”，当前系统剪贴板区域更新为“取自设备 B”；焦点回到原应用时不模拟粘贴，用户自行使用系统粘贴操作。

Fetching 期间新 sequence 到达只更新设备卡片的“有更新内容”徽标，现有 import_id 仍绑定旧 SlotSelection。用户可以取消旧 Import 再选择新内容，前端不得自动重定向。AwaitingConfirmation 必须显示被固定内容的来源、类型、大小和捕获时间，避免用户误认成当前最新槽位。

导入状态的用户反馈统一为：

- `Fetching`：显示总进度和当前表示，可取消；取消不改变本机剪贴板。
- `AwaitingConfirmation`：正文已经校验，但必须再次明确确认；提供“使用”和“丢弃”。
- `Committing`：短暂禁用重复操作，等待 Adapter 的 MutationOutcome。
- `Imported`：显示成功来源和可选的“作为本机槽位发布”，后者必须是单独动作。
- `Unavailable`：显示授权过期、来源丢失、Lease 到期或策略变化等具体原因，可返回最新设备槽位。
- `Failed`：明确说明本机剪贴板未改变，并允许在槽位仍有效时重试。
- `FailedAfterMutation`：高优先级提示“本机剪贴板可能已部分改变”，提供检查当前剪贴板和诊断入口，不能显示普通成功提示。

### 16.5 多同步组聚合

同一 origin_device_id 在多个同步组中的卡片默认聚合展示，但授权数据始终保持按组隔离：

- 相同 origin_sequence、capture_id 和 capture_bundle_hash 的多组事件可以合并展示本机实际获权的 Representation 并集；每个表示仍携带自己的 SourceRef。
- 最高 sequence 在不同组中的表示可以互补，但只有身份一致且 Daemon 能生成无歧义 SlotSelection 时才提供统一“使用”操作。
- 不同组停留在不同 sequence 时，卡片标题展示设备最高已知 sequence；展开区域按组列出各自内容和可用性，不把旧组中的可用正文伪装为设备当前最新剪贴板。
- 多个组都能提供相同表示时，由 Daemon 选择有效期、缓存完整度和连接质量更合适的 SourceRef；UI 展示实际来源组，但不要求普通用户手动选择网络发送方。
- 同一 sequence 的 capture 身份冲突进入 protocol_conflict，卡片使用安全告警样式并禁止聚合 Import。

用户可以切换“按设备聚合”和“按同步组查看”。该视图选项只改变展示，不改变底层槽位主键、权限或缓存复用规则。

### 16.6 预览与隐私

安全预览由 Daemon 在捕获或验证正文时生成有界摘要，前端只接收已按本地策略处理的字段：

- 纯文本默认显示有限字符和有限行数，移除不可见控制字符；搜索不索引截断范围以外的正文。
- HTML 和 RTF 默认显示其纯文本摘要，不渲染远端标记、不加载远端资源。
- 图片默认只显示类型、尺寸和大小；缩略图可在隐私设置中开启，并由已验证本地缓存生成。
- 文件剪贴板默认只显示数量、总大小和类型摘要；文件名预览可单独开启，路径只显示安全的相对项名。
- URL 默认显示规范化主机和截断地址，不自动请求网页标题、图标或任何远端资源。
- 私有格式只显示类型标识、大小和兼容性，不尝试通用预览。
- 锁屏、系统通知、任务栏缩略图和诊断导出永不显示正文预览；通知只说“设备 B 的剪贴板已就绪”。

UI 在同步组隐私设置中说明：事件描述中的设备、类型、大小和时间等元数据可能对 signed audience 可见，即使某个正文 Representation 只授权给部分成员。密码管理器等敏感来源的检测只能降低风险，不能承诺识别所有秘密内容。

### 16.7 视觉语言与可访问性

第一版采用克制的系统原生风格，不依赖复杂动效。视觉层使用语义 Design Token，亮色和暗色主题分别映射，组件不得直接硬编码颜色值：

- `surface/background/elevated`：窗口、卡片和浮层层级。
- `text/primary/secondary/disabled`：正文层级。
- `accent`：当前焦点和主要操作。
- `success`：Ready、Imported。
- `warning`：Partial、Stale、AwaitingConfirmation。
- `danger`：Blocked、Expired、FailedAfterMutation 和安全冲突；安全冲突还必须使用专用图标和标题，不能与普通失败完全同形。
- `info`：MetadataOnly、Fetching 和连接过程。

状态不能只靠颜色区分，必须同时使用图标、文字和可访问名称。焦点环在亮暗主题和高对比模式下都满足平台可见性要求；正文和关键控件对比度至少达到 WCAG 2.2 AA。支持系统字体缩放、减少动态效果、屏幕阅读器和全键盘操作；相对时间的可访问文本包含完整本地时间。

卡片只对进入、退出和进度变化使用短暂动效；收到新远端槽位时不得抢焦点、改变当前键盘选择或把列表滚回顶部。protocol_conflict、FailedAfterMutation 等关键提示使用非自动消失区域，普通成功提示可以短暂显示。

### 16.8 主窗口页面细节

首页把“当前系统剪贴板”和“设备最新槽位”分成两个区域，并显示发布/订阅是否暂停。首页的快捷 Import 与 Switcher 使用相同事务流程，不能提供绕过二次确认的另一套路径。

设备页按 Nearby、Paired、Offline 和 Attention 分组。未配对设备只允许进入配对；已配对设备可以配置备注、普通文件接收策略、所属同步组和设备级剪贴板策略覆盖。解除配对、撤销设备和移出同步组是不同操作，UI 必须解释影响范围并在危险操作前确认。

同步组详情使用成员列表和策略矩阵：行是内容类型或类型族，列是 publish、subscribe、prefetch 和大小限制。矩阵列出 capability registry 中所有本机支持复制的类型，并允许用户逐项配置；组默认策略与设备覆盖同时存在时显示最终生效值及其来源。成员方向、正文授权和 relay 权限分开编辑，避免一个含糊的“允许同步”开关。

剪贴板页提供与 Switcher 相同的设备卡片以及更多诊断字段：origin_sequence、来源组、事件有效截止、Representation 缓存情况和机器可读跳过原因。默认不形成剪贴板历史浏览器；Superseded lease object 不出现在列表中。

普通文件传输中心与 Clipboard Switcher 保持概念分离。普通 TransferOffer 在传输页展示接受、拒绝、重命名目标、进度、暂停、恢复和取消；文件剪贴板的下载进度显示在对应设备卡片及 Import 详情中，不伪装成普通文件投送。发送面板支持拖拽文件/目录和选择目标设备，不能把“发送文件”误标为修改对方剪贴板。

设置页至少包括：

- 剪贴板：全局 publish/subscribe、每类型策略、预取阈值、TTL、文件剪贴板和私有格式。
- 快捷键：打开 Switcher、选择固定设备、暂停发布、暂停订阅；检测冲突并允许清除绑定。
- 隐私：文本、图片、文件名预览，锁屏行为，计费网络和电池模式。
- 存储：缓存占用、配额、当前 pin、清除剪贴板缓存；清理前展示对 Fetching/Import 的影响。
- 网络与传输：下载目录、端口、并发、普通文件自动接收策略。
- 外观与辅助功能：跟随系统/亮色/暗色、高对比和减少动效。
- 诊断：连接、身份、协议冲突和最近机器可读错误；正文与密钥永不进入普通诊断导出。

### 16.9 托盘、通知与快捷设备

托盘图标只表达全局运行、全部暂停和需要关注三类高层状态；具体设备状态通过菜单或 Switcher 查看，避免用一个图标承载过多含义。托盘菜单提供：

- 打开 Clipboard Switcher。
- 显示主窗口。
- 暂停/恢复发布。
- 暂停/恢复订阅。
- 固定设备的最新槽位快捷入口；选择后仍遵循正常 Import 和确认规则。
- 显式“将当前本机剪贴板作为新槽位发布”。
- 清除剪贴板缓存。
- 暂停/恢复普通文件接收。
- 退出桌面 UI，或在平台允许时另行选择“退出 UI 与 Daemon”；两者含义必须明确。

通知只用于需要用户返回处理的事件，例如按需正文已就绪、普通文件等待接收、授权失效或安全冲突。远端每次复制不弹系统通知；实时变化只安静更新设备槽位。点击“内容已就绪”通知只打开固定 import_id 的确认界面，不直接 ConfirmImport。

用户可以为常用设备绑定快捷键。快捷键的语义是“打开并聚焦该设备槽位”，或者在用户明确启用快速使用后对已 Ready 内容发起一次可审计 Import；未 Ready 内容仍进入 Fetching，并在完成后要求再次确认。设备槽位过期、冲突或选择不唯一时只打开 Switcher 说明原因，不退化为选择任意可用组。

### 16.10 前端状态一致性

Tauri 前端启动、唤醒、失焦后恢复或检测到 event_revision 间隙时，必须先请求完整 UiSnapshot，再订阅后续增量事件。Snapshot 包含 snapshot_revision；前端只应用 event_revision 更大的连续事件，发现乱序或缺口就丢弃派生状态并重新取 Snapshot。

每个可变对象携带 object_revision。用户命令包含界面所见 revision；Daemon 返回 RevisionChanged 时前端重新加载并要求用户确认，不做乐观重放。主窗口和 Switcher 同时打开时都以 Daemon 状态为准，通过 import_id 展示同一个 Fetching 或 AwaitingConfirmation 操作。

所有页面都要定义加载、空、部分失败、离线和权限不足状态：

- 首次连接 Daemon 时显示骨架和“正在连接本机服务”，不把它说成正在搜索远端设备。
- Daemon 不可用时保留设置和诊断入口，但禁用会产生误导的 Import/发送操作。
- 没有设备时给出配对入口；有设备但没有有效槽位时说明“等待设备复制内容”。
- 单个设备失败不遮挡其他设备卡片；全局错误只用于 Daemon、数据库或身份等确实影响全局的故障。
- UI 关闭或崩溃不取消已经持久化的传输和 ImportIntent；重新打开后从 Daemon 恢复，不使用前端本地缓存猜测终态。

第一版的验收重点是：远端槽位更新及时但绝不自动覆盖本机剪贴板；任意设备都能被清楚选择；Ready 与仅元数据不会混淆；按需内容准备后一定再次确认；多组聚合不扩大权限；失败、取消和安全状态准确且可恢复。

### 16.11 Android 前台客户端

Android 与桌面共享 React 功能组件、语义 Design Token、UiSnapshot、Import 状态机和 Rust Core，不复制协议或授权逻辑。平台差异通过 Clipboard、Lifecycle、ShareIntent、Notification 和 Storage Adapter 隔离；业务组件只读取 capability，不通过 user-agent 猜测安全能力。

Android 第一阶段采用前台实时模型：

- 应用进入前台后自动连接本地 Rust Core，执行设备发现、连接恢复并请求完整 UiSnapshot；完成前显示“正在恢复设备连接”，不能展示虚假的在线状态。
- ForegroundLive 期间实时接收设备槽位事件，并在平台允许时观察本机剪贴板变化；本机真实复制按组策略自动发布为 Android 自己的设备槽位。
- 应用进入后台后转为 Suspended，停止承诺实时发现、剪贴板捕获和事件接收；不显示“后台同步中”。回到前台重新从 Snapshot 恢复。
- Android 系统拒绝或限制剪贴板读取时，显示 ClipboardCapabilityLimited，并保留用户明确点击“发布当前剪贴板”的入口；不要求辅助功能、默认输入法或设备管理权限。
- 远端槽位写入 Android 系统剪贴板仍必须由用户选择并经过正常 Create/Confirm Import；生命周期恢复、通知和 Share Intent 都不能直接写入。

Android 使用底部导航和全屏 Clipboard Switcher，不使用桌面侧边栏、托盘、窗口关闭或全局键盘快捷键文案。所有主要触控目标至少 44 CSS px，关键操作常驻可见，不依赖 hover；适配状态栏、导航栏和显示挖孔的 safe-area inset。

Android 首页优先显示：ForegroundLive/Reconnecting/Suspended 状态、当前系统剪贴板、发布当前剪贴板、远端设备槽位和最后同步时间。设备、同步组、传输和设置页继续存在，但复杂策略矩阵在窄屏上改为逐类型详情，不横向压缩成不可操作表格。

系统分享入口用于从浏览器、相册和文件管理器发布内容。文本和 URL 可以在有界校验后形成本机槽位；图片与文件必须持有系统授予的 URI 权限、流式复制到受管理缓存并完成哈希后才能发布。第一阶段工程应先定义 SharePayload 边界；Android SDK 可用后再生成并验证 Intent Filter、URI grant 和 Gradle 配置。

Android 不生成或包含 iOS 工程、Entitlement、Share Extension 或 APNs 依赖。未来若考虑 iOS，必须重新设计后台恢复和系统 Paste 权限，不能直接宣称复用 Android 行为。

## 17. CLI

主要命令：

- airdrop devices：列出附近和已配对设备。
- airdrop pairing allow --duration 120s：临时打开 Responder 配对窗口。
- airdrop pair <device>：发起配对。
- airdrop group create <name>：创建同步组。
- airdrop group invite <group> <device>：邀请已配对设备。
- airdrop group leave <group>：本机立即退出同步组。
- airdrop group delete <group>：Owner 发布 GroupTombstone 并删除组。
- airdrop group policy <group> ...：配置方向、类型、大小和 TTL。
- airdrop clipboard slots：列出有权限设备的最新槽位、类型、时间和缓存状态，不输出正文。
- airdrop clipboard use <device> [--group <group>]：显式把所选设备槽位取入本机剪贴板；多个组无法唯一确定时必须指定 group。
- airdrop clipboard import confirm <import-id>：按需内容准备完成后执行最终确认；交互式 use 也可以等待并提示确认。
- airdrop clipboard pause / resume：暂停或恢复发布与订阅。
- airdrop send <device> <paths...>：发送文件或目录。
- airdrop clipboard publish <group>：可选地把当前本机剪贴板显式发布为新槽位。
- airdrop transfers：查看传输状态。
- airdrop cancel <transfer-id>：取消传输。
- airdrop trust revoke <device>：解除配对。
- airdrop doctor：检查防火墙、端口、mDNS 和存储权限。

CLI 默认调用本地 Daemon，不重复维护网络连接。

## 18. 推荐项目结构

    AirDrop/
    ├── crates/
    │   ├── protocol/
    │   ├── core/
    │   │   └── src/
    │   │       ├── identity/
    │   │       ├── discovery/
    │   │       ├── transport/
    │   │       ├── pairing/
    │   │       ├── sync_group/
    │   │       ├── clipboard/
    │   │       ├── transfer/
    │   │       └── storage/
    │   ├── platform/
    │   ├── daemon/
    │   └── cli/
    ├── desktop/                 # 历史目录名；Tauri 桌面与 Android 共享应用
    │   ├── src/
    │   │   ├── app/             # 窗口入口、路由和错误边界
    │   │   ├── features/
    │   │   │   ├── clipboard/   # Switcher、设备卡片和 Import 状态
    │   │   │   ├── devices/
    │   │   │   ├── groups/
    │   │   │   ├── transfers/
    │   │   │   └── settings/
    │   │   ├── ipc/             # UiSnapshot、增量事件和类型安全命令
    │   │   ├── components/      # 无业务权限判断的通用组件
    │   │   ├── styles/          # 语义 Design Token 和主题映射
    │   │   └── test/
    │   ├── src-tauri/           # Tauri 权限、桌面窗口和 Android 工程
    │   ├── package.json
    │   └── vite.config.ts
    ├── tests/
    ├── docs/
    ├── DESIGN.md
    ├── Cargo.toml
    └── README.md

建议使用 Cargo Workspace，但第一阶段不为每个领域建立独立 crate，避免过早形成大量依赖边和循环抽象。protocol 只保存规范化编码、消息模型和版本兼容规则，不依赖网络与存储；core 内部按领域模块隔离；platform 保存文件系统、凭据、剪贴板和 IPC Adapter；daemon 是唯一组合根。

desktop/src 按业务功能垂直切分，不建立一个包含全部远端状态的万能前端 Store。目录名保留是为了避免无价值迁移，不表示只支持桌面；ipc 层负责 Snapshot + event_revision 的一致性和生成的 TypeScript 类型，feature 只能通过 ipc command 改变状态。通用 components 不接受 group_id、SourceRef 等授权对象，避免权限逻辑泄漏到视觉组件。桌面主窗口、桌面 Switcher 和 Android 全屏选择器共享 feature 与 ipc 层，不复制 Import 流程。

当某个 core 模块已经形成稳定公共接口、拥有独立测试需求或需要被其他项目复用时再拆 crate。Transfer 通过抽象 ByteStream、RandomAccessSource 和 TargetFs 接口测试，不强绑定 Quinn 或具体平台 API。

## 19. 推荐技术选择

- 异步运行时：Tokio。
- QUIC：Quinn。
- TLS：Rustls。
- 身份签名：Ed25519 成熟实现。
- 发现：mdns-sd 或同类跨平台库。
- 哈希：BLAKE3。
- 序列化：Serde。
- 本地数据库：SQLite，使用 sqlx 或 rusqlite。
- CLI：Clap。
- 桌面与 Android 应用容器：Tauri 2；Android 不启用永久前台服务。
- 前端：TypeScript、React 和 Vite；业务状态以 Daemon Snapshot/事件为权威，React 本地状态只保存搜索词、展开项和焦点等短期视图状态。
- 样式：CSS 自定义属性承载语义 Design Token，优先复用可审计的无样式可访问组件原语，不引入同时控制业务状态和视觉状态的重量级 UI 框架。
- IPC 类型：从 Rust IPC DTO/Schema 生成或在构建期校验 TypeScript 类型，禁止长期手工维护两份易漂移的状态枚举。
- Android 工具链：受支持的 JDK、Android SDK/NDK、Tauri CLI 生成工程和 Rust Android target；版本锁定在仓库配置中，不能依赖开发机隐式默认值。
- 剪贴板：使用跨平台库并为平台差异预留 Adapter。
- 缓存加密：使用成熟 AEAD 实现，密钥由系统凭据存储保护。
- 槽位顺序：每个设备使用持久化 origin_sequence 单调更新自己的槽位；不同设备槽位不参与全局排序。
- 错误：库层使用具体错误枚举，应用边界使用 anyhow 一类上下文封装。

实际选库时需要核查许可证、维护状态和目标平台支持，不把库名称写成不可替换的协议要求。

## 20. 状态机

发送端文件传输状态：

    Created
      -> PreparingManifest
      -> Offered
      -> AwaitingDecision
      -> Transferring
      -> AwaitingVerification
      -> Completed

接收端文件传输状态：

    OfferReceived
      -> AwaitingLocalDecision
      -> Accepted
      -> Receiving
      -> Verifying
      -> CommitIntent
      -> Materializing
      -> TreePrepared
      -> Renamed
      -> FilesystemApplied
      -> Committed

Rejected 只允许从 AwaitingLocalDecision 产生；Cancelled 只允许由有权限的一方从非终态产生；Failed 表示不能按当前 Manifest 自动恢复的错误。PreparingManifest、Transferring、Receiving、Verifying 和 AwaitingVerification 可以进入 Paused，并保存 resume_from，条件恢复后回到对应状态。CommitIntent 之后不回退到普通传输状态，而进入启动协调恢复流程。

本机发布槽位事件状态：

    Captured -> Filtered -> Offered -> ActiveSlot
                  |          |
                  +-------> Skipped / Expired / Superseded

远端槽位缓存状态：

    OfferReceived -> MetadataReady -> Prefetching -> Partial / Ready
                            |              |
                            +----------> Blocked / Failed / Expired

显式取用状态独立建模：

    ImportRequested -> Fetching -> AwaitingConfirmation -> Committing -> Imported
           |             |                |                 |
           +----------> Cancelled / Failed / FailedAfterMutation / Unavailable

多目标发布状态按 `(event_id, peer_device_id)` 保存，不能用一个全局“已发送”布尔值表示。远端槽位按 `(group_id, origin_device_id)` 保存；Import 只有经过 CreateImportIntent/ConfirmImport，并由 Daemon 创建有效 AdapterWriteCapability，才能进入 Committing/Imported。

状态迁移由 Daemon 单点执行并与业务数据写入同一 SQLite 事务。UI 只能发送命令。每个对象的状态改变携带单调递增 revision；Daemon 的 IPC 事件另有全局 event_revision，分别解决对象并发修改和客户端事件乱序。

## 21. 错误处理

- mDNS 不可用：应用保持运行，并提示使用诊断功能；后续可增加手动地址。
- 对端身份不匹配：立即断开，显示安全告警，不自动重新配对。
- 连接中断：保留临时文件和 ResumeState，进入 Paused。
- 磁盘空间不足：发送接受响应前检查，传输中也持续处理写入错误。
- 文件读取期间源文件变化：停止该文件，标记 SourceChanged。
- 文件哈希不一致：保留或删除临时文件由安全策略决定，默认删除损坏块并允许重试。
- 文件名非法：在接受前拒绝 Manifest。
- 目标在预检后出现：原子 no-replace 返回 TargetChanged，保留已验证对象并等待用户取消或生成新的 target_mapping_revision。
- 剪贴板类型不兼容或被策略阻止：远端槽位记录 blocked/ClipboardSkipped，不覆盖本机剪贴板，也不回退到不受允许的表示。
- 预取失败：槽位保留 metadata/partial 状态，允许用户选择时重试，本机剪贴板不受影响。
- Import 正文下载、哈希或平台写入失败：Import 失败并保留原剪贴板；槽位本身仍可在后续重试。
- 文件剪贴板缓存空间不足：不创建半完成的本地文件列表；清理过期缓存后仍不足则跳过。
- 远端 origin_sequence 回退、计数溢出或相同 sequence 对应不同内容：冻结该来源槽位并记录安全诊断。
- GroupManifest revision 或 membership_epoch 过旧：先请求最新配置，期间不传输剪贴板正文。
- UI 崩溃：Daemon 和传输继续运行。

所有网络任务通过 CancellationToken 管理，应用退出时先停止接收新任务，再有限时间等待数据库和临时状态写入。

## 22. 安全设计

- 发现信息不被视为可信身份。
- 首次配对必须有人在两端确认验证码。
- 已配对设备使用固定身份公钥验证。
- 全部传输使用 TLS 1.3 加密。
- 配对不会自动授予剪贴板权限；设备必须显式接受同步组邀请。
- GroupManifest 和可转发 ClipboardSlotEvent 使用现有 Ed25519 身份密钥签名，并绑定版本、group_id 和 membership_epoch。
- 本机发起或收到成员删除、解除配对和本地暂停后，会立即阻止新的剪贴板正文发送；离线成员的远端撤销状态必须明确展示。
- 加入组后自动镜像授权设备槽位，但网络更新永不自动写入本机系统剪贴板；私有格式和文件剪贴板默认关闭，本地策略只能收紧权限。
- Clipboard Adapter 写入必须携带由 Daemon 根据当前用户确认创建的一次性 AdapterWriteCapability，网络任务不能创建或恢复该能力。
- 不接受发送方提供的绝对路径。
- 规范化相对路径并拒绝父目录跳转。
- 发送和接收都使用平台无跟随、句柄式路径 Adapter，防止符号链接、Junction 和 TOCTOU 绕过。
- 不自动打开或执行接收文件。
- 文件投送自动接收默认关闭。
- 剪贴板加密缓存不与日志、历史记录或崩溃报告混合保存，过期后删除。
- Daemon IPC 仅允许当前系统用户访问。
- 日志不记录剪贴板正文、文件正文或私钥。
- 协议解析对长度、数量、嵌套和字符串大小设置上限。
- 配对、请求和失败尝试均有限速。

## 23. 性能与资源控制

- 文件数据流式读取和写入，不按文件大小分配内存。
- 所有 Channel 有界。
- QUIC 连接、并发文件数、块缓存和 Hash 任务数可配置并有安全默认值。
- 同步组成员数、同时活动 Blob、每槽位事件表示数、剪贴板正文大小和缓存总量均有硬上限。
- 每组为每个 origin_device_id 只保留最新槽位和必要的短暂传输对象，内存与磁盘占用不随复制次数无限增长。
- 剪贴板监听可以使用短稳定窗口合并一次应用操作产生的连续系统通知，但用户产生的不同内容哈希不能被错误折叠。
- 哈希线程与异步网络任务分离，避免阻塞 Tokio Runtime。
- 发送目录时逐步构建清单；TransferOffer 前必须得出准确文件数和总大小，完整 Manifest 通过有界流传输。
- 历史记录和未完成临时文件有清理策略。

性能目标不写死绝对带宽，而以“接近同机环境下普通局域网文件复制的有效吞吐，且内存不随文件大小增长”为验收方向。

## 24. 可观测性与诊断

本地诊断信息包括：

- mDNS 发布和发现状态。
- 当前监听地址和 QUIC 端口。
- 在线设备及其身份验证状态。
- active/draining 连接及连接仲裁原因。
- 同步组 revision、成员在线状态和策略匹配结果。
- 活跃连接、RTT 和传输速率。
- 传输块重试和校验失败。
- 临时目录占用。
- Import 通知抑制次数。
- ClipboardSlot Offer、MetadataReady、Partial、Ready、Skipped、Expired 计数。
- ImportRequested、Imported、Failed 计数和不含正文的原因分类。

doctor 命令执行：

- 检查端口绑定。
- 检查局域网接口。
- 检查 mDNS 组播能力。
- 检查防火墙常见问题。
- 检查下载目录和临时目录权限。
- 检查系统凭据存储是否可用。

诊断报告默认不包含设备公钥全文、文件名和剪贴板内容。

## 25. 测试策略

### 单元测试

- 协议消息编码、大小限制、版本兼容和未知字段规则。
- Manifest、GroupManifest 和 ClipboardSlotEvent 规范化编码、哈希与签名固定向量。
- 路径规范化、临时对象布局和目录穿越防护，包括文件 `a` 与目录 `a.part` 等映射案例。
- 发送端、接收端、提交事务和剪贴板投递状态机。
- ResumeState 合并、范围分页和语义幂等处理。
- 连接仲裁在双方同时拨号、多地址和 Daemon 重启情况下得出相同结果。
- 每来源 origin_sequence 单调更新、回退拒绝和相同 sequence 内容冲突。
- GroupTombstone 和本地撤销能够阻止旧 GroupManifest 回滚恢复权限。
- ClipboardCapabilities 与策略合并，确保本地策略只能收紧权限。
- AdapterWriteCapability 只能由 Daemon 在本地 Create/Confirm Import 流程中创建，网络 Offer 和预取完成不能触发 Clipboard Adapter 写入。
- expired 槽位不能创建 ImportIntent；有效期内创建的 Intent 只能在固定 ImportLease 内完成，Lease 到期进入 Unavailable。
- 多 SourceRef Import 分别取得不可变 Lease Grant；来源在普通 TTL 后、source_expires_at 前仍保留正文，重连重放未过期 Grant。
- Import 产生的多次系统通知全部抑制，之后真实应用复制相同内容仍生成新 capture_id。
- Adapter 在多表示写入中部分改变剪贴板后返回失败时进入 FailedAfterMutation，受污染 generation 不得发布。
- 配对 TLS exporter 派生、角色排序、版本绑定和确认状态机。

### 集成测试

- 三个本地 Daemon 完成发现、配对和建组；任意设备复制后，另外两台更新该来源槽位，但本机系统剪贴板字节保持不变。
- Owner 分别配对两个成员后，两个成员通过签名 GroupManifest 建立 group-scoped 直连，无需再次两两配对，且不能越权发起普通文件投送。
- 三台设备近似同时复制不同内容，所有成员最终看到三个独立设备槽位，不发生相互覆盖或循环风暴。
- 来源设备离线后，由其他持有最新有效事件的成员向重新上线设备补发。
- send_only、receive_only、disabled 和内容类型策略正确生效。
- 同一设备加入多个组时，本地复制共享 capture_id/sequence 但生成组隔离事件；UI 可按设备聚合，Import 通知不会自动桥接或重新发布。
- 不兼容、超限、被禁止或预取失败的槽位事件不会覆盖接收端原剪贴板。
- 用户在 A 上选择 B 的槽位后只有 A 的系统剪贴板改变，且该 Import 不更新 A 的发布槽位；显式“作为本机槽位发布”才产生新事件。
- 在 Import Committing、Adapter 写入和 suppression generation 记录之间注入崩溃，重启后不能把远端内容误发布为本机 capture。
- 模拟 Adapter 部分写入后失败，确认 suppression 不被删除、UI 报告可能变更，随后真实新复制仍能恢复正常发布。
- 大文件按需下载超过首次用户动作时进入 AwaitingConfirmation；没有第二次 ConfirmImport 时不得写入。
- Daemon 重启后恢复加密的最新槽位缓存；凭据存储不可用时只使用内存缓存。
- 两个本地进程传输单文件、多文件和嵌套目录。
- 多个顶层目录以及文件/目录混合输入按 top_level_id 独立提交，单个顶层失败不重做已 Committed 单元。
- 连接中断和双方 Daemon 重启后从 durable 块恢复。
- 在 CommitIntent、逐对象 Materializing、TreePrepared、目录 Rename、文件/目录刷新、FilesystemApplied 和 Committed 边界分别注入崩溃，恢复后文件树、tree_hash 与数据库状态一致。
- 在预检后、no-replace 提交前创建同名目标，确认进入 TargetChanged 且后来创建的文件/目录字节不变。
- 主窗口退出后 Daemon 继续镜像槽位和文件传输；重新打开选择器或使用 CLI 后仍可 Import。

### 跨平台测试

- Windows、macOS 和 Linux 两两互传及三设备混合组。
- 纯文本、HTML、RTF、PNG、URL 和文件列表的能力协商与降级。
- 同平台兼容私有格式透传；不兼容私有格式安全跳过。
- 不同文件名编码、保留字符、大小写和 Unicode 规范化差异。
- Windows 保留名、ADS、路径长度和文件/目录前缀冲突。
- macOS 剪贴板 changeCount、Windows Clipboard Sequence、X11 Selection 和 Wayland/Portal 可用性差异。
- 系统休眠、锁屏、用户会话切换、网络切换和防火墙场景。

### 安全测试

- 伪造 mDNS 广播不能冒充已配对设备。
- 被移出组或使用旧 membership_epoch 的设备不能取得新的剪贴板正文。
- 旧 epoch 事件可以用历史 Manifest 验签，但不能发送给后来加入或当前已移除的成员。
- 本地撤销表必须覆盖仍声明该设备为 active 的旧 GroupManifest。
- 篡改 GroupManifest、ClipboardSlotEvent 描述、正文哈希或来源签名必须失败。
- 网络连接、缓存完成回调和远端消息不能获得 AdapterWriteCapability 或调用系统剪贴板写入。
- 私有格式、文件剪贴板和敏感标记不能绕过本地策略。
- 恶意 Manifest 不能写出目标目录，临时对象名不能受远端路径控制。
- 超大消息、超多表示、超多文件、图片尺寸炸弹和畸形协议不会造成无界分配。
- 中间人导致验证码不一致时不能建立信任。
- 配对、控制消息和文件流无法使用 QUIC 0-RTT 重放。

## 26. 开发阶段

### 阶段一：可信连接与协议基础

- Cargo Workspace、protocol/core/platform/daemon/cli 基础结构。
- 长期设备身份、mDNS、QUIC、连接仲裁和 IPC Snapshot。
- 首次验证码配对和身份固定。
- 规范化编码与固定测试向量。

完成标准：两台设备能够发现、配对、稳定选择唯一 active connection，并识别身份变化。

### 阶段二：双设备文本槽位纵向闭环

- 两设备同步组、邀请和方向策略。
- 纯文本 Clipboard Adapter。
- ClipboardSlotEvent、全局 origin_sequence、签名和去重。
- 远端文本槽位缓存、CLI slots/use、Import 抑制、暂停和恢复。

完成标准：完成一次组授权后，两台设备复制纯文本会实时更新对方槽位，但不改变对方系统剪贴板；执行 `clipboard use` 后才导入所选内容。

### 阶段三：多设备与桌面体验

- 三台及以上设备的独立来源槽位与多组聚合。
- 离线每设备最新槽位 TTL 补发。
- Tauri Clipboard Switcher、全局快捷键、托盘暂停、组管理、预览和缓存状态。
- Daemon 自动启动、单实例和版本兼容。

完成标准：三设备同时复制、休眠和重连场景下，每个来源槽位独立更新；用户能快速选择任意设备，未选择时本机剪贴板不变。

### 阶段四：共享内容传输基础

- Blob、Manifest 和 Range Stream。
- 单文件、多文件、目录和混合顶层项模型。
- 对象式临时区、跨平台路径预检、哈希校验和缓存配额。
- 可供文件剪贴板与普通文件投送共同调用的 Transfer 接口。

完成标准：文件和目录能够安全传入事件独占缓存，只有完整校验后的 mapped top-level paths 才能交给 Clipboard Adapter。

### 阶段五：完整剪贴板类型

- HTML、RTF、图片、URL 和多表示选择。
- 文件/目录剪贴板及缓存生命周期。
- 同平台私有格式能力协商。
- 敏感来源抑制、加密短期缓存和本地策略。

完成标准：标准类型跨平台选择最佳表示；文件槽位选择后在内容完整落地时才能 Import；不兼容内容和后台预取不会破坏当前剪贴板。

### 阶段六：普通文件投送、恢复与发布加固

- 普通文件 Offer、接受、拒绝、取消和顶层冲突映射。
- CommitIntent、逐顶层原子提交、分块 durable 状态、范围恢复和双端重启。
- 传输历史、基础 UI、网络异常、磁盘故障、安装包和真实设备矩阵。
- 性能、隐私、安全审计和升级迁移测试。

完成标准：大文件中断并重启应用后继续传输；崩溃不会产生数据库与目标文件矛盾；Windows、macOS、Linux 达到 v1.0 验收标准。

### 阶段七：Android 前台实时客户端

- Tauri Android 工程、Rust Android target 和平台 capability 探测。
- 前台启动、暂停、进程被终止和恢复时的 Snapshot 重建。
- Android 系统剪贴板文本捕获、显式发布和远端文本 Import。
- 底部导航、全屏设备选择器、safe-area 与触控可访问性。
- 文本/URL Share Intent；图片和文件 URI grant 留到流式缓存能力完成后启用。
- 与桌面设备组成同步组的真实 Android 设备测试。

完成标准：Android 应用前台时可以加入已有同步组、实时看到桌面设备槽位、发布本机文本并显式 Import 远端文本；进入后台后不声称继续实时同步，重新前台时通过新 Snapshot 恢复且不自动写入系统剪贴板。

## 27. 验收标准

### v1.0 实时镜像与按需取用

- Windows、macOS 和 Linux 能运行，并完成两两连接及至少一个三平台混合同步组测试；平台剪贴板 API 不具备后台能力时必须明确降级并报告 capability。
- 未配对设备不能加入同步组；加入组必须由目标用户显式确认一次。
- Owner 与各成员完成直接配对后，普通成员之间无需重复比较验证码即可获得严格限定的 group-scoped 权限。
- 组内默认互相发布/订阅，也能按成员配置 send_only、receive_only 和 disabled。
- 设备唤醒且局域网空闲时，小于 64 KiB 的纯文本从本地变化被 Daemon 观察到至远端槽位 MetadataReady，端到端 p95 目标不超过 2 秒。
- 任何 ClipboardSlotOffer、预取完成、设备上线或重连都不能自动写入本机系统剪贴板。
- 用户可以从选择器、快捷键或 CLI 选择任意授权设备的最新槽位；只有 Create/Confirm Import 使 Daemon 创建有效 AdapterWriteCapability 后才允许写入。
- Clipboard Switcher 必须把“当前系统剪贴板”“本机最近发布槽位”和各远端设备槽位分开展示；远端实时更新不得抢焦点、改变当前选中事件或把列表滚回顶部。
- metadata_only、partial、ready、expired、blocked 和 protocol_conflict 必须有不同文字、图标及操作状态；不能只凭颜色或在线状态表达可用性。
- 按需内容完成下载后必须显示固定来源摘要并等待第二次确认；关闭并重新打开 UI 后仍能从同一 import_id 恢复 Fetching 或 AwaitingConfirmation。
- 同一设备跨组聚合时，不同 sequence 必须可展开辨认且不能静默混合；切换“按设备/按组”视图不得改变最终 SlotSelection 和授权结果。
- 纯文本、HTML、RTF、PNG、URL 和文件列表可以按能力镜像；Import 时选择最佳兼容组合，私有格式不兼容时跳过或退化到标准表示。
- 三台设备同时复制后，每台设备都能看到三个彼此独立的最新槽位，不存在全组胜出者。
- 设备离线后重新连接，只补发 TTL 内每个来源的最新槽位，不回放完整历史。
- expired 槽位可以显示为 stale，但不能新建 Import；在到期前创建的 Import 只可在有界 Lease 内完成。
- 被策略拒绝、超限、损坏、不兼容或下载失败的槽位不会清空或覆盖本机剪贴板。
- 文件槽位只有在用户选择且缓存内容完整校验后才写入本地路径，过期和未引用缓存能够安全清理。
- Import 事件不会更新本机发布槽位或自动跨组桥接；只有显式重新发布才会产生本机槽位事件。
- 移出组的新 membership_epoch 被在线成员应用后，解除配对或本地暂停的设备不再取得新的剪贴板正文；离线成员显示待确认状态。

### v1.0 文件投送与可靠性

- 身份公钥变化触发安全错误，而不是静默接受。
- 单文件、多文件和目录能够完整传输，哈希一致。
- 传输多 GiB 文件时进程内存不随文件大小线性增长。
- 网络中断和双方 Daemon 重启后可以从 durable 块恢复。
- 恶意相对路径不能写出用户选择的目标目录，合法名称不会因临时 `.part` 映射产生冲突。
- 预检后新出现的目标不能被提交覆盖；no-replace 失败必须保留目标并进入 TargetChanged。
- CommitIntent 任意崩溃点恢复后，数据库和目标文件状态一致。
- UI 主窗口关闭不会中断 Daemon 中的槽位镜像或文件传输。
- 默认设置不会自动接收普通文件投送；剪贴板镜像仅在用户显式加入同步组后启用，且不存在自动应用选项。

### Android 前台客户端

- Android 应用前台进入 ForegroundLive 后，能够与 Windows、macOS 或 Linux 桌面节点完成发现、配对、加入同步组和文本槽位互换。
- 应用切入后台后 UI/诊断不得声称仍在实时同步；系统终止进程不会损坏身份、组状态、ImportIntent 或已校验缓存。
- 从后台重新进入前台必须先获取完整 Snapshot，再应用增量事件；后台期间设备产生多个新槽位时只显示每设备最新合法槽位。
- 生命周期切换、重连和 Snapshot 完成不能读取或写入系统剪贴板；只有前台真实本机复制、明确“发布当前剪贴板”和显式 Import 能触发相应 Adapter。
- Android 剪贴板能力受系统限制时必须报告 capability limited，不得要求辅助功能、默认输入法、设备管理或永久前台服务作为基本功能前提。
- Android 使用底部导航、至少 44 CSS px 触控目标和 safe-area；不存在只能通过 hover、右键或桌面快捷键完成的核心操作。
- iOS 工程和依赖不属于当前交付物，文档与 UI 不显示未经实现的 iOS 支持。

## 28. 关键风险

### 跨平台剪贴板能力

Windows、macOS、X11 和 Wayland 的剪贴板生命周期、后台监听、延迟渲染和私有格式模型差异很大。尤其 Wayland 环境可能要求桌面 Portal、前台会话或特定 compositor 支持。产品必须按运行环境报告真实 capability，不能为了宣传“全格式”而伪装支持。

Android 普通应用只能在平台允许的前台上下文中可靠读取剪贴板，系统版本还可能显示访问提示、限制后台读取或清理敏感内容。用户授权不等于获得永久后台执行能力；产品必须把 ForegroundLive、Suspended 和 ClipboardCapabilityLimited 分开建模，不能通过高风险权限绕过限制。

### 实时镜像的隐私风险

实时镜像仍可能把密码、令牌、截图和文件缓存到其他设备，即使没有自动覆盖系统剪贴板。平台敏感标记并不可靠，因此显式组授权、发布/订阅策略、预取模式、短 TTL、暂停入口、私有格式默认关闭和最小化日志都是 v1.0 必须能力。

v1 中事件描述元数据对 signed audience 整体可见，成员级类型规则只保护正文。对元数据存在性也敏感的场景必须使用更严格的组边界，而不能误以为关闭某成员的正文权限会隐藏类型、大小和哈希。

### 多设备槽位一致性

每个来源槽位由单一设备写入，简化了不同设备之间的冲突，但 origin_sequence 持久化、组 revision、成员删除、转发和离线补发仍需严格处理。相同 sequence 不同内容必须隔离，旧事件不能覆盖新槽位，UI 聚合不能跨组扩大权限。

### 文件剪贴板缓存

跨设备粘贴文件要求接收端实际保存内容。缓存过早清理会导致粘贴失败，保存过久则产生隐私和磁盘占用问题，需要把当前剪贴板引用、TTL、配额和清理恢复作为一个整体设计。

### 配对与协议安全

不得凭设备名称或发现广播建立信任，也不得自行发明密码学算法。配对验证码、规范化签名对象、Rustls 自定义验证器和组成员撤销需要单独安全审查。

### 断点续传与提交一致性

源文件变化、块状态写入时机、目标 Rename 和 SQLite 状态容易产生不一致。durable 块顺序、CommitIntent 和启动协调恢复是实现中的高风险部分。

### 功能范围膨胀

实时剪贴板镜像、设备选择器和 Android 前台客户端已经显著扩大范围。公网中继、账号、云历史、iOS、Android 永久后台同步、文件夹持续同步、自动 Owner 选举、自动应用远端槽位和跨应用私有格式转换都必须继续留在范围外。

## 29. 最终范围

最终产品是一个 Rust Core 驱动的跨平台局域网设备协作工具。桌面 v1.0 覆盖 Windows、macOS 和 Linux；随后交付 Android 前台实时客户端，iOS 暂不考虑。共同能力包括设备发现、人工配对、身份固定、同步组、QUIC 加密直连、多设备剪贴板实时镜像、按设备选择与显式 Import、能力协商、短期离线补发、文件与目录投送、断点续传和共享 Tauri UI。

剪贴板镜像不是附加功能，而是核心体验：用户只在建立信任和加入同步组时确认一次，之后每台授权设备的最新剪贴板会实时出现在选择器中，但不会干扰本机当前剪贴板。用户选择某台设备后才取用内容；文件列表和兼容私有格式按显式策略启用。项目不承诺理解所有应用私有数据，而是完整枚举可读取表示、进行能力协商，并在不兼容时安全降级。

项目的差异化不只是“局域网传文件”，而是在无需账号和中心服务器的前提下，让多个可验证身份的设备形成一个可控、可恢复、尽量无感的本地协作空间。

Android 打开期间是完整的实时组成员；进入后台后允许系统挂起，重新前台再恢复最新状态。这一边界是轻量性、隐私和系统兼容性的主动选择，不以高风险权限换取虚假的永久在线承诺。
