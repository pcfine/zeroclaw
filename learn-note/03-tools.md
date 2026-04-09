# 03-Tools 子系统学习笔记（ZeroClaw）

一、职责总览
- 提供面向 LLM 的“工具调用”执行面。每个工具实现 Tool trait，暴露 name/description/JSON 参数 schema 与 async execute 接口，返回结构化 ToolResult。
- 工具注册与装配：
  - default_tools：基础工具（shell + 文件读写/编辑、glob/content 搜索）。
  - all_tools_with_runtime：完整工具集（内存、浏览器、HTTP、Web 搜索、图像、知识图谱、Jira/Notion/M365 等集成、委派/群智、SOP/管线、Composio/MCP…），按配置有条件注入。
- 安全与治理：
  - 统一封装的 RateLimitedTool、PathGuardedTool 在 execute 之前施加速率限制与路径防护。
  - SecurityPolicy 注入到具体工具（如 Shell）并通过 create_sandbox 加强隔离。
  - 审批钩子与 CLI 交互（approval manager）在 loop_.rs 统一把关。
  - 输出与日志的凭据脱敏（scrub_credentials）。
- MCP 延迟加载：
  - DeferredMcpToolSet/ActivatedToolSet 管理延迟加载的 MCP 工具，ToolSearchTool 在需要时激活并把 tool spec 注入当轮上下文。
  - MCP 工具通过 McpToolWrapper 适配成统一 Tool。
- 执行流程与并发：
  - dispatcher 解析工具调用（native vs XML），loop_.rs 汇总 tool_specs、过滤/审批/去重后，择机并发或串行执行（execute_tools_parallel/sequential）。
  - 工具执行通过 execute_one_tool 统一采集成功/错误/耗时、处理取消、套用凭据脱敏。
- 错误处理与失败模式：
  - 未知工具、审批拒绝、重复调用、路径阻断、超额速率、Hook 取消、并发禁用（含 tool_search 竞态规避）、取消令牌触发等，均返回结构化错误。

二、核心类型与约定
1) Tool trait / ToolSpec / ToolResult
- 定义：
  - ToolResult：{ success: bool, output: String, error: Option<String> }
  - ToolSpec：{ name, description, parameters(JSON Schema) }
  - Tool：name()/description()/parameters_schema()/execute()，以及 helper spec()
- 代码摘录（简化）：
  - /home/mi/work/open_source/zeroclaw/src/tools/traits.rs:4-43
    - pub struct ToolResult { success, output, error }
    - pub struct ToolSpec { name, description, parameters }
    - pub trait Tool { fn name/description/parameters_schema; async fn execute; fn spec }
- 约定：
  - 所有工具的参数必须能序列化为 JSON（并提供 schema）。
  - execute 返回 Ok(ToolResult) 即表示“工具成功执行否”的语义由 success 表达；致命异常才用 Err(e)。

2) 包装器（Wrappers）
- RateLimitedTool<T: Tool>
  - 在调用 inner.execute 前，检查 SecurityPolicy::is_rate_limited / record_action。
  - 超限返回 ToolResult { success: false, error: "Rate limit exceeded..." }，不触发 inner。
  - /home/mi/work/open_source/zeroclaw/src/tools/wrappers.rs:33-84
- PathGuardedTool<T: Tool>
  - 从 args 中提取可能的路径参数（默认字段："path","command","pattern","query","file"，可自定义 - extractor）。
 使用 SecurityPolicy::forbidden_path_argument / is_path_allowed 阻断敏感路径或命令 拼接路径。
  - 阻断返回 ToolResult { success: false, error: "Path blocked..." }，不触发 inner。
  - /home/mi/work/open_source/zeroclaw/src/tools/wrappers.rs:88-177
- 组合顺序（外→内）：RateLimitedTool → PathGuardedTool → Concrete Tool（如 Shell）
  - /home/mi/work/open_source/zeroclaw/src/tools/wrappers.rs:8-15
  - 在注册时对 Shell 已按上述顺序封装（见后文）。

三、工具注册与装配流程
1) default_tools / default_tools_with_runtime
- 默认工具列表：
  - shell（RateLimited(PathGuarded(ShellTool))）
  - file_read / file_write / file_edit
  - glob_search / content_search
- 构造：
  - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:285-306（default_tools 与 default_tools_with_runtime）
  - Shell 工具在 all_tools_with_runtime 中使用 create_sandbox/with_timeout 替换实现（见下）。
2) all_tools / all_tools_with_runtime（完整注册表）
- 入口与签名：
  - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:381-409（签名与返回多个 handle）
- 初始工具集（节选）：
  - Shell：Arc<RateLimitedTool<PathGuardedTool<ShellTool::new_with_sandbox(...).with_timeout_secs(...)>>>
    - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:410-420
  - 文件/搜索/计划任务/Cron/Memory/模型路由与切换/代理配置/Git/Pushover/计算器/天气/画布 等：
    - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:421-455（及后续大量 push）
  - 浏览器与代理（按配置启用）：
    - BrowserOpenTool / BrowserTool / BrowserDelegateTool
    - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:515-554
  - HttpRequestTool / WebFetchTool / TextBrowserTool / WebSearchTool：
    - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:556-599
  - Notion / Jira / ProjectIntel / SecurityOps / Backup / DataManagement / GoogleWorkspace：
    - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:600-700, 643-681, 653-658, 660-675, 683-700
  - ClaudeCode / ClaudeCodeRunner / CodexCli / GeminiCli / OpenCodeCli：
    - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:701-744
  - PDF / 截图 / 图片信息 / 会话工具 / LinkedIn / ImageGen：
    - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:746-784
  - Poll / SOP（需 sops_dir）/ Composio / Reaction / AskUser / Escalate：
    - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:786-829, 805-813, 815-823, 825-829
  - Microsoft365 / Knowledge / Delegate / Swarm / Workspace / VerifiableIntent：
    - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:830-889, 892-911, 914-956, 958-971, 973-988, 990-999
  - Pipeline 工具（管线链式执行）：
    - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:1049-1056
- 技能工具注册（skills → tools）：
  - register_skill_tools 会将 skills 转换出的工具追加到注册表，并避免重名覆盖。
  - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:308-333
- 装配产出：
  - Vec<Box<dyn Tool>> 与若干 Channel/Delegate 句柄，用于后续通道与委派功能联动。

四、MCP 集成与延迟激活
1) McpToolWrapper（MCP → Tool 适配器）
- 将 MCP server 的工具包裹为本地 Tool，透出 name/description/schema，execute 时调用 registry.call_tool。
- 安全注释：剥离 approved 字段（监督模式会注入该字段；MCP 端不认识，需移除）。
  - /home/mi/work/open_source/zeroclaw/src/tools/mcp_tool.rs:12-26, 40-80（剥离逻辑：54-66）
2) DeferredMcpToolSet 与 ActivatedToolSet
- DeferredMcpToolSet：收集 MCP 注册中心所有工具的轻量 stub，支持关键词搜索与 select。
  - /home/mi/work/open_source/zeroclaw/src/tools/mcp_deferred.rs:53-151
- ActivatedToolSet：每会话已激活工具集合，提供：
  - activate/is_activated/get/tool_specs/tool_names
  - get_resolved：兼容某些 provider 把 server 前缀剥离后的“唯一后缀”解析。
  - /home/mi/work/open_source/zeroclaw/src/tools/mcp_deferred.rs:153-221（唯一后缀解析：183-211）
3) ToolSearchTool（内置工具）
- 用于延迟加载场景：按 “select:name1,name2” 或关键词搜索激活 MCP 工具，将其完整 <function>{spec}</function> 返回并放入 ActivatedToolSet。
- 关键逻辑：
  - execute：解析 query 与 max_results，选择 select/搜索路径；激活并累积函数块；统计 activated_count。
  - /home/mi/work/open_source/zeroclaw/src/tools/tool_search.rs:1-31, 34-61, 63-137
  - select_tools：逐名激活 + 输出函数块 + 记录未命中。
  - /home/mi/work/open_source/zeroclaw/src/tools/tool_search.rs:140-191
4) Agent 中的 MCP 装配
- config.mcp.deferred_loading=true：
  - 创建 DeferredMcpToolSet，注入 ToolSearchTool，并维护 Arc<Mutex<ActivatedToolSet>>。
- 非延迟模式：直接把各 MCP 工具包装为 Tool 并注册。
- /home/mi/work/open_source/zeroclaw/src/agent/agent.rs:421-475

五、执行流与 Dispatcher 交互
1) Dispatcher（native vs XML）
- NativeToolDispatcher：
  - parse_response：从 provider 返回的 tool_calls 列表解析 name/arguments（JSON 失败回退空对象）。
  - format_results：将 ToolExecutionResult 列表转 ConversationMessage::ToolResults。
  - should_send_tool_specs = true（向 provider 发送完整 tool specs）。
  - /home/mi/work/open_source/zeroclaw/src/agent/dispatcher.rs:171-251
- XmlToolDispatcher：
  - parse_response：从 <tool_call>{...}</tool_call> 标签解析；strip_think_tags 剥离 <think>..</think>。
  - tool_specs() 静态方法：把 Vec<Box<dyn Tool>> 转成 Vec<ToolSpec>。
  - should_send_tool_specs = false（XML 协议下不单独发 specs，由系统提示给出协议说明）。
  - /home/mi/work/open_source/zeroclaw/src/agent/dispatcher.rs:29-169（tool_specs：107-110）
2) 每轮构建 tool_specs（含延迟激活）
- loop_.rs 中，每次迭代重新构建 tool_specs，合并 ActivatedToolSet 中新激活的工具，并按过滤组排除 MCP 不相关工具。
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:2404-2417
- 过滤规则（filter_tool_specs_for_turn）：
  - 内置工具（非 mcp_ 前缀）总是传递。
  - groups 为空则放行全部。
  - MCP 工具：always 组按 glob 匹配即入；dynamic 组需命中关键词才入。
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:90-135
3) 执行阶段关键钩子与策略
- before_tool_call Hook（可修改或取消）：
  - Cancel → 记录结果“Cancelled by hook: …”；Continue 可修改 name/args。
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:2945-2997
- 审批（ApprovalManager）：
  - 需要审批的工具在 CLI 交互中提示；非交互通道默认拒绝（Denied by user）。
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3006-3061
- 去重（相同工具名 + 规范化参数签名）：
  - 重复调用直接跳过并返回“Skipped duplicate…”错误结果，且标记 deduplicated: true。
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3063-3107
- 并发与串行决策：
  - 多个调用且无需交互审批 → 尝试并发；若批次中包含 tool_search，强制串行以避免激活竞态。
  - /home/mi/work/open_source/zeroclaw/src/agent/tool_execution.rs:129-154
  - 实际执行入口：execute_tools_parallel（join_all）/execute_tools_sequential。
  - /home/mi/work/open_source/zeroclaw/src/agent/tool_execution.rs:158-181, 185-209
- after_tool_call Hook（仅读）：
  - 把 ToolResult 投递给 hook 以便审计。
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3201-3209

六、单个工具执行与错误处理
1) execute_one_tool（统一入口）
- 查找顺序：先静态注册表 find_tool；未找到时尝试 ActivatedToolSet::get_resolved（支持唯一后缀）。
- 未找到：返回 Ok(ToolExecutionOutcome { success: false, error_reason: "Unknown tool: …" }).
- 执行：
  - 尊重取消令牌（tokio::select），取消时 Err(ToolLoopCancelled) 上抛。
  - ToolResult.success = true → 脱敏 scrub_credentials 后作为 output。
  - ToolResult.success = false → 取 error.unwrap_or(output) → 作为 error_reason（脱敏），output=“Error: reason”。
  - Err(e) → 统一封装 "Error executing {tool}: {e}" 为 error_reason（脱敏）。
- /home/mi/work/open_source/zeroclaw/src/agent/tool_execution.rs:37-125
2) 凭据脱敏（scrub_credentials）
- 正则匹配键（token/api_key/password/secret/user_key/bearer/credential）和值（>=8 字符），保留前 4 个字符，其余替换为 *[REDACTED]；支持 : 或 = 与双引号/单引号包裹值。
- 在多处用于：
  - 记录 tool_call_result 事件、错误展示、向模型回显 tool 输出等。
- /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:213-257（函数定义）
- 使用示例：记录事件输出与错误处均调用 scrub_credentials。
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3194-3199, 2973-2977 等

七、安全策略与守护
- 统一安全注入：
  - SecurityPolicy 在工具构造时注入，用于：
    - 速率限制（is_rate_limited / record_action）→ RateLimitedTool 统一封装。
    - 路径/命令中的敏感路径阻断（forbidden_path_argument/is_path_allowed）→ PathGuardedTool 统一封装。
  - ShellTool 通过 create_sandbox 注入沙箱与超时。
  - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:219（import）, 410-420（Shell 封装）
- 审批与最小权限：
  - 交互式 CLI 审批、非交互默认拒绝，避免无人值守下的敏感操作。
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3006-3061
- MCP 参数整洁：
  - McpToolWrapper.execute 去除 approved 字段，避免把本地监督字段泄露到 MCP 服务器。
  - /home/mi/work/open_source/zeroclaw/src/tools/mcp_tool.rs:54-66
- 工具过滤：
  - filter_tool_specs_for_turn 针对 MCP 的工具过滤组（always/dynamic），结合用户消息关键词，仅向模型暴露必要的工具 schema，降低越权面与上下文成本。
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:90-135
- 并发竞态规避：
  - tool_search 与其激活的工具不能同批并发执行，避免在查找时尚未激活导致的 Unknown tool。
  - /home/mi/work/open_source/zeroclaw/src/agent/tool_execution.rs:137-145
- 输出与日志脱敏：
  - scrub_credentials 全面应用在向人/模型回显与事件日志中，防止敏感凭据外泄。

八、常见内置工具（按大类，详见代码中 all_tools_with_runtime 的 push 列表）
- 基础与文件：
  - shell, file_read, file_write, file_edit, glob_search, content_search
- 计划与内存：
  - cron_add/list/remove/run/runs/update, memory_store/recall/forget/export/purge, schedule
- 模型与路由：
  - model_routing_config, model_switch, proxy_config
- 开发与通知：
  - git_operations, pushover, pdf_read, screenshot, image_info, image_gen
- 浏览与请求：
  - browser_open, browser（自动化）, browser_delegate, http_request, web_fetch, text_browser, web_search
- 协作与委派：
  - delegate, swarm, sessions_list/history/send, ask_user, reaction, escalate_to_human, poll
- 业务/集成：
  - notion, jira, microsoft365, google_workspace, linkedin, project_intel, security_ops, data_management, backup
- 知识与推理：
  - knowledge_tool, llm_task, verifiable_intent, pipeline
- 生态扩展：
  - composio, claude_code, claude_code_runner, codex_cli, gemini_cli, opencode_cli
- 工作空间与 SOP：
  - workspace_tool, sop_list/execute/advance/approve/status
- MCP（按配置）：
  - mcp__*（非延迟：直接注册；延迟：通过 tool_search 激活）

九、典型失败/边界场景
- 未知工具：
  - 未注册/未激活时 execute_one_tool 返回 error_reason="Unknown tool: …"（非致命 Err）。
  - /home/mi/work/open_source/zeroclaw/src/agent/tool_execution.rs:58-72
- 审批拒绝：
  - 直接构造失败结果“Denied by user.”，并继续下一调用。
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3024-3057
- 重复调用：
  - 同轮内相同签名跳过，不消耗执行资源。
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3063-3107
- 路径阻断 / 速率限制：
  - PathGuardedTool / RateLimitedTool 提前返回失败结果，inner 未触发。
  - /home/mi/work/open_source/zeroclaw/src/tools/wrappers.rs:151-176, 65-82
- Hook 取消：
  - before_tool_call 可直接取消，记录 “Cancelled by hook: …”。
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:2945-2990
- tool_search 并发禁用：
  - 避免竞态（强制串行）。
  - /home/mi/work/open_source/zeroclaw/src/agent/tool_execution.rs:137-145
- 执行取消：
  - CancellationToken 触发 → Err(ToolLoopCancelled) 上抛，由上层处理。
  - /home/mi/work/open_source/zeroclaw/src/agent/tool_execution.rs:75-80
- 凭据外泄风险：
  - scrub_credentials 在多处输出通道被调用；注意：工具内部日志不在此范围，需工具自身注意。
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:213-257

十、执行路径小结（从模型到工具到回传）
- 模型输出 → Dispatcher 解析工具调用（Native: tool_calls；XML: <tool_call>）。
  - /home/mi/work/open_source/zeroclaw/src/agent/dispatcher.rs:171-193, 32-86
- loop_.rs：
  - 重新构建 tool_specs（含 ActivatedToolSet），按过滤组筛选。
  - before_tool_call → 审批 → 去重 → 决策并发/串行（tool_search 特例）。
  - execute_one_tool/execute_tools_* 执行，记录 runtime_trace/ObserverEvent。
  - after_tool_call hook → 汇总 ToolExecutionResult → Dispatcher.format_results → 发回 provider 历史。
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:2404-2417, 2945-3178, 3201-3209
  - /home/mi/work/open_source/zeroclaw/src/agent/tool_execution.rs:37-209

附：关键代码片段（精选）

- Tool/ToolSpec/ToolResult（定义）
  - /home/mi/work/open_source/zeroclaw/src/tools/traits.rs:4-43
  - 片段：
    - pub struct ToolResult { success: bool, output: String, error: Option<String> }
    - pub struct ToolSpec { name: String, description: String, parameters: serde_json::Value }
    - pub trait Tool { fn name/description/parameters_schema; async fn execute; fn spec }

- 默认工具装配
  - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:285-306
  - 片段：
    - vec![
      RateLimitedTool::new(PathGuardedTool::new(ShellTool::new(...), ...), ...),
      FileReadTool, FileWriteTool, FileEditTool, GlobSearchTool, ContentSearchTool
    ]

- 全量工具装配（Shell 封装/沙箱/超时）
  - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:410-420
  - 片段：
    - RateLimitedTool::new(
        PathGuardedTool::new(
          ShellTool::new_with_sandbox(...).with_timeout_secs(...),
          ...
        ),
        ...
      )

- Wrappers：RateLimitedTool 执行
  - /home/mi/work/open_source/zeroclaw/src/tools/wrappers.rs:65-82
  - 片段：
    - if self.security.is_rate_limited() { return Ok(ToolResult{success:false,...}) }
    - if !self.security.record_action() { return Ok(ToolResult{success:false,...}) }
    - self.inner.execute(args).await

- Wrappers：PathGuardedTool 执行
  - /home/mi/work/open_source/zeroclaw/src/tools/wrappers.rs:151-176
  - 片段：
    - if let Some(path) = extract_path { if forbidden → return Ok(fail); }
    - self.inner.execute(args).await

- MCP 延迟集与激活
  - DeferredMcpToolSet（搜索/激活/spec）：
    - /home/mi/work/open_source/zeroclaw/src/tools/mcp_deferred.rs:53-151
  - ActivatedToolSet（唯一后缀解析 get_resolved）：
    - /home/mi/work/open_source/zeroclaw/src/tools/mcp_deferred.rs:183-211
  - ToolSearchTool.execute（select/搜索激活，输出 <function> 块）：
    - /home/mi/work/open_source/zeroclaw/src/tools/tool_search.rs:84-121, 140-171

- MCP Tool 包装与 approved 字段剥离
  - /home/mi/work/open_source/zeroclaw/src/tools/mcp_tool.rs:54-66
  - 片段：
    - match args { Object(mut map) => { map.remove("approved"); Object(map) } ... }

- Dispatcher 工具规格输出与解析
  - XmlToolDispatcher::tool_specs：
    - /home/mi/work/open_source/zeroclaw/src/agent/dispatcher.rs:107-110
  - NativeToolDispatcher（should_send_tool_specs=true）：
    - /home/mi/work/open_source/zeroclaw/src/agent/dispatcher.rs:248-251

- 每轮 tool_specs（合并已激活）
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:2404-2417

- MCP 工具过滤（always/dynamic/关键词）
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:90-135

- 并发决策（tool_search 特例）
  - /home/mi/work/open_source/zeroclaw/src/agent/tool_execution.rs:137-145

- 单工具执行与错误封装/脱敏
  - execute_one_tool：
    - /home/mi/work/open_source/zeroclaw/src/agent/tool_execution.rs:37-125
  - scrub_credentials：
    - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:213-257

- 执行前后 Hook / 审批 / 去重
  - before_tool_call：
    - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:2945-2997
  - 审批（ApprovalManager）：
    - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3006-3061
  - 去重：
    - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3063-3107
  - after_tool_call：
    - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3201-3209

- Agent 中 MCP 装配（延迟 vs 直接）
  - /home/mi/work/open_source/zeroclaw/src/agent/agent.rs:421-475

参考：更多完整工具清单详见
- /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:410-1066（all_tools_with_runtime 内部 push 列表）