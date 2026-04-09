# 09 — 可观测性 / 健康 / Doctor 笔记（ZeroClaw）

本节记录 ZeroClaw 在可观测性（Observability）、健康（Health）与诊断（Doctor）方面的职责、架构、关键类型与事件、指标与日志模式、诊断流程、常见失败模式与恢复手册，以及与安全与网关的集成点。文中附带关键定义与函数的源码定位（绝对路径:行号）。

---

## 1. 职责与目标

- 可观测性
  - 统一事件与指标采集接口（Observer trait）；
  - 多后端支持：日志、Verbose 控制台进度、Prometheus、OpenTelemetry（OTLP）；可多路广播；
  - 运行时追踪（runtime_trace）：持久化 JSONL 审计（工具调用、模型响应等）以便排障与事后分析。

- 健康
  - 进程内健康注册表：组件状态、最近 OK/错误时间、重启次数、运行时快照；
  - 对外暴露健康检查接口（/health 与 /api/health）。

- Doctor 诊断
  - 一键体检：配置语义检查、工作区就绪、守护/调度器/通道就绪性、环境、CLI 工具；
  - 追踪排障：查询 runtime_trace 最近事件、按 ID 检索特定事件；
  - 模型连通性探测：按 Provider 刷新模型目录，分类错误（鉴权/配额/限速等）。

- 网关与安全集成
  - 健康与指标 HTTP 接口；
  - 事件 SSE 实时广播，受配对/鉴权保护；
  - /pair 配对与速率限制（防爆破）。

---

## 2. 架构总览

- 抽象与工厂
  - Observer 抽象定义事件与指标接口，支持 flush、name、as_any 下转型
    - /home/mi/work/open_source/zeroclaw/src/observability/traits.rs:147–188
  - create_observer 基于配置选择后端，未启用的 feature 自动回退到 Noop
    - /home/mi/work/open_source/zeroclaw/src/observability/mod.rs:28–83

- 后端实现
  - NoopObserver：零开销占位
    - /home/mi/work/open_source/zeroclaw/src/observability/noop.rs:7–21
  - LogObserver：基于 tracing 输出结构化日志
    - /home/mi/work/open_source/zeroclaw/src/observability/log.rs:14–176
  - VerboseObserver：交互式 CLI 进度提示（不泄露内容）
    - /home/mi/work/open_source/zeroclaw/src/observability/verbose.rs:16–64
  - PrometheusObserver：内置注册表与编码，/metrics 文本暴露
    - /home/mi/work/open_source/zeroclaw/src/observability/prometheus.rs:1–304, 307–514
  - OtelObserver：OTLP HTTP（/v1/traces, /v1/metrics）导出
    - /home/mi/work/open_source/zeroclaw/src/observability/otel.rs:11–33, 35–194, 197–501
  - MultiObserver：扇出到多个后端
    - /home/mi/work/open_source/zeroclaw/src/observability/multi.rs:4–41

- 运行时追踪（Runtime Trace）
  - JSONL 结构化事件文件；三种存储策略：none/rolling/full；rolling 只保留最近 N 条；
  - 安全：0600 权限写入与重写；原子 rename 替换；
  - /home/mi/work/open_source/zeroclaw/src/observability/runtime_trace.rs:12, 15–30, 33–52, 61–141, 144–193, 195–230, 232–314

- 健康注册表（Health）
  - 组件状态、时间戳、重启次数；进程级快照与 JSON 输出；
  - /home/mi/work/open_source/zeroclaw/src/health/mod.rs:8–23, 25–37, 39–83, 85–103

- 网关集成（Gateway）
  - 公开健康检查：GET /health（无鉴权）
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1120–1129
  - Prometheus 指标：GET /metrics（按 feature 与后端启用情况输出/提示）
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1131–1183
  - 鉴权健康：GET /api/health（需 Bearer）
    - /home/mi/work/open_source/zeroclaw/src/gateway/api.rs:777–788
  - 诊断 API：GET/POST /api/doctor（需 Bearer）
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:928–930, /home/mi/work/open_source/zeroclaw/src/gateway/api.rs:594–606

- SSE 广播
  - 事件流：GET /api/events（按配对状态鉴权）
    - /home/mi/work/open_source/zeroclaw/src/gateway/sse.rs:18–57
  - BroadcastObserver 包装内部 Observer，转发精选事件到 SSE（不包含敏感内容）
    - /home/mi/work/open_source/zeroclaw/src/gateway/sse.rs:60–161
  - 网关启动时包装 Observer 并标记健康
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:783–800, 1003–1005

- 配置（ObservabilityConfig）
  - backend, otel_endpoint, otel_service_name, runtime_trace_{mode,path,max_entries}
  - /home/mi/work/open_source/zeroclaw/src/config/schema.rs:5094–5147

---

## 3. 关键类型与 Hook 点

- ObserverEvent 事件模型
  - 定义：/home/mi/work/open_source/zeroclaw/src/observability/traits.rs:10–116
  - 事件包括：AgentStart/End、LlmRequest/Response、ToolCallStart/ToolCall、TurnComplete、ChannelMessage、HeartbeatTick、CacheHit/Miss、Error、HandStarted/Completed/Failed、DeploymentStarted/Completed/Failed、RecoveryCompleted
  - 设计原则：足够诊断信息而避免泄露提示/响应正文

- ObserverMetric 指标模型
  - 定义：/home/mi/work/open_source/zeroclaw/src/observability/traits.rs:123–145
  - 包含：RequestLatency、TokensUsed、ActiveSessions、QueueDepth、HandRunDuration、HandFindingsCount、HandSuccessRate、DeploymentLeadTime、RecoveryTime

- Observer 抽象接口
  - record_event, record_metric, flush, name, as_any
  - 定义：/home/mi/work/open_source/zeroclaw/src/observability/traits.rs:156–188

---

## 4. 指标与日志/追踪模式

- Prometheus 指标命名（部分）
  - 计数器：zeroclaw_agent_starts_total、zeroclaw_llm_requests_total{provider,model,success}、zeroclaw_tool_calls_total{tool,success}、zeroclaw_channel_messages_total{channel,direction}、zeroclaw_errors_total{component}、zeroclaw_cache_hits_total{cache_type}、zeroclaw_cache_misses_total{cache_type}、zeroclaw_cache_tokens_saved_total{cache_type}、zeroclaw_hand_runs_total{hand,success}、zeroclaw_hand_findings_total{hand}、zeroclaw_deployments_total{status}
  - 直方图：zeroclaw_agent_duration_seconds{provider,model}、zeroclaw_tool_duration_seconds{tool}、zeroclaw_request_latency_seconds、zeroclaw_hand_duration_seconds{hand}、zeroclaw_deployment_lead_time_seconds、zeroclaw_recovery_time_seconds
  - 仪表：zeroclaw_tokens_used_last、zeroclaw_active_sessions、zeroclaw_queue_depth、zeroclaw_deployment_failure_rate、zeroclaw_mttr_seconds
  - 实现：/home/mi/work/open_source/zeroclaw/src/observability/prometheus.rs:6–295, 307–505

- OpenTelemetry 仪表与 Span
  - 仪表：
    - 计数器：zeroclaw.agent.starts、zeroclaw.llm.calls、zeroclaw.tool.calls、zeroclaw.channel.messages、zeroclaw.heartbeat.ticks、zeroclaw.errors、zeroclaw.tokens.used、zeroclaw.hand.runs、zeroclaw.hand.findings
    - 直方图：zeroclaw.agent.duration(s)、zeroclaw.llm.duration(s)、zeroclaw.request.latency(s)、zeroclaw.hand.duration(s)
    - 仪表（Gauge）：zeroclaw.sessions.active、zeroclaw.queue.depth
  - Span：
    - llm.call、agent.invocation、tool.call、error、hand.run；按持续时间回填 start_time，并设置 Status
  - 实现：/home/mi/work/open_source/zeroclaw/src/observability/otel.rs:11–33, 35–194, 197–440, 442–501

- 日志（LogObserver）
  - 通过 tracing::info 输出事件与指标，带关键字段与毫秒时长
  - /home/mi/work/open_source/zeroclaw/src/observability/log.rs:14–176

- 运行时追踪（Runtime Trace）JSONL 模式
  - 事件结构：id, timestamp, event_type, channel/provider/model/turn_id, success, message, payload
  - 定义：/home/mi/work/open_source/zeroclaw/src/observability/runtime_trace.rs:33–52
  - 写入策略：
    - append：创建父目录、0600 权限、逐行 JSON、fsync；若 rolling 则裁剪
      - /home/mi/work/open_source/zeroclaw/src/observability/runtime_trace.rs:71–107, 102–104
    - trim_to_last_entries：读取-过滤-写 tmp 文件（0600）-rename 原子替换
      - /home/mi/work/open_source/zeroclaw/src/observability/runtime_trace.rs:109–141
  - 加载/查询：
    - load_events(path, limit, event_filter, contains) 返回按时间逆序（新到旧）
      - /home/mi/work/open_source/zeroclaw/src/observability/runtime_trace.rs:232–292
    - find_event_by_id(path, id) 自底向上扫描命中即返
      - /home/mi/work/open_source/zeroclaw/src/observability/runtime_trace.rs:294–314

---

## 5. 健康（Health）

- 结构
  - ComponentHealth：status, updated_at, last_ok, last_error, restart_count
    - /home/mi/work/open_source/zeroclaw/src/health/mod.rs:8–15
  - HealthSnapshot：pid, updated_at, uptime_seconds, components
    - /home/mi/work/open_source/zeroclaw/src/health/mod.rs:17–23

- API
  - 标记与计数
    - mark_component_ok(component) 设置 ok 与 last_ok
      - /home/mi/work/open_source/zeroclaw/src/health/mod.rs:62–68
    - mark_component_error(component, error) 设置 error 与 last_error
      - /home/mi/work/open_source/zeroclaw/src/health/mod.rs:70–77
    - bump_component_restart(component) 饱和自增
      - /home/mi/work/open_source/zeroclaw/src/health/mod.rs:79–83
  - 快照
    - snapshot() / snapshot_json()：汇总与 JSON 序列化（失败时返回错误对象）
      - /home/mi/work/open_source/zeroclaw/src/health/mod.rs:85–103

- 网关使用
  - 网关启动后标记 gateway 组件 OK
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:787–788
  - GET /health 返回 runtime 健康 JSON（无鉴权）
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1120–1129
  - GET /api/health 返回健康快照（需鉴权）
    - /home/mi/work/open_source/zeroclaw/src/gateway/api.rs:777–788

---

## 6. Doctor 诊断系统

- 核心类型
  - Severity: Ok/Warn/Error
    - /home/mi/work/open_source/zeroclaw/src/doctor/mod.rs:14–20
  - DiagResult { severity, category, message }
    - /home/mi/work/open_source/zeroclaw/src/doctor/mod.rs:22–28

- 体检入口
  - diagnose(config) -> Vec<DiagResult>：配置语义、工作区、守护/环境/CLI 工具检查
    - /home/mi/work/open_source/zeroclaw/src/doctor/mod.rs:78–89
  - run(config)：人类可读输出（命令行）
    - /home/mi/work/open_source/zeroclaw/src/doctor/mod.rs:91–134
  - 网关 API：POST /api/doctor（也支持 GET）
    - /home/mi/work/open_source/zeroclaw/src/gateway/api.rs:594–606, 606–617

- 模型目录连通性探测
  - run_models(config, provider_override, use_cache)
    - /home/mi/work/open_source/zeroclaw/src/doctor/mod.rs:193–299
  - 错误分类：Skipped（不支持直播发现）、AuthOrAccess（401/403/429/配额/计划）、Error
    - /home/mi/work/open_source/zeroclaw/src/doctor/mod.rs:136–152, 153–180
  - 统计输出与矩阵展示（provider, status, models, detail）
    - /home/mi/work/open_source/zeroclaw/src/doctor/mod.rs:292–320

- 追踪排障（runtime_trace）
  - run_traces(config, id, event_filter, contains, limit)
    - 解析路径：resolve_trace_path
      - /home/mi/work/open_source/zeroclaw/src/doctor/mod.rs:335–345
    - 按 id 精确查找：find_event_by_id
      - /home/mi/work/open_source/zeroclaw/src/doctor/mod.rs:347–360
    - 文件不存在提示启用 rolling/full
      - /home/mi/work/open_source/zeroclaw/src/doctor/mod.rs:363–370
    - 批量查询：load_events + 过滤 + 逆序打印
      - /home/mi/work/open_source/zeroclaw/src/doctor/mod.rs:372–415

---

## 7. 与安全与网关的集成

- 健康与指标
  - /health：公开返回运行时健康 JSON（不泄露密钥）
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1120–1129
  - /metrics：Prometheus 文本暴露；若后端未启用，返回提示
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1131–1183

- 事件 SSE
  - /api/events：当 require_pairing 为真时，需 Authorization: Bearer <token>
    - /home/mi/work/open_source/zeroclaw/src/gateway/sse.rs:24–38
  - 广播内容仅包含必要元数据（不含提示/响应原文）
    - /home/mi/work/open_source/zeroclaw/src/gateway/sse.rs:85–141

- 配对与速率限制（部分）
  - POST /pair：客户端配对，含连接来源速率限制与认证尝试限流
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1185–1211

---

## 8. DORA 运维指标

- 收集器
  - DoraCollector：部署与恢复记录环形缓冲；窗口化快照（7/30/90 天）
    - /home/mi/work/open_source/zeroclaw/src/observability/dora.rs:65–73, 75–112, 114–133
  - DoraSnapshot：total_deployments、failed_deployments、change_failure_rate、mean_lead_time、mttr、window
    - /home/mi/work/open_source/zeroclaw/src/observability/dora.rs:38–53

- Prometheus 集成（DORA）
  - 指标：zeroclaw_deployments_total{status}、zeroclaw_deployment_failure_rate、zeroclaw_deployment_lead_time_seconds、zeroclaw_recovery_time_seconds、zeroclaw_mttr_seconds
  - /home/mi/work/open_source/zeroclaw/src/observability/prometheus.rs:194–231, 429–456, 497–503

---

## 9. 常见失败模式与恢复手册

- OTel 导出端点不可达
  - 记录事件/指标不 panic，flush 会发出 warn 日志
  - /home/mi/work/open_source/zeroclaw/src/observability/otel.rs:485–492
  - 建议：检查 otel_endpoint（默认 http://localhost:4318），Collector 可用性与网络 ACL

- Prometheus 后端未启用
  - /metrics 返回提示文本，指引在配置中将 backend 设为 "prometheus"
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1134–1138, 1160–1176
  - 建议：配置 [observability] backend="prometheus"，并确保网关已包装成 PrometheusObserver 或经 BroadcastObserver 间接获取

- runtime_trace 文件缺失或无匹配
  - 缺失时提示启用 runtime_trace_mode=rolling/full 并复现
  - /home/mi/work/open_source/zeroclaw/src/doctor/mod.rs:363–370, 381–386
  - 建议：在配置中设置 rolling 并调大 runtime_trace_max_entries 以保留足够上下文

- 追踪文件权限与原子性
  - 以 0600 权限写入与 set_permissions；重写采用 tmp + rename
  - /home/mi/work/open_source/zeroclaw/src/observability/runtime_trace.rs:86–100, 133–139
  - 建议：如出现权限错误，检查工作区目录与文件属主

- SSE 未授权
  - require_pairing=true 时未携带有效 Bearer 将返回 401
  - /home/mi/work/open_source/zeroclaw/src/gateway/sse.rs:24–38
  - 建议：先完成 /pair 配对，保存并使用 Bearer 令牌访问

- 配对/鉴权爆破防护
  - /pair 具速率限制与认证尝试限流
  - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1192–1211
  - 建议：遇限流重试需遵循 retry_after 提示，避免被动拒绝

---

## 10. 配置要点与示例

- ObservabilityConfig（默认值）
  - backend: "none" | "log" | "verbose" | "prometheus" | "otel"
  - otel_endpoint: 可选（默认 http://localhost:4318）
  - otel_service_name: 可选（默认 "zeroclaw"）
  - runtime_trace_mode: "none"（默认）| "rolling" | "full"
  - runtime_trace_path: "state/runtime-trace.jsonl"（默认）
  - runtime_trace_max_entries: 200（默认）
  - 定义与默认：/home/mi/work/open_source/zeroclaw/src/config/schema.rs:5096–5147

- runtime_trace 初始化与使用
  - init_from_config(config, workspace_dir)：构建（或禁用）全局 logger
    - /home/mi/work/open_source/zeroclaw/src/observability/runtime_trace.rs:178–193
  - record_event(...)：写入事件行（id 与 timestamp 自动生成）
    - /home/mi/work/open_source/zeroclaw/src/observability/runtime_trace.rs:195–230
  - CLI/Doctor 查询
    - run_traces 子命令逻辑：/home/mi/work/open_source/zeroclaw/src/doctor/mod.rs:335–415

---

## 11. 事件与指标后端映射概览（节选）

- 事件映射
  - LogObserver：tracing::info（字段见各 match 分支）
    - /home/mi/work/open_source/zeroclaw/src/observability/log.rs:15–127
  - PrometheusObserver：递增计数/直方图/仪表
    - /home/mi/work/open_source/zeroclaw/src/observability/prometheus.rs:307–457
  - OtelObserver：Counters/Histograms + 结束态 Span（带回填 start_time 与 Status）
    - /home/mi/work/open_source/zeroclaw/src/observability/otel.rs:197–440
  - BroadcastObserver：挑选事件构造最小 JSON 广播（避免敏感数据）
    - /home/mi/work/open_source/zeroclaw/src/gateway/sse.rs:85–141

- 指标映射
  - PrometheusObserver：/home/mi/work/open_source/zeroclaw/src/observability/prometheus.rs:460–505
  - OtelObserver：/home/mi/work/open_source/zeroclaw/src/observability/otel.rs:442–483

---

## 12. 与其他子系统的协作

- Hooks（提示）
  - 观测与审计可与 Hooks 协作（如命令执行审计）
  - 相关配置字段（如内置 webhook_audit）定义：
    - /home/mi/work/open_source/zeroclaw/src/config/schema.rs:5171–5203

- 网关节点发现、SSE、WS 聊天/画布接口与本文关系
  - SSE 用于实时可观测性可视化；Prometheus/OTel 用于离线与聚合监控；
  - 网关打印可用端点（含 /health 与 /metrics）：
    - /home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:780–785

---

## 13. 实践建议

- 生产环境基线
  - 至少启用 backend="prometheus" 暴露基础指标；
  - 需要统一链路追踪时启用 backend="otel"，配置 otel_endpoint；
  - runtime_trace 建议 rolling，max_entries 适当增大用于近因诊断。

- 安全基线
  - 在需要时开启 require_pairing，确保 SSE 与管理接口走 Bearer；
  - 关注 /pair 速率限制与重试窗口，配合日志与 Doctor 输出定位问题。

- 诊断流程
  - 首先看 /health 与 /api/health 健康快照；
  - 若问题涉及工具/模型调用，启用 runtime_trace 并用 doctor traces 查询；
  - 观察 /metrics 指标趋势与 OTel 后端 Span 事件。

---

## 14. 代码参考索引（关键条目）

- Observer 抽象与事件/指标
  - /home/mi/work/open_source/zeroclaw/src/observability/traits.rs:10–116, 123–145, 156–188
- 观察者工厂
  - /home/mi/work/open_source/zeroclaw/src/observability/mod.rs:28–83
- 各后端实现
  - Noop：/home/mi/work/open_source/zeroclaw/src/observability/noop.rs:7–21
  - Log：/home/mi/work/open_source/zeroclaw/src/observability/log.rs:14–176
  - Verbose：/home/mi/work/open_source/zeroclaw/src/observability/verbose.rs:16–64
  - Prometheus：/home/mi/work/open_source/zeroclaw/src/observability/prometheus.rs:1–304, 307–514
  - OpenTelemetry：/home/mi/work/open_source/zeroclaw/src/observability/otel.rs:11–501
  - Multi：/home/mi/work/open_source/zeroclaw/src/observability/multi.rs:4–41
- 运行时追踪
  - /home/mi/work/open_source/zeroclaw/src/observability/runtime_trace.rs:12–141, 144–314
- 健康
  - /home/mi/work/open_source/zeroclaw/src/health/mod.rs:8–103
- 网关与 SSE
  - /health 与 /metrics：/home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:1120–1183
  - SSE：/home/mi/work/open_source/zeroclaw/src/gateway/sse.rs:18–57, 60–161
  - 包装与路由：/home/mi/work/open_source/zeroclaw/src/gateway/mod.rs:783–800, 1003–1005
  - /api/健康 与 /api/doctor：/home/mi/work/open_source/zeroclaw/src/gateway/api.rs:594–606, 777–788
- 配置
  - ObservabilityConfig：/home/mi/work/open_source/zeroclaw/src/config/schema.rs:5094–5147
- DORA
  - /home/mi/work/open_source/zeroclaw/src/observability/dora.rs:38–53, 65–133

---