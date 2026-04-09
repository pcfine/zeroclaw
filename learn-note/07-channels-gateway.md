# A7：Channels & Gateway 学习笔记（ZeroClaw 网关）

一、职责与定位
- 网关（gateway）是 ZeroClaw 的外部接口层，统一承载：
  - REST API（管理与运营接口）、Webhook（渠道/第三方回调）、WebSocket（实时双向聊天）、SSE（事件广播）
  - 配额与鉴权（配对令牌、可选 Webhook 密钥、速率限制、幂等）
  - TLS/mTLS 终止与证书钉扎（pinning）
  - 会话持久化（WS 会话历史）、设备管理（配对设备注册/撤销/轮换）、观测与指标
- 核心设计目标：可运维、安全、可扩展。通过通道适配器（channels）把外部平台消息归一为内部聊天消息，并调用 Agent 处理，回执至原通道或 API。

二、架构总览（进程模型 / 重启策略 / 优雅退出）
- 进程模型
  - 主入口 run_gateway 负责：
    - 初始化 AppState（Provider/Model/Memory/配额器/鉴权器/幂等存储/工具注册表/事件广播/节点注册/会话后端/设备注册/待配对存储/画布存储/路径前缀等）
    - 构建 axum Router，挂载全部路由（REST、Webhook、WS、SSE、静态单页应用）
    - TLS/mTLS：按配置构建 rustls TlsAcceptor，启用手动 TLS 接入循环（hyper_util::server::conn::auto 驱动每连接）
    - 无 TLS 场景：直接 axum::serve + with_graceful_shutdown
  - 关键代码：
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:886–1016（路由定义）
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1038–1111（TLS 接入循环与优雅退出）
- 重启策略（CLI）
  - zeroclaw gateway restart 会先调用 /admin/shutdown 触发优雅退出，轮询端口释放后再启动
  - 关键代码：
    - /home/mi/work/open_source/zeroclaw/src/main.rs:1063–1100（restart 分支）
    - /home/mi/work/open_source/zeroclaw/src/main.rs:1881–1885（shutdown_gateway 发起 admin/shutdown）
- 优雅退出
  - 普通 TCP：with_graceful_shutdown 监听 watch 信号
  - TLS：select! 同时监听 listener.accept 与 shutdown_rx.changed，收到信号后 break 退出
  - 关键代码：
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1102–1111（plain TCP 优雅退出）
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1056–1099（TLS 循环中监听 shutdown）

三、路由与接口总览
- 管理端（仅本机）：/admin/shutdown, /admin/paircode, /admin/paircode/new
- 运行健康与指标：/health（公开但不泄露机密）、/metrics（Prometheus 文本暴露，未启用则给出提示文本）
- 配对：
  - /pair（提交一次性 code → bearer token）
  - /pair/code（公开，仅初始未配对阶段返回 code，用于容器/远程可视化）
- Webhook：
  - /webhook（通用 webhook，配对+可选 X-Webhook-Secret；幂等键）
  - /whatsapp（GET 验证、POST 消息）、/linq（POST）、/wati（GET 验证、POST）、/nextcloud-talk（POST）、/webhook/gmail（POST）
- Web Dashboard API（选摘）：/api/status, /api/config(GET/PUT), /api/tools, /api/cron(*), /api/integrations(*), /api/doctor, /api/memory(*), /api/cost, /api/cli-tools, /api/health, /api/sessions(*), /api/canvas(*)
- 配对与设备管理 API：/api/pairing/initiate, /api/pair（增强）, /api/devices, /api/devices/{id}, /api/devices/{id}/token/rotate
- 实时通道：
  - SSE 事件：/api/events
  - WS：/ws/chat（聊天）、/ws/canvas/{id}（画布）、/ws/nodes（节点发现）
  - 静态资源与 SPA fallback：/_app/{*path}, fallback
- 关键代码：
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:886–1016

四、鉴权、配额与幂等
- 配对令牌（PairingGuard）
  - /pair：速率限制 + 授权尝试速率限制（暴力破解防护），校验 X-Pairing-Code，成功后持久化 token 至 config.toml；失败/锁定返回相应错误与重试秒数
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1186–1259（handle_pair）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1261–1275（persist_pairing_tokens）
- Bearer 校验与 Webhook 密钥
  - 通用 /webhook：若 require_pairing，则需 Authorization: Bearer；可选 X-Webhook-Secret（常量时比对），并支持 X-Idempotency-Key 去重
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1326–1524（handle_webhook）
- WS 鉴权（浏览器兼容）
  - 令牌提取优先级：Authorization > Sec-WebSocket-Protocol: bearer.<token> > ?token=
  - /home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:68–113（extract_ws_token）
  - /home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:115–150（handle_ws_chat）
- SSE 鉴权
  - 若 require_pairing，则需 Bearer，否则 401
  - /home/mi/work/open_source/zeroclaw/src/gateway/sse.rs:18–58
- 速率限制与幂等
  - 每客户端键（IP/转发头，受 trust_forwarded_headers 控制）的 /pair、/webhook 限流
  - 授权失败尝试限流（auth_limiter）
  - /webhook 幂等键（X-Idempotency-Key）防重复处理
  - 关键实现见：
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1398–1414（幂等键）
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1192–1211（配对限流+auth 限流）

五、TLS / mTLS 与证书钉扎
- 服务器证书与私钥由配置加载，rustls 构建 ServerConfig；可选 mTLS 客户端证书校验（可选强制）
- 证书钉扎：对客户端证书计算 SHA-256 指纹，仅允许配置指纹列表
- 关键代码：
  - /home/mi/work/open_source/zeroclaw/src/gateway/tls.rs:17–21（build_tls_acceptor）
  - /home/mi/work/open_source/zeroclaw/src/gateway/tls.rs:23–53（build_server_config）
  - /home/mi/work/open_source/zeroclaw/src/gateway/tls.rs:55–91（build_client_verifier）
  - /home/mi/work/open_source/zeroclaw/src/gateway/tls.rs:93–99（cert_sha256_fingerprint）

六、通道适配器与 Webhook 集成
- WhatsApp
  - GET /whatsapp：Meta webhook 验证（hub.challenge），常量时比较 verify_token
  - POST /whatsapp：X-Hub-Signature-256（HMAC-SHA256）签名校验（可选 app_secret），解析消息、调用 Agent 回复
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1537–1561（GET 验证）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1564–1584（verify_whatsapp_signature）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1591–1695（消息处理）
- Linq（iMessage/RCS/SMS）
  - POST /linq：X-Webhook-Timestamp + X-Webhook-Signature，HMAC-SHA256 校验 "{timestamp}.{body}"，拒绝>300s陈旧时间戳
  - /home/mi/work/open_source/zeroclaw/src/channels/linq.rs:414–433, 444–448（签名校验）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1698–1815（消息处理）
- WATI（WhatsApp 服务商）
  - GET /wati：验证挑战回显
  - POST /wati：解析消息；如为 audio/voice，尝试转写后构建消息；调用 Agent 回复
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1817–1930（验证与消息处理）
- Nextcloud Talk
  - POST /nextcloud-talk：X-Nextcloud-Talk-Random + X-Nextcloud-Talk-Signature 校验（HMAC），解析消息并回复
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1932–2044
- Gmail Push
  - POST /webhook/gmail：Bearer <secret> 简单鉴权；限制请求体 ≤1MB；异步处理 Pub/Sub 推送
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:2050–2112

七、WebSocket 聊天与事件流
- 握手与协议
  - /ws/chat 支持子协议 “zeroclaw.v1”，回显 Sec-WebSocket-Protocol；支持 connect 握手帧（可携带 session_id、device_name、capabilities），兼容旧客户端直接 message
  - 鉴权见第四节（多路径令牌）
  - /home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:115–150（handle_ws_chat）
  - /home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:230–303（connect 握手/回退）
- 会话与持久化
  - session_id 可由客户端提供或服务器生成，带前缀 gw_；持久化至 SQLite（可在配置中关闭）；会话名可存取；状态（running/idle/error）更新
  - 并发串行化：每 session_key 通过 SessionActorQueue 获取锁，避免并发 turn 冲突
  - /home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:155–229, 350–369（会话与持久化）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:671–679, 850–851（session_backend 初始化与队列构造）
- 事件流（TurnEvent）
  - Agent::turn_streamed 产生 chunk、thinking、tool_call、tool_result；服务端转发为 WS 文本帧
  - 结束时发送 chunk_reset 与 done(full_response)
  - 同步 SSE 广播 agent_start/agent_end/error
  - /home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:374–531

八、配对与设备管理 API
- 设备注册表（SQLite 缓存+数据库，启动预热）
  - 字段：id/name/device_type/paired_at/last_seen/ip_address；以 token_hash（SHA-256）为键
  - 能力：register/list/revoke/update_last_seen/device_count
  - /home/mi/work/open_source/zeroclaw/src/gateway/api_pairing.rs:16–187
- 配对流程（增强）
  - /api/pairing/initiate：生成新的 pairing code（需已配对设备 Bearer 访问）
  - /api/pair：提交 code；成功返回 token，并将 token_hash 与设备信息注册（从 X-Forwarded-For 获取 client_id）
  - /api/devices：列出设备；/api/devices/{id}：撤销；/api/devices/{id}/token/rotate：为重新配对生成 code
  - /home/mi/work/open_source/zeroclaw/src/gateway/api_pairing.rs:237–383

九、观察性与指标
- SSE 广播
  - /api/events：将内部 broadcast channel 包装为 SSE；需要配对 Bearer
  - /home/mi/work/open_source/zeroclaw/src/gateway/sse.rs:18–58
- 事件上报（广播装饰器）
  - 将 inner observer 事件以统一 JSON 广播：llm_request/tool_call/tool_call_start/error/agent_start/agent_end（含 timestamp）
  - /home/mi/work/open_source/zeroclaw/src/gateway/sse.rs:60–162
- Prometheus 指标
  - /metrics：若启用 Prometheus observer，encode 文本；否则输出启用提示
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1160–1183

十、配置与默认值
- GatewayConfig 关键字段（默认见实现）
  - port/host/require_pairing/allow_public_bind/paired_tokens
  - pair_rate_limit_per_minute/webhook_rate_limit_per_minute
  - trust_forwarded_headers/path_prefix
  - rate_limit_max_keys/idempotency_ttl_secs/idempotency_max_keys
  - session_persistence/session_ttl_hours
  - pairing_dashboard（code_length/code_ttl/max_pending/max_failed/lockout_secs）
  - tls（enabled/cert_path/key_path/client_auth）
  - /home/mi/work/open_source/zeroclaw/src/config/schema.rs:2007–2075, 2136–2182, 2184–2197, 2199–2213
- 请求超时与体积限制
  - 默认 MAX_BODY_SIZE=65536，REQUEST_TIMEOUT_SECS=30（测试断言）
  - 支持通过 ZEROCLAW_GATEWAY_TIMEOUT_SECS 环境变量覆盖（测试覆盖默认回退）
  - 参考：
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:2268–2279（测试断言）

十一、错误处理与失败模式
- 常见返回
  - 401 Unauthorized：未配对/无效 Bearer、Webhook 密钥缺失或错误、Gmail Push secret 不匹配
  - 403 Forbidden：WhatsApp 验证失败（token mismatch）
  - 400 Bad Request：JSON 解析失败、Gmail envelope 无效、缺少必填字段
  - 429 Too Many Requests：配对/鉴权尝试限流触发
  - 413 Payload Too Large：Gmail Push 超体积
  - 200 OK（幂等重复）：/webhook idempotent duplicate
- 关键路径
  - /webhook：LLM 调用异常进行 provider 错误信息清洗（sanitize），上报 error 事件与时延指标
  - WS：JSON 格式错误/未知类型/空内容分别返回结构化错误帧；turn 失败区分 AUTH/PROVIDER/AGENT 错误码，session state 标记 error
  - TLS：握手失败记录 debug 日志；不中断主循环
- 参考：
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1486–1521（webhook LLM 错误清洗与上报）
  - /home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:502–529（WS 错误分类与广播）

十二、安全考量
- 缺省安全与显式开放
  - 默认 require_pairing = true；allow_public_bind = false；trust_forwarded_headers = false
  - 路径前缀 path_prefix 支持反代；nest 后添加 "/prefix/" 重定向保证一致性
- 管理端限制
  - /admin/* 仅允许 loopback 访问（require_localhost）；用于 CLI 控制
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:2125–2136, 2139–2155
- Webhook Secrets 与常量时比较
  - X-Webhook-Secret 与 WhatsApp verify_token 采用常量时比较，防计时侧信道
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1369–1383（Secret 比对）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1546–1551（WhatsApp token 比对）
- 签名验证
  - Linq/Nextcloud Talk/HMAC 校验；WhatsApp X-Hub-Signature-256 校验；Gmail Push Bearer secret 校验；时间戳陈旧拒绝（Linq）
- TLS/mTLS 与证书钉扎
  - 客户端证书验证可选强制；指纹钉扎进一步收敛信任
- 资源防护
  - 请求体全局 64KB 限制，Gmail Push 1MB 限制；请求超时默认 30s；每键限流/鉴权限流/幂等去重

十三、运维与重启
- CLI
  - zeroclaw gateway start/restart/get-paircode
  - restart：优雅关闭→等待端口释放（最久 5s）→重启
  - get-paircode：从运行中网关获取当前/新配对码（用于无终端环境）
  - /home/mi/work/open_source/zeroclaw/src/main.rs:1063–1160, 1903–1918
- Admin
  - /admin/shutdown：watch 通知网关优雅退出；/admin/paircode 与 /admin/paircode/new：便于运维面板展示与刷新
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:2118–2215

十四、关键代码引用（精选）
- 路由注册
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:886–1016
- TLS/mTLS 与优雅退出
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1038–1111
  - /home/mi/work/open_source/zeroclaw/src/gateway/tls.rs:23–53, 55–91, 93–99
- 健康与指标
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1120–1129（/health）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1160–1183（/metrics）
- 配对与令牌持久化
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1186–1275
- 通用 Webhook（鉴权/密钥/幂等/观测）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1326–1524
- WhatsApp
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1537–1561（GET 验证）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1564–1584（HMAC 校验）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1591–1695（消息处理）
- Linq
  - /home/mi/work/open_source/zeroclaw/src/channels/linq.rs:414–433, 444–448（签名校验）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1698–1815（消息处理）
- WATI
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1817–1930
- Nextcloud Talk
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1932–2044
- Gmail Push
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:2050–2112
- SSE 事件与广播装饰器
  - /home/mi/work/open_source/zeroclaw/src/gateway/sse.rs:18–58, 60–162
- WebSocket（鉴权/会话/事件）
  - /home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:68–113, 115–150, 374–531
- 配对与设备管理
  - /home/mi/work/open_source/zeroclaw/src/gateway/api_pairing.rs:16–187, 237–383
- 配置 Schema（网关/配对面板/TLS）
  - /home/mi/work/open_source/zeroclaw/src/config/schema.rs:2007–2075, 2136–2182, 2184–2197, 2199–2213
- 管理端（本机限制、关机、配对码）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:2125–2215
- CLI 运维（重启/获取配对码）
  - /home/mi/work/open_source/zeroclaw/src/main.rs:1063–1160, 1881–1885, 1903–1918

附：精选片段
- WS 令牌提取优先级
  - /home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:68–77
    - “1. Authorization: Bearer 2. Sec-WebSocket-Protocol: bearer.<token> 3. ?token=”
- Webhook 幂等
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1398–1414
    - 读取 X-Idempotency-Key，已存在则直接返回 status=duplicate
- 管理端本机限制
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:2125–2136
    - 非 loopback 返回 403，避免远程滥用关机与配对码
- TLS 手动接入循环（摘）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1069–1091
    - 接收 TLS、service_fn 包装、hyper_util::server::conn::auto::Builder::serve_connection

本笔记覆盖网关职责与接口、TLS/mTLS、通道 webhook 集成、WS/SSE 实时路径、配对与设备注册、鉴权与幂等、速率限制、观测与指标、运维重启，以及安全与失败模式要点。