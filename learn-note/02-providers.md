# A2：Providers 子系统分析

以下为对子系统“Providers”的结构化中文笔记。

一、概览：模块职责与边界
- Provider 子系统将上层 Agent 的统一对话与工具接口，适配到各家模型推理后端（OpenAI、Anthropic、Bedrock、Ollama、GLM、Gemini 等），通过 Provider trait 抽象，提供统一的聊天、工具调用、流式事件与能力声明。
- 关键入口与类型统一于 traits.rs：ChatMessage、ToolCall、ChatResponse、TokenUsage、StreamEvent、ProviderCapabilities、ToolsPayload 及 Provider trait 默认实现，决定“是否走原生工具调用或走 PromptGuided 注入”的策略。
  - 参见 /home/mi/work/open_source/zeroclaw/src/providers/traits.rs:273-286（ProviderCapabilities），294-305（ToolsPayload），317-327（convert_tools 默认返回 PromptGuided），374-417（chat() 在非原生能力下注入工具指令的回退路径），188-200（StreamEvent 定义）。
- 具体 Provider 模块负责各家 API 的请求体/响应体序列化、工具定义映射、能力声明与可选的流式实现（例如 Anthropic 完成了 StreamEvent::ToolCall 事件分发；OpenAI 当前未实现流式）。
- providers/mod.rs 维护 Provider 工厂与路由/可靠性封装（ReliableProvider），并提供上游错误信息净化（去敏）工具。
  - 参见 /home/mi/work/open_source/zeroclaw/src/providers/mod.rs:807-831（sanitize_api_error），1047-1076（工厂方法入口）。

二、能力模型（native_tool_calling / vision / prompt_caching）
- ProviderCapabilities
  - 字段定义：native_tool_calling、vision、prompt_caching（/home/mi/work/open_source/zeroclaw/src/providers/traits.rs:273-286）。
  - 默认 capabilities() 返回全 False；supports_native_tools()/supports_vision() 默认读取该结构（traits.rs:431-439）。
- 典型实现对比
  - Anthropic：三者全 True（/home/mi/work/open_source/zeroclaw/src/providers/anthropic.rs:880-886），并实现 prompt caching 策略：基于系统/会话长度决定 cache_control 注入（同文件:255-273, 828-838, 985-997）。
  - Bedrock：native_tool_calling/vision True，prompt_caching False（/home/mi/work/open_source/zeroclaw/src/providers/bedrock.rs:1051-1056）。
  - OpenAI：未覆盖 capabilities()，但覆写 supports_native_tools() 返回 True（/home/mi/work/open_source/zeroclaw/src/providers/openai.rs:459-462），并在 usage 中映射 cached_tokens（442-447）。
  - Ollama：native_tool_calling False（/home/mi/work/open_source/zeroclaw/src/providers/ollama.rs:632-634），以 PromptGuided 与“嵌入式标签解析”结合实现工具流程。

能力影响：
- native_tool_calling=False 时，Provider.chat() 会将工具协议与 schema 以文本注入（PromptGuided），不走 chat_with_tools（traits.rs:374-417）。
- vision=True 允许多模态消息转换：Anthropic 将用户图像占位与内容块组合（/home/mi/work/open_source/zeroclaw/src/providers/anthropic.rs:465-486），Bedrock/Ollama亦有各自格式。
- prompt_caching=True 时，Provider 可设置 cache_control 并从响应 usage 提取 cached_input_tokens（Anthropic：/home/mi/work/open_source/zeroclaw/src/providers/anthropic.rs:520-525；OpenAI：/home/mi/work/open_source/zeroclaw/src/providers/openai.rs:442-447）。

三、工具调用路径：PromptGuided vs 原生（含 convert_tools）
- 统一工具载体 ToolsPayload（traits.rs:294-305）：
  - Gemini（functionDeclarations）、Anthropic（tools+input_schema）、OpenAI（tools:function）、PromptGuided（instructions 文本）。
- 默认回退（PromptGuided）
  - convert_tools 默认返回 PromptGuided 并由 chat() 将说明文本插入 system prompt（/home/mi/work/open_source/zeroclaw/src/providers/traits.rs:317-327, 374-417）。
  - 文本协议采用 <tool_call>…</tool_call> 包裹 JSON（同文件：534-565）。
- 原生路径（各 Provider 定制）
  - OpenAI：在 chat()/chat_with_tools 中把 ToolSpec 转为 OpenAI tools，并把历史 assistant/tool 消息转为 native 结构，tool_choice=auto（/home/mi/work/open_source/zeroclaw/src/providers/openai.rs:420-428, 488-495, 233-247, 249-321, 323-335）。
    - chat_with_tools 验证工具 JSON（必须 type=function），错误即报（/home/mi/work/open_source/zeroclaw/src/providers/openai.rs:463-485）。
  - Anthropic：convert_tools 将 ToolSpec 转为带 input_schema 的 tools，并对最后一个 tool 加 ephemeral cache_control（/home/mi/work/open_source/zeroclaw/src/providers/anthropic.rs:279-301）。chat_with_tools 接受 OpenAI 形状工具，转回 ToolSpec 复用 chat()（/home/mi/work/open_source/zeroclaw/src/providers/anthropic.rs:456-486, 928-935）。
  - Bedrock：内部使用 Converse 的 toolUse/toolResult；提供 convert_tools_to_converse（/home/mi/work/open_source/zeroclaw/src/providers/bedrock.rs:872-891）并在 convert_messages 中把 assistant.tool_calls 与 tool 结果整理为“助手 toolUse + 用户 toolResult 合并”序列（606-667）。
  - Ollama：原生工具关闭，但可在对话里检测/维持 <tool_call> 或推理字段；若上游提供 tools 列表时桥接到 chat_with_tools（/home/mi/work/open_source/zeroclaw/src/providers/ollama.rs:858-861）。

四、流式接口与 ToolCall 事件（StreamEvent）
- StreamEvent 枚举：TextDelta、ToolCall、PreExecutedToolCall/PreExecutedToolResult、Final（/home/mi/work/open_source/zeroclaw/src/providers/traits.rs:188-200）。默认 stream_chat() 将旧式 StreamChunk 映射为事件（517-531）。
- Anthropic 流式实现
  - 构建启用 stream 的请求体（/home/mi/work/open_source/zeroclaw/src/providers/anthropic.rs:569-575, 1006-1016）。
  - 逐行解析 SSE（message_start/content_block_start/content_block_delta/content_block_stop 等），组装 tool_use id/name/partial_json，发出 StreamEvent::ToolCall（/home/mi/work/open_source/zeroclaw/src/providers/anthropic.rs:585-733，尤其 637-698）。
  - 同时支持 supports_streaming()/supports_streaming_tool_events()=true（/home/mi/work/open_source/zeroclaw/src/providers/anthropic.rs:953-960）。
- 其他 Provider
  - OpenAI 当前未实现流式；Bedrock 文件未见流式实现；Ollama 以自身流格式为主（文件中为大流式解析与“思考内容剥离”，但工具事件以 PromptGuided 逻辑为主）。

五、错误/兼容性处理要点
- 统一错误类型：
  - StreamError（HTTP/JSON/SSE/Provider/IO）（/home/mi/work/open_source/zeroclaw/src/providers/traits.rs:241-257）。
  - ProviderCapabilityError 用于能力缺失的结构化错误（同文件:259-266）。
- 错误净化（敏感信息清洗与截断）
  - sanitize_api_error + api_error 工具（/home/mi/work/open_source/zeroclaw/src/providers/mod.rs:807-831），各 Provider 在非成功状态时调用（如 OpenAI：/home/mi/work/open_source/zeroclaw/src/providers/openai.rs:394-399, 438-441, 505-507）。
- 兼容性/健壮性：
  - Bedrock 在工具结果 JSON 不合法时，仍构造 toolResult（status=error）并通过 last_pending_tool_use_id 兜底匹配（/home/mi/work/open_source/zeroclaw/src/providers/bedrock.rs:620-667, 686-695, 699-730）。
  - Anthropic/Bedrock 将连续多个 tool_result 合并为单条“用户”消息以满足 API 要求（Anthropic：/home/mi/work/open_source/zeroclaw/src/providers/anthropic.rs:396-410；Bedrock：/home/mi/work/open_source/zeroclaw/src/providers/bedrock.rs:652-667）。
  - ReliableProvider 统一处理重试与降级，记录 fallback 信息供上层提示（/home/mi/work/open_source/zeroclaw/src/providers/reliable.rs:18-39, 44-50）。
- 原生/回退分发
  - Agent 在调度时根据 provider.supports_native_tools() 选择 NativeToolDispatcher 或 XmlToolDispatcher（/home/mi/work/open_source/zeroclaw/src/agent/agent.rs:504-508），这与 ProviderCapabilities/覆写方法联动。

六、关键代码引用（节选）
- ProviderCapabilities/ToolsPayload/StreamEvent/回退注入
  - /home/mi/work/open_source/zeroclaw/src/providers/traits.rs:273-286, 294-305, 317-327, 374-417, 188-200
- Anthropic 原生工具与流式 ToolCall
  - /home/mi/work/open_source/zeroclaw/src/providers/anthropic.rs:279-301（convert_tools/input_schema+cache_control）
  - /home/mi/work/open_source/zeroclaw/src/providers/anthropic.rs:833-843（tool_choice any 覆盖）
  - /home/mi/work/open_source/zeroclaw/src/providers/anthropic.rs:585-733（SSE 解析并发出 StreamEvent::ToolCall）
  - /home/mi/work/open_source/zeroclaw/src/providers/anthropic.rs:880-886（capabilities 三项 True）
- OpenAI 原生工具与用量
  - /home/mi/work/open_source/zeroclaw/src/providers/openai.rs:420-428（chat：tools->native）
  - /home/mi/work/open_source/zeroclaw/src/providers/openai.rs:463-485（chat_with_tools：验证工具 JSON）
  - /home/mi/work/open_source/zeroclaw/src/providers/openai.rs:323-335（parse_native_response→ToolCall）
  - /home/mi/work/open_source/zeroclaw/src/providers/openai.rs:442-447（usage 包含 cached_tokens）
  - /home/mi/work/open_source/zeroclaw/src/providers/openai.rs:459-462（supports_native_tools=true）
- Bedrock 工具合并与健壮性
  - /home/mi/work/open_source/zeroclaw/src/providers/bedrock.rs:606-667（assistant toolUse + 合并 toolResult）
  - /home/mi/work/open_source/zeroclaw/src/providers/bedrock.rs:686-695（多字段提取 tool_call_id）
  - /home/mi/work/open_source/zeroclaw/src/providers/bedrock.rs:1051-1056（capabilities）
- PromptGuided 协议文本
  - /home/mi/work/open_source/zeroclaw/src/providers/traits.rs:534-565（<tool_call> 协议说明）
- 可靠性与错误净化
  - /home/mi/work/open_source/zeroclaw/src/providers/reliable.rs:18-50（fallback 记录/作用域）
  - /home/mi/work/open_source/zeroclaw/src/providers/mod.rs:807-831（sanitize_api_error）
