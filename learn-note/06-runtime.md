# A6：Runtime 子系统学习笔记（运行时抽象、进程与守护、会话与调度）

一、职责与定位
- 定义并抽象执行环境能力：Shell、文件系统、长运行进程、内存上限；在不同平台（本机、Docker）间提供统一接口
  - 接口：/home/mi/work/open_source/zeroclaw/src/runtime/traits.rs:14–71（RuntimeAdapter）
- 驱动进程与守护：主进程 CLI 管理网关、Cron、守护进程；在守护中监督各组件并处理优雅退出
  - 入口与子命令：/home/mi/work/open_source/zeroclaw/src/main.rs（见下方索引）
  - 守护监督：/home/mi/work/open_source/zeroclaw/src/daemon/mod.rs:48–163，196–231
- 与上层模块的接口与配合：
  - Agent/Tools：根据 runtime.has_shell_access()/has_filesystem_access() 决定是否启用工具（例如 Shell 工具）
  - Gateway：Tokio 异步运行、路由与 TLS、WS/SSE、优雅退出；见 A7 与本笔记“执行链路”
  - Cron：调度执行 shell/agent 任务（带安全校验、超时与资源限制）
  - Observability：/metrics 与 SSE 广播事件；守护进程周期上报状态快照

二、架构总览（进程模型 / Tokio / 队列 / 缓存 / 持久化）
- 运行时抽象层（RuntimeAdapter + 工厂）
  - 工厂：/home/mi/work/open_source/zeroclaw/src/runtime/mod.rs:11–24（native/docker/cloudflare 占位）
  - NativeRuntime：具备完整能力；/home/mi/work/open_source/zeroclaw/src/runtime/native.rs:13–56
  - DockerRuntime：容器化隔离、读写与资源限制、工作目录挂载；/home/mi/work/open_source/zeroclaw/src/runtime/docker.rs:55–138
- 进程与守护
  - CLI 子命令启动/重启/状态探测；/home/mi/work/open_source/zeroclaw/src/main.rs（见索引）
  - 守护进程统一拉起与监督 gateway/cron/heartbeat 等；/home/mi/work/open_source/zeroclaw/src/daemon/mod.rs:48–163
- 会话与队列
  - 会话持久化：trait + SQLite 实现（WAL、FTS5、状态 running/idle/error、TTL 清理）
    - Trait：/home/mi/work/open_source/zeroclaw/src/channels/session_backend.rs:34–134
    - SQLite：/home/mi/work/open_source/zeroclaw/src/channels/session_sqlite.rs:24–106, 167–572
  - WS per-session 串行化队列：SessionActorQueue（在 Gateway AppState 中构造与使用）
    - 构造位置：/home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:850–851（new(8, 30, 600)）
- Cron 调度与执行
  - 调度器：轮询同步 + 存储 + 超时执行 + 输出脱敏 + 渠道投递
    - /home/mi/work/open_source/zeroclaw/src/cron/scheduler.rs:25–27, 29–… , 452–470, 472–…
  - 存储层：SQLite 持久化任务与运行记录、输出截断
    - /home/mi/work/open_source/zeroclaw/src/cron/store.rs:12–14, 22–48
- 缓存层（LLM 响应两级缓存）
  - 热缓存（内存）+ 温缓存（SQLite）+ TTL/LRU 淘汰
  - /home/mi/work/open_source/zeroclaw/src/memory/response_cache.rs:24–37, 39–86, 102–273

三、核心组件
- 运行时接口与实现
  - RuntimeAdapter：能力声明 + shell 命令构建；/src/runtime/traits.rs:14–71
  - NativeRuntime：sh -c / cmd /C；存储目录 ~/.zeroclaw；/src/runtime/native.rs:13–56
  - DockerRuntime：
    - workspace 路径校验与白名单；/src/runtime/docker.rs:17–52
    - docker run 参数：--rm/--init/--interactive、--network、--memory、--cpus、--read-only、-v/--workdir、image、sh -c；/src/runtime/docker.rs:86–138
    - has_filesystem_access 取决于 mount_workspace；supports_long_running=false；memory_budget 由 memory_limit_mb 推导；/src/runtime/docker.rs:64–84
- 进程入口与 CLI
  - Gateway 启动/重启与配对码运维：
    - Restart：/home/mi/work/open_source/zeroclaw/src/main.rs:1063–1160
    - shutdown_gateway：/home/mi/work/open_source/zeroclaw/src/main.rs:1881–1899
    - fetch_paircode：/home/mi/work/open_source/zeroclaw/src/main.rs:1901–1946
  - 守护进程入口：/home/mi/work/open_source/zeroclaw/src/main.rs:1163–1182（调用 daemon::run）
- 守护与优雅退出
  - 信号处理与主循环：/home/mi/work/open_source/zeroclaw/src/daemon/mod.rs:11–46, 48–163
  - 组件监督（指数退避）：/home/mi/work/open_source/zeroclaw/src/daemon/mod.rs:196–231
  - 周期状态写入：/home/mi/work/open_source/zeroclaw/src/daemon/mod.rs:173–194
- Gateway（运行时承载服务）
  - AppState 初始化（含 session_backend 与 SessionActorQueue）：/home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:780–879
  - 路由注册（REST/Webhook/WS/SSE/静态）：/home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:886–1016
  - TLS 接入循环 + 优雅退出（watch）：/home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1038–1111, 1102–1111
  - 健康与指标：/home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1120–1183
  - 管理端（仅本机）：/home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:2118–2215
- WebSocket 会话流
  - 令牌提取优先级：/home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:68–113
  - 握手与聊天处理：/home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:115–150
  - 事件转发与错误分类：/home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:374–531
- Cron 调度器与 CLI
  - 校验与添加/更新：/home/mi/work/open_source/zeroclaw/src/cron/mod.rs:25–46, 81–111
  - CLI 处理：/home/mi/work/open_source/zeroclaw/src/cron/mod.rs:155–398
- 两级响应缓存
  - 结构/构造/命中/写入/提升/统计：/home/mi/work/open_source/zeroclaw/src/memory/response_cache.rs:24–273

四、执行链路
- 启动（CLI → 守护/网关）
  - CLI 解析子命令→按需启动 gateway/daemon/cron
  - 守护进程 run：spawn gateway/channels/heartbeat/scheduler supervisors → 监听 SIGINT/SIGTERM → watch 触发优雅退出
  - 参考：
    - /home/mi/work/open_source/zeroclaw/src/main.rs:1163–1182（daemon 入口）
    - /home/mi/work/open_source/zeroclaw/src/daemon/mod.rs:48–163（主循环）
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1038–1111, 1102–1111（优雅退出）
- Cron 任务执行流
  - 周期轮询（最小 5s）→ 同步/恢复 → 任务选择 → 安全校验与命令构建（runtime.build_shell_command）→ 超时与资源限制 → 输出脱敏与截断 → 渠道投递 → 记录 run
  - 参考：
    - /home/mi/work/open_source/zeroclaw/src/cron/scheduler.rs:25–27, 29–…（run 主循环）
    - /home/mi/work/open_source/zeroclaw/src/cron/scheduler.rs:452–470（scan_and_redact_output）
    - /home/mi/work/open_source/zeroclaw/src/cron/scheduler.rs:472–…（deliver_announcement*）
    - /home/mi/work/open_source/zeroclaw/src/cron/store.rs:12–14, 22–48（存储与截断）
- WS 会话 turn 串行化流
  - WS 握手鉴权（Bearer/子协议/查询参数）→ 获取 session_key → 提交到 SessionActorQueue（串行执行）→ Agent turn stream → 事件（thinking/tool_call/tool_result/chunk/done）经 WS/SSE 转发 → 会话状态更新
  - 参考：
    - /home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:68–113, 115–150, 374–531
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:850–851（SessionActorQueue 构造）
    - /home/mi/work/open_source/zeroclaw/src/channels/session_backend.rs:34–134（接口）
    - /home/mi/work/open_source/zeroclaw/src/channels/session_sqlite.rs:382–430, 432–572（状态与查询）

五、配置与默认值
- Runtime 配置（[runtime]）
  - RuntimeConfig：/home/mi/work/open_source/zeroclaw/src/config/schema.rs:5397–5417, 5485–5494
    - kind: "native" | "docker"（默认 "native"）
    - docker: DockerRuntimeConfig（见下）
    - reasoning_enabled/reasoning_effort（提供者统一推理开关/力度）
  - DockerRuntimeConfig：/home/mi/work/open_source/zeroclaw/src/config/schema.rs:5419–5483
    - image（默认 alpine:3.20）、network（默认 none）
    - memory_limit_mb（默认 512）、cpu_limit（默认 1.0）
    - read_only_rootfs（默认 true）
    - mount_workspace（默认 true）
    - allowed_workspace_roots（挂载工作区可信根目录白名单）
- Gateway 与运行约束（选摘）
  - 请求体限制/超时（默认 64KB/30s，可通过环境变量覆盖）；/home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:2268–2279（测试断言）
  - 管理端仅 loopback；/home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:2125–2215
- Cron
  - 轮询周期最小值与超时常量；/home/mi/work/open_source/zeroclaw/src/cron/scheduler.rs:25–27

六、常见失败模式与恢复
- 运行时选择错误
  - runtime.kind 空/未知/未实现（cloudflare）：/home/mi/work/open_source/zeroclaw/src/runtime/mod.rs:16–23（错误信息）
  - 处理：校正配置为 native/docker
- Docker 工作区挂载校验失败
  - 路径非绝对、为根“/”、不在 allowed_workspace_roots：/home/mi/work/open_source/zeroclaw/src/runtime/docker.rs:17–52
  - 处理：修正工作区路径与白名单
- 资源限制与超时
  - Cron shell 超时（SHELL_JOB_TIMEOUT_SECS）、内存/CPU 限制未生效（未配置时不加 flag）；/home/mi/work/open_source/zeroclaw/src/cron/scheduler.rs:25–27，/home/mi/work/open_source/zeroclaw/src/runtime/docker.rs:103–109, 259–274（测试）
  - 处理：在配置中显式设置 memory_limit_mb/cpu_limit
- 会话卡死或并发冲突
  - 通过 SessionActorQueue 串行化；持久层 list_stuck_sessions 查询；/home/mi/work/open_source/zeroclaw/src/channels/session_sqlite.rs:432–572
  - 处理：监控 stuck 列表、重置会话状态
- 网关重启与优雅退出
  - restart 先发 /admin/shutdown 再启动；TLS/Plain 路径均监听 shutdown；/home/mi/work/open_source/zeroclaw/src/main.rs:1063–1160，/home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1038–1111, 1102–1111
  - 处理：使用 CLI restart，确保端口释放（最多 5s）

七、代码索引（按模块与关键行）
- Runtime 抽象与实现
  - /home/mi/work/open_source/zeroclaw/src/runtime/traits.rs:14–71（RuntimeAdapter）
  - /home/mi/work/open_source/zeroclaw/src/runtime/mod.rs:11–24（create_runtime 工厂）
  - /home/mi/work/open_source/zeroclaw/src/runtime/native.rs:13–56（NativeRuntime）
  - /home/mi/work/open_source/zeroclaw/src/runtime/docker.rs:17–52（workspace 挂载校验），55–138（docker run 命令构建）
- 配置 Schema（Runtime/Docker）
  - /home/mi/work/open_source/zeroclaw/src/config/schema.rs:5397–5494（RuntimeConfig 与默认），5419–5483（DockerRuntimeConfig 与默认）
- CLI 与运维
  - /home/mi/work/open_source/zeroclaw/src/main.rs:1063–1160（gateway restart）
  - /home/mi/work/open_source/zeroclaw/src/main.rs:1881–1899（shutdown_gateway）
  - /home/mi/work/open_source/zeroclaw/src/main.rs:1901–1946（fetch_paircode）
  - /home/mi/work/open_source/zeroclaw/src/main.rs:1163–1182（daemon 入口）
- 守护进程
  - /home/mi/work/open_source/zeroclaw/src/daemon/mod.rs:11–46（信号），48–163（run 主体），173–194（状态写入），196–231（组件监督）
- Gateway（承载）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:780–879（AppState 初始化）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:886–1016（路由）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1038–1111（TLS 接入与退出），1102–1111（Plain 优雅退出）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1120–1183（健康/指标）
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:2118–2215（Admin 路由与本机限制）
- WebSocket 会话
  - /home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:68–113（令牌提取），115–150（握手/处理），374–531（事件与错误）
- 会话存储
  - /home/mi/work/open_source/zeroclaw/src/channels/session_backend.rs:34–134（Trait + SessionState）
   - /home/mi/work/open_source/zeroclaw/src/channels/session_sqlite.rs:24–106（初始化/迁移），167–430（读写/状态），432–572（查询/检索）
- Cron 调度
  - /home/mi/work/open_source/zeroclaw/src/cron/mod.rs:25–46, 81–111（校验与添加/更新），155–398（CLI）
  - /home/mi/work/open_source/zeroclaw/src/cron/scheduler.rs:25–27（常量），29–…（run 循环），452–470（脱敏），472–…（投递）
  - /home/mi/work/open_source/zeroclaw/src/cron/store.rs:12–14（截断标记），22–48（新增/持久化）
- 两级缓存
  - /home/mi/work/open_source/zeroclaw/src/memory/response_cache.rs:24–273（结构/构造/命中/写入/提升/统计）

附：关键片段
- 运行时工厂（配置选择）
  - /home/mi/work/open_source/zeroclaw/src/runtime/mod.rs:11–24
  - 行为：根据 RuntimeConfig.kind 选择 NativeRuntime 或 DockerRuntime，非法配置返回错误
- RuntimeAdapter 能力门控
  - /home/mi/work/open_source/zeroclaw/src/runtime/traits.rs:21–45
  - 行为：显式声明 has_shell_access/has_filesystem_access/supports_long_running/memory_budget 等，供上层据此启用/禁用工具与服务
- Docker 命令构建与隔离参数
  - /home/mi/work/open_source/zeroclaw/src/runtime/docker.rs:86–138
  - 要点：--network、--memory、--cpus、--read-only、-v 工作区挂载、--workdir、image + sh -c
- 守护监督与优雅退出
  - /home/mi/work/open_source/zeroclaw/src/daemon/mod.rs:48–163, 196–231
  - 要点：组件拉起 + 指数退避 + 信号监听 + 退出时 abort/await
- WS 串行化与会话状态
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:850–851（队列构造）
  - /home/mi/work/open_source/zeroclaw/src/gateway/ws.rs:374–531（事件与错误分类）
  - /home/mi/work/open_source/zeroclaw/src/channels/session_sqlite.rs:382–430, 432–572（状态持久化与检索）
- Cron 超时与输出脱敏
  - /home/mi/work/open_source/zeroclaw/src/cron/scheduler.rs:25–27（SHELL_JOB_TIMEOUT_SECS），452–470（scan_and_redact_output），472–…（deliver_announcement）

本笔记覆盖运行时抽象与实现、进程与守护、会话与队列、Cron 调度与执行、两级缓存、与 Gateway/WS/SSE/Observability 的接口，以及配置、执行链路、常见失败模式与代码索引。