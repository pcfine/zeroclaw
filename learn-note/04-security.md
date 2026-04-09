# A4 学习笔记：ZeroClaw 安全子系统（Security）

## 概述
- 安全目标与范围
  - 工具执行安全：命令风险分级、路径越界防护、沙箱与超时、环境变量白名单
  - 速率限制：按发送方/会话限制操作频次，防止滥用与高频消耗
  - 审批流：在监督/渠道模式下对高风险工具调用进行人工审批或自动拒绝
  - 凭据与内容安全：Prompt 注入防护（PromptGuard）、外发数据泄露检测（LeakDetector）、配置展示与更新中的敏感字段脱敏
  - 凭据存储与迁移：SecretStore 使用 ChaCha20-Poly1305 加密，支持从旧 XOR 方案迁移
  - 网关鉴权与连接安全：Pairing 首次配对与持久令牌、API Bearer Token 鉴权、TLS/mTLS 与证书指纹钉扎
  - SSRF 防护：HttpRequest 工具域名白名单、私网/本地地址阻断、超时与响应截断
- 执行链路（工具封装顺序）
  - RateLimitedTool → PathGuardedTool → 具体工具（如 ShellTool / HttpRequestTool）
  - ShellTool 内部再应用 Sandbox.wrap_command 与命令超时控制

## 核心类型与职责
- SecurityPolicy（全局策略与校验）
  - 字段与默认值：/home/mi/work/open_source/zeroclaw/src/security/policy.rs:172-188, 305-321
  - 默认允许命令（Unix/Windows）：/home/mi/work/open_source/zeroclaw/src/security/policy.rs:190-222, 229-262
  - 默认禁止路径（Unix/Windows）：/home/mi/work/open_source/zeroclaw/src/security/policy.rs:264-287, 289-303
  - 路径允许判定：is_path_allowed() /home/mi/work/open_source/zeroclaw/src/security/policy.rs:1335-1397
  - 命令风险与校验：
    - split_unquoted_segments：/home/mi/work/open_source/zeroclaw/src/security/policy.rs:419-506
    - contains_unquoted_single_ampersand：/home/mi/work/open_source/zeroclaw/src/security/policy.rs:512-576
    - contains_unquoted_char：/home/mi/work/open_source/zeroclaw/src/security/policy.rs:579-624
    - contains_unquoted_shell_variable_expansion（变量注入识别）：/home/mi/work/open_source/zeroclaw/src/security/policy.rs:630-659
    - command_risk_level：/home/mi/work/open_source/zeroclaw/src/security/policy.rs:868-1002
    - validate_command_execution：/home/mi/work/open_source/zeroclaw/src/security/policy.rs:1003-1264
    - forbidden_path_argument（命令中路径提取与阻断）：/home/mi/work/open_source/zeroclaw/src/security/policy.rs:1265-1334
  - 配置装载：from_config() /home/mi/work/open_source/zeroclaw/src/security/policy.rs:1580-1611
- 工具包装器（横切关注点）
  - RateLimitedTool：调用前检查并记录速率预算；/home/mi/work/open_source/zeroclaw/src/tools/wrappers.rs:35-84
  - PathGuardedTool：从参数提取路径/命令并执行路径策略阻断；/home/mi/work/open_source/zeroclaw/src/tools/wrappers.rs:88-177
- Shell 执行安全
  - ShellTool：参数校验→策略校验（validate_command_execution）→沙箱包装→环境变量清洗→超时执行；/home/mi/work/open_source/zeroclaw/src/tools/shell.rs:138-240
  - Sandbox 抽象：/home/mi/work/open_source/zeroclaw/src/security/traits.rs:22-52；NoopSandbox 提供空实现：/home/mi/work/open_source/zeroclaw/src/security/traits.rs:54-80
- 审批管理（ApprovalManager）
  - 交互/非交互两种模式；Auto-approve / Always-ask / 会话级 Always 名单；/home/mi/work/open_source/zeroclaw/src/approval/mod.rs:59-101
  - 核心判定 needs_approval()：/home/mi/work/open_source/zeroclaw/src/approval/mod.rs:113-151
  - 记录审计与“Always”允许清单：record_decision() /home/mi/work/open_source/zeroclaw/src/approval/mod.rs:153-169
- Prompt 注入防护（PromptGuard）
  - 枚举与行为：GuardResult, GuardAction, PromptGuard；/home/mi/work/open_source/zeroclaw/src/security/prompt_guard.rs:19-39, 53-58
  - scan() 评分与分类封锁/警告：/home/mi/work/open_source/zeroclaw/src/security/prompt_guard.rs:83-131
  - 模式类目：系统覆盖、角色混淆、工具 JSON 注入、密钥提取、命令注入、Jailbreak；/home/mi/work/open_source/zeroclaw/src/security/prompt_guard.rs:133+
- 凭据泄露检测（LeakDetector）
  - LeakResult, LeakDetector 定义：/home/mi/work/open_source/zeroclaw/src/security/leak_detector.rs:16-35
  - scan() 检测与脱敏：/home/mi/work/open_source/zeroclaw/src/security/leak_detector.rs:56-75（API Key、AWS、通用秘密、私钥、JWT、DB URL、高熵令牌）
- SecretStore（加密存储）
  - 结构体与加密：/home/mi/work/open_source/zeroclaw/src/security/secrets.rs:37, 56-76
  - 解密与迁移（enc2:/enc:/明文）：/home/mi/work/open_source/zeroclaw/src/security/secrets.rs:85-93, 101-120
  - 密钥文件管理与权限：/home/mi/work/open_source/zeroclaw/src/security/secrets.rs:171-255
- 网关安全
  - PairingGuard：首次配对、失败尝试计数与退避、恒定时间比较；/home/mi/work/open_source/zeroclaw/src/security/pairing.rs:43, 99-199, 201-209, 331-350
  - TLS/mTLS 与证书钉扎：/home/mi/work/open_source/zeroclaw/src/gateway/tls.rs:18-24, 24-91, 94-100, 105-165
  - API 鉴权与配置脱敏：/home/mi/work/open_source/zeroclaw/src/gateway/api.rs:13, 26-45, 964-1020+

## 执行链路（工具调用到安全防护）
- 组装顺序（外→内）
  - RateLimitedTool → PathGuardedTool → <具体工具>；/home/mi/work/open_source/zeroclaw/src/tools/wrappers.rs:10-23, 33-84, 86-177
- Shell 工具（示例链路）
  - PathGuardedTool 依据参数 "command"/"path"/"pattern"/"query"/"file" 拦截路径；/home/mi/work/open_source/zeroclaw/src/tools/wrappers.rs:123-133, 151-176
  - ShellTool 在 execute() 中：
    - 使用 SecurityPolicy.validate_command_execution 校验命令风险与审批；/home/mi/work/open_source/zeroclaw/src/tools/shell.rs:148-157
    - Sandbox.wrap_command 应用 OS 级隔离；/home/mi/work/open_source/zeroclaw/src/tools/shell.rs:176-182
    - 清空环境并仅透传允许变量；/home/mi/work/open_source/zeroclaw/src/tools/shell.rs:183-189（结合 policy.shell_env_passthrough）
    - 应用超时；/home/mi/work/open_source/zeroclaw/src/tools/shell.rs:191-239
- HttpRequest 工具（SSR F防护）
  - validate_url()：仅 http/https、允许列表、阻断私网/本地/保留地址；/home/mi/work/open_source/zeroclaw/src/tools/http_request.rs:35-67, 401-693
  - execute()：策略 can_act/record_action、超时、最大响应大小截断与头部脱敏展示；/home/mi/work/open_source/zeroclaw/src/tools/http_request.rs:204-306（关键路径）
- 审批流集成
  - 在代理执行多工具前判断 needs_approval 并综合 Autonomy；/home/mi/work/open_source/zeroclaw/src/agent/tool_execution.rs:131, 145-147
  - Agent 主循环在工具执行前插入审批与决策；/home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3006-3016, 2934-2940

## 网关鉴权与 mTLS/证书钉扎
- API 层鉴权
  - require_auth() 使用 PairingGuard 对所有 /api 路径进行 Bearer Token 校验；/home/mi/work/open_source/zeroclaw/src/gateway/api.rs:26-45, 多处调用见 100, 134, 166 等
  - 配置接口响应前 mask_sensitive_fields，对密钥字段用 "***MASKED***"；/home/mi/work/open_source/zeroclaw/src/gateway/api.rs:13, 964-1020, 1720-1791 等
- TLS/mTLS
  - 构建服务端配置、可选客户端证书校验与指纹钉扎；/home/mi/work/open_source/zeroclaw/src/gateway/tls.rs:18-24, 24-91, 94-100, 105-165

## 配置要点（与安全相关）
- Autonomy 与动作预算
  - level（ReadOnly/Supervised/Full）、workspace_only、max_actions_per_hour、max_cost_per_day_cents
  - require_approval_for_medium_risk、block_high_risk_commands
  - shell_env_passthrough、shell_timeout_secs、allowed_commands、forbidden_paths、allowed_roots
  - 注：from_config 正常化 allowed_roots（~ 展开/相对路径转工作区内绝对路径）；/home/mi/work/open_source/zeroclaw/src/security/policy.rs:1580-1611
- 网关与 TLS
  - paired_tokens、pairing_code、mTLS 客户端证书与 pinned_certs（SHA-256 指纹）

## 常见失败模式与返回
- 速率限制
  - "Rate limit exceeded: too many actions in the last hour" 或 "action budget exhausted"；/home/mi/work/open_source/zeroclaw/src/tools/wrappers.rs:66-81
- 路径防护
  - "Path blocked by security policy: {path}"；/home/mi/work/open_source/zeroclaw/src/tools/wrappers.rs:166-171
- Shell 安全校验
  - validate_command_execution 返回风险/审批/语义阻断原因作为 error；/home/mi/work/open_source/zeroclaw/src/tools/shell.rs:148-156
  - 超时："Command timed out after {timeout_secs}s and was killed"；/home/mi/work/open_source/zeroclaw/src/tools/shell.rs:232-237
  - 沙箱错误："Sandbox error: ..."；/home/mi/work/open_source/zeroclaw/src/tools/shell.rs:179-181
- HTTP 请求阻断
  - URL 校验失败（非允许域/私网/非法表示/非 http(s)）；/home/mi/work/open_source/zeroclaw/src/tools/http_request.rs:35-67, 852-870（替代表示测试）
- 网关鉴权失败
  - 未带或错误 Bearer Token → require_auth 返回 401/错误；/home/mi/work/open_source/zeroclaw/src/gateway/api.rs:26-45
- 配置脱敏
  - 读取/回显配置时敏感字段为 "***MASKED***"；/home/mi/work/open_source/zeroclaw/src/gateway/api.rs:13, 964-1020

## 代码索引（主要文件）
- 策略与执行
  - /home/mi/work/open_source/zeroclaw/src/security/policy.rs
  - /home/mi/work/open_source/zeroclaw/src/tools/wrappers.rs
  - /home/mi/work/open_source/zeroclaw/src/tools/shell.rs
  - /home/mi/work/open_source/zeroclaw/src/tools/http_request.rs
  - /home/mi/work/open_source/zeroclaw/src/security/traits.rs
- 审批与代理集成
  - /home/mi/work/open_source/zeroclaw/src/approval/mod.rs
  - /home/mi/work/open_source/zeroclaw/src/agent/tool_execution.rs
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs
- Prompt/泄露/秘钥
  - /home/mi/work/open_source/zeroclaw/src/security/prompt_guard.rs
  - /home/mi/work/open_source/zeroclaw/src/security/leak_detector.rs
  - /home/mi/work/open_source/zeroclaw/src/security/secrets.rs
- 网关安全
  - /home/mi/work/open_source/zeroclaw/src/security/pairing.rs
  - /home/mi/work/open_source/zeroclaw/src/gateway/tls.rs
  - /home/mi/work/open_source/zeroclaw/src/gateway/api.rs

## 附：关键片段（摘录）
- 工具包装顺序与阻断
  - /home/mi/work/open_source/zeroclaw/src/tools/wrappers.rs:151-176
    - if let Some(arg) = self.extract_path_string(&args) { ... if self.security.forbidden_path_argument(&arg) ... else if !self.security.is_path_allowed(&arg) ... return Ok(ToolResult { success: false, error: Some(format!("Path blocked by security policy: {path}")) }) }
- Shell 执行安全链
  - /home/mi/work/open_source/zeroclaw/src/tools/shell.rs:148-157, 176-189, 191-199, 232-237
    - validate_command_execution() 决策 → sandbox.wrap_command(...) → env_clear + 仅白名单透传 → tokio 超时 → 超时错误消息
- 路径允许判定（层次化）
  - /home/mi/work/open_source/zeroclaw/src/security/policy.rs:1334-1397
    - 拒绝 NUL、../ 与 URL 编码穿越、~user；绝对路径需在 workspace/allowed_roots 内或被拒；最后做 forbidden_paths 前缀阻断
- 命令结构化解析与危险操作识别
  - /home/mi/work/open_source/zeroclaw/src/security/policy.rs:419-506, 512-576, 579-624, 630-659
    - 分段解析保留引号、识别单 &、重定向符号、变量展开等，以支持风险分级与校验
- SecretStore 加密与迁移
  - /home/mi/work/open_source/zeroclaw/src/security/secrets.rs:56-76, 101-120, 171-255
    - ChaCha20-Poly1305 AEAD 加密（enc2:），支持 legacy enc: XOR 自动迁移，首次使用生成/加载密钥文件并设置权限
- Prompt 注入与泄露检测
  - /home/mi/work/open_source/zeroclaw/src/security/prompt_guard.rs:83-131
    - 多类模式评分，Block/Sanitize/Warn 动作
  - /home/mi/work/open_source/zeroclaw/src/security/leak_detector.rs:56-75
    - 多类凭据模式检测并返回脱敏内容
- 网关鉴权与钉扎
  - /home/mi/work/open_source/zeroclaw/src/security/pairing.rs:201-209, 331-350
    - is_authenticated() 使用恒定时间比较 constant_time_eq()
  - /home/mi/work/open_source/zeroclaw/src/gateway/tls.rs:105-165
    - PinnedCertVerifier 校验客户端证书指纹

## 备注
- 本笔记聚焦安全相关结构与链路，更多行为与测试覆盖可参见各文件内测试用例（policy.rs 与 http_request.rs 测试覆盖了路径/命令/SSRF 的多种边界条件）。