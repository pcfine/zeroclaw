# 10 — 集成（Integrations）总览与实战笔记

本笔记梳理 ZeroClaw 的外部系统集成能力：渠道与模型提供商、HTTP/REST、安全设置、MCP 工具、WebSocket 节点发现、浏览器自动化与硬件接入示例。包含关键代码路径与行号，便于交叉定位。

---

## A. 集成注册表与分类（Registry & Categories）

核心类型定义与 CLI 展示逻辑：

- 集成状态与分类（Active/Available/ComingSoon + 分类标签/全集）
  - 文件：/home/mi/work/open_source/zeroclaw/src/integrations/mod.rs
    - 枚举 IntegrationStatus（Active/Available/ComingSoon）
      - 6-15
    - 枚举 IntegrationCategory 及 label()/all()
      - 18-29, 31-59
    - IntegrationEntry（name/description/category/status_fn）
      - 61-67
- CLI 命令：集成信息展示（大小写不敏感；按状态展示图标与标签；常见集成的“上手提示”）
  - 文件：/home/mi/work/open_source/zeroclaw/src/integrations/mod.rs
    - handle_command → show_integration_info
      - 69-74, 76-180
    - 示例提示（Telegram/Discord/Slack/OpenRouter/Ollama/iMessage/GitHub/Browser/Cron/Weather/Webhooks）
      - 104-176

说明：
- status_fn: fn(&Config) -> IntegrationStatus 以配置决定 Active/Available。
- Info 子命令匹配名称时大小写不敏感（测试覆盖：204-218）。

代码片段：
```rust
// /src/integrations/mod.rs:6-15,18-29,61-67
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum IntegrationStatus { Available, Active, ComingSoon }

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum IntegrationCategory { Chat, AiModel, Productivity, MusicAudio, SmartHome, ToolsAutomation, MediaCreative, Social, Platform }

pub struct IntegrationEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub category: IntegrationCategory,
    pub status_fn: fn(&Config) -> IntegrationStatus,
}
```

---

## B. 集成目录概览（按分类）

注册表完整清单与激活逻辑（基于 Config）：

- 文件：/home/mi/work/open_source/zeroclaw/src/integrations/registry.rs

1) Chat Providers（聊天渠道）
- Telegram（17-22）、Discord（28-35）、Slack（40-47）：
  - 若相应 channels_config.* 配置存在则 Active，否则 Available。
- Webhooks（49-58）：HTTP 触发端点。
- WhatsApp（61-70, Webhook）、Signal（73-82）、iMessage（85-95, macOS 桥接）。
- Matrix（103-113）：homeserver/access_token 等配置决定 Active。
- DingTalk（139-149）、QQ Official（151-161）、Email（716-726）。
- 规划中：Microsoft Teams（97-101）、Nostr（115-119）、WebChat（121-125）、Nextcloud Talk（127-131）、Zalo（133-137）。

2) AI Models（模型提供商）
- 通过 default_provider 或 default_model 前缀判断 Active：
  - OpenRouter（164-173，provider=openrouter 且 api_key 有值时 Active）。
  - Anthropic（176-186，provider=anthropic）。
  - OpenAI（188-198，provider=openai）。
  - Google Gemini（200-213，model 以 google/ 前缀）。
  - DeepSeek（215-228，model 以 deepseek/ 前缀）。
  - xAI（230-243，model 以 x-ai/ 前缀）。
  - Mistral（245-258，model 以 mistral 前缀）。
  - Ollama（260-269，provider=ollama，本地模型）。
  - Perplexity（272-281，provider=perplexity）。
  - Venice（296-306）、Vercel AI Gateway（308-317）、Cloudflare AI（320-329）…
  - 区域/别名映射：Moonshot/Kimi、MiniMax、Qwen、GLM、Z.AI、Qianfan（440-450）等通过 is_*_alias 判定（导入于行 2-5）。
  - 其他：Groq、Together、Fireworks、Novita、Cohere 等。
  - 规划中：Hugging Face、LM Studio（284-294）。

3) Productivity（生产力）
- Google Workspace（Drive/Gmail/Calendar/Sheets/Docs），由 google_workspace.enabled 判定 Active（511-523）。
- GitHub/Notion/Apple Notes/Reminders/Obsidian/Things 3/Bear/Trello/Linear 规划中（524-577）。

4) Music & Audio：Spotify/Sonos/Shazam（579-596，规划中）

5) Smart Home：Home Assistant/Philips Hue/8Sleep（598-616，规划中）

6) Tools & Automation
- Browser（618-627）：由 browser.enabled 判定 Active。
- Shell / File System：始终 Active（630-640）。
- Cron：由 cron.enabled 判定（642-652）。
- Weather：始终 Active（672-676）。
- 其他（Voice/Gmail/1Password/Canvas）规划中。

7) Social：Twitter/X（规划中）、Email（见上）

8) Platforms：macOS/Linux Active（平台条件编译判断），Windows/iOS/Android Available（728-769）。

测试覆盖（确保完整性/不崩溃/类别覆盖/状态合理等）：773-1096。

---

## C. Providers & 模型/嵌入路由

参考文档（ID/别名/环境变量映射、可靠性回退、模型路由/嵌入路由）：
- /home/mi/work/open_source/zeroclaw/docs/reference/api/providers-reference.md

要点摘录：
- 凭据解析顺序（15-23）：
  1) 配置/CLI 明确提供
  2) Provider 专属 env
  3) 通用 env：ZEROCLAW_API_KEY → API_KEY
- Fallback Provider 链（25-74）：
  - 在超时/503/429（含 API Key 轮换后）/模型不可用时按顺序回退
  - 每个回退独立解析凭据，可跨 API 家族（OpenAI → Anthropic → 本地 Ollama）
  - 支持 per-model fallback（91-105）
- 样例配置（36-41, 55-67）：
```toml
[reliability]
fallback_providers = ["anthropic", "groq", "openrouter"]
provider_retries = 2
provider_backoff_ms = 500

[reliability.model_fallbacks]
"gpt-4o" = ["gpt-4-turbo"]
"claude-opus-4-20250514" = ["claude-sonnet-4-20250514"]
```
- API Key 轮换（75-89）
- 模型路由（hint:<name>）与嵌入路由（322-364, 344-380）：
  - [[model_routes]] / [[embedding_routes]] 按 hint 选择 provider/model，支持每路由自定义 api_key。
- 自定义端点：
  - OpenAI 兼容：default_provider="custom:<url>"（261-267）
  - Anthropic 兼容：default_provider="anthropic-custom:<url>"（269-273）
- Provider 备注（150+）：
  - Vercel AI Gateway（150-157）：确保使用 https://ai-gateway.vercel.sh/v1
  - Gemini（158-165）：支持 OAuth 与 API Key；thinking 模型自动过滤 reasoning 块
  - Ollama Vision（166-172）：[IMAGE:<source>] 走原生 images 字段；非多模态会报能力错误
  - Ollama Cloud（173-180）：仅远端 api_url 可用 :cloud；本地发现排除 :cloud
  - llama.cpp / SGLang / vLLM / Osaurus：默认本地端口，API Key 可选（182-213, 188-195, 196-202, 203-213）
  - Bedrock（214-224）：Converse API，AK/SK，默认区域 us-east-1，支持工具调用与 prompt 缓存
  - NVIDIA NIM（247-260）：base API/推荐模型
- 升级模型安全策略（382-395）：以 hint 稳定引用，更新路由目标并 smoke test。

---

## D. 自定义 Provider & 本地部署（OpenAI/Anthropic 兼容 + 本地框架）

- 文档：/home/mi/work/open_source/zeroclaw/docs/contributing/custom-providers.md
  - OpenAI 兼容（custom:）/ Anthropic 兼容（anthropic-custom:）配置样例（11-25, 31-37, 39-47）
  - 本地服务：
    - llama.cpp（49-79）：llama-server 启动 → default_provider="llamacpp" → api_url 指向本地 /v1
    - SGLang（81-111）：python 启动 → default_provider="sglang"
    - vLLM（112-142）：vllm serve → default_provider="vllm"
  - 验证与排错（143-205）

---

## E. REST/HTTP 外部系统集成工具（http_request 工具）

- 文件：/home/mi/work/open_source/zeroclaw/src/tools/http_request.rs

功能与安全：
- 支持方法：GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS（69-82；校验不通过返回明确错误）
- 参数模式（Tool.parameters_schema）：url、method、headers、body（177-201）
- 强安全防护：
  - 仅 http:// 与 https://（46-48）
  - 必须配置 allowlist：allowed_domains（50-55；normalize 去重：311-319）
  - 默认禁止私有/本地地址（58-64；包含 localhost/.localhost/.local、RFC1918、Multicast、Broadcast、保留、文档网段、IPv6 私有/链路本地等 — 401-454）
  - 明确拒绝 IPv6 host 文字与 URL userinfo（369-371, 365-367）
  - 禁止自动重定向（115-146, 129-133）
- 超时与大小：
  - 超时：timeout_secs（0→警告并使用 30s 安全默认；122-131）
  - 响应体截断：max_response_size（0 表示不截断；148-163）
- 请求构造：
  - headers 传递与展示时敏感头自动脱敏（Authorization/api-key/token/secret；96-113）
  - 响应头对 Set-Cookie 脱敏（262-273）
- 行为控制：
  - autonomy/rate limit：SecurityPolicy.can_act()/record_action()（214-228）
- 返回格式：
  - "Status: <code> <reason>\nResponse Headers: ...\n\nResponse Body:\n<text>"（282-289）

常见坑：
- 未配置 [http_request].allowed_domains 会被拒绝（50-55）
- IPv6 地址/本地地址默认拒绝（369-371, 413-421）
- 即便 allowed_domains="*"，私有地址仍默认拒绝；需 allow_private_hosts=true 才允许（971-1038）
- URL 不可有空格/非法 scheme/userinfo（35-48, 564-581, 943-951, 960-967）

---

## F. MCP 工具与外部工具服务器（Model Context Protocol）

配置结构（config.schema）：
- 文件：/home/mi/work/open_source/zeroclaw/src/config/schema.rs
  - 传输枚举 McpTransport（Stdio/Http/Sse；默认 Stdio）
    - 895-906, 898-905
  - 单服务器配置 McpServerConfig（name/transport/url/command/args/env/headers/tool_timeout_secs）
    - 908-934
  - 总配置 McpConfig（enabled/deferred_loading/servers）
    - 936-951, 957-965
  - 注意：deferred_loading 默认 true（953-955, 960-964）→ 工具 schema 延迟加载以节省上下文

传输实现与约束（工具层）：
- 文件：/home/mi/work/open_source/zeroclaw/src/tools/mcp_transport.rs
  - 常量与头部：
    - MAX_LINE_BYTES=4MB（15-17）
    - RECV_TIMEOUT_SECS=30（18-20）
    - 接受类型 MCP_STREAMABLE_ACCEPT（21-23）
    - MCP_JSON_CONTENT_TYPE / MCP_SESSION_ID_HEADER（24-27）
  - StdioTransport：
    - 以 command+args/env 启动本地进程（50-61）
    - 流式读写 stdin/stdout，超时与大小限制（79-103, 118-139）
  - HTTP Transport：
    - 基于 reqwest POST，120s 超时，携带 headers（157-176）
    - 会维护 MCP-Session-Id 会话头（178-195）
  - SSE Transport（文件后续实现，预览略）

要点：
- JSON-RPC 2.0 消息类型在 mcp_protocol.rs 定义（/home/mi/work/open_source/zeroclaw/src/tools/mcp_protocol.rs:1-73 等）；
- 工具列表载入（tools/list）支持延迟拉取 schema（与 deferred_loading 配合）。

---

## G. 动态节点发现（/ws/nodes WebSocket）

- 配置结构：/home/mi/work/open_source/zeroclaw/src/config/schema.rs
  - NodesConfig（enabled/max_nodes/auth_token）
    - 996-1011, 1013-1025
- 行为：
  - 启用后，外部进程/设备可通过 WebSocket /ws/nodes 连接并在运行期上报能力（说明见 996-1000 注释）
  - 支持 Bearer Token 鉴权（1008-1010）
  - 默认最大并发 16（1013-1015）

---

## H. 浏览器自动化（Headless/GUI）

- 文档：/home/mi/work/open_source/zeroclaw/docs/browser-setup.md

关键配置：
```toml
# /docs/browser-setup.md:33-39
[browser]
enabled = true
allowed_domains = ["*"]       # 可改为精确域名列表
backend = "agent_browser"
native_headless = true
```

快速上手（15-26, 50-54）：
- 安装 agent-browser 与 Chrome for Testing
- 测试：echo "Open https://example.com ..." | zeroclaw agent

GUI 访问（VNC/noVNC/CRD）：56-135
测试用例（CLI 与 ZeroClaw 集成）：137-169
安全注意（204-210）：限制 allowed_domains，VNC 端口需防火墙或内网（如 Tailscale）

---

## I. 硬件接入示例：Aardvark（USB I2C/SPI/GPIO 适配器）

- 文档：/home/mi/work/open_source/zeroclaw/docs/aardvark-integration.md

分层设计：
1) crates/aardvark-sys（C SDK 绑定，仅此层触达 C 库）：
   - find_devices/open_port/i2c_scan/i2c_read/i2c_write/spi_transfer/gpio_set/gpio_get/Drop（42-78）
   - Stub 模式：无 SDK 返回空/错误但不崩溃（80）
2) AardvarkTransport（桥接 ZeroClaw 命令协议 ZcCommand ↔ aardvark-sys）：
   - 文件：src/hardware/aardvark.rs（文档说明 84-131）
   - 逐命令映射：i2c_scan/i2c_read/i2c_write/spi_transfer/gpio_set/gpio_get（99-127）
   - 惰性打开/自动关闭句柄
3) Tools（i2c_scan/i2c_read/i2c_write/spi_transfer/gpio_aardvark/datasheet）：
   - 文件：src/hardware/aardvark_tools.rs（138-180）
   - 仅在探测到硬件后注册
4) Device Registry（设备注册/别名/能力/transport）：
   - 文件：src/hardware/device.rs（184-208）
5) 启动流程 boot()：探测并注册设备与工具（212-233）
6) Tool Registry：按有无硬件选择性加载工具（237-254）

该示例说明了 ZeroClaw 如何将外部设备抽象为工具，按需加载并在 Agent 回路中消费。

---

## J. 常见陷阱 & 安全基线

- HTTP 工具：
  - 未设置 [http_request].allowed_domains → 直接拒绝（/src/tools/http_request.rs:50-55）
  - URL 含空格/非法 scheme/userinfo → 拒绝（35-48, 564-581, 943-951, 960-967）
  - 默认拒绝本地/私网/IPv6 本地地址（58-64, 401-454）；即便 allowed_domains="*" 也会挡（524-531）
  - 需要访问私网 → 显式 allow_private_hosts=true 且 allowlist 匹配（994-1026）
  - 禁止重定向：若目标依赖 30x，需要自行处理（129-133）
- Provider：
  - 端点必须带 http(s)://，有些网关需 /v1（custom/anthropic-custom；/docs/contributing/custom-providers.md 与 providers-reference.md）
  - Vercel AI Gateway 要用 https://ai-gateway.vercel.sh/v1（providers-reference.md:150-157）
  - :cloud 模型仅在远端 Ollama；本地模式会校验失败（173-180）
  - 模型 ID 不匹配/变更：建议使用 hint 路由稳定引用（providers-reference.md:382-395）
- MCP：
  - Stdio 传输需可执行命令；HTTP/SSE 需 URL 与必要 headers（/src/config/schema.rs:908-934）
  - 响应过大或超时：4MB/30s 限制（/src/tools/mcp_transport.rs:15-23, 18-20）

---

## K. 典型配置与部署配方（Recipes）

- 列出 provider：
  - zeroclaw providers（文档：/docs/reference/api/providers-reference.md:7-11）
- 多云高可用（fallback 链 + 模型回退）：
```toml
# /docs/reference/api/providers-reference.md:55-67
default_provider = "openai"
default_model = "gpt-4o"

[reliability]
fallback_providers = ["anthropic", "ollama"]

[reliability.model_fallbacks]
"gpt-4o" = ["gpt-4-turbo"]
"claude-opus-4-20250514" = ["claude-sonnet-4-20250514"]
```
- 自定义端点（OpenAI/Anthropic 兼容）：
```toml
# /docs/contributing/custom-providers.md:31-37
api_key = "your-api-key"
default_provider = "anthropic-custom:https://api.example.com"
default_model = "claude-sonnet-4-6"
```
- 本地框架启动（llama.cpp/SGLang/vLLM）：见 /docs/contributing/custom-providers.md（49-142）
- 浏览器自动化（headless/GUI/VNC/CRD）：见 /docs/browser-setup.md

---

## L. 文档与代码索引（绝对路径 + 行号）

- 注册与展示
  - /home/mi/work/open_source/zeroclaw/src/integrations/mod.rs（状态/分类/CLI 展示/提示）：6-15, 18-59, 61-67, 69-180
  - /home/mi/work/open_source/zeroclaw/src/integrations/registry.rs（清单与激活逻辑 + 测试）：11-769, 773-1096
- HTTP 工具（REST 集成）
  - /home/mi/work/open_source/zeroclaw/src/tools/http_request.rs（安全/参数/执行/SSRF 测试）：8-33, 35-67, 69-82, 96-113, 115-146, 148-163, 166-306, 311-454, 456-1038
- MCP
  - 配置：/home/mi/work/open_source/zeroclaw/src/config/schema.rs（MCP/NODES）：895-951, 957-965, 996-1011, 1013-1025
  - 传输实现：/home/mi/work/open_source/zeroclaw/src/tools/mcp_transport.rs（常量、Stdio/HTTP、会话头）：1-28, 31-39, 43-76, 79-103, 106-145, 149-176, 178-195
  - 协议类型：/home/mi/work/open_source/zeroclaw/src/tools/mcp_protocol.rs：1-73（JSON-RPC 结构/常量）
- Providers 参考与路线
  - /home/mi/work/open_source/zeroclaw/docs/reference/api/providers-reference.md（凭据解析、fallback、路由、各 Provider 备注）
  - /home/mi/work/open_source/zeroclaw/docs/getting-started/multi-model-setup.md（多模型与路由实践）
  - /home/mi/work/open_source/zeroclaw/docs/contributing/custom-providers.md（自定义端点与本地框架）
- 浏览器自动化
  - /home/mi/work/open_source/zeroclaw/docs/browser-setup.md（headless/GUI/VNC/CRD）
- 硬件接入示例
  - /home/mi/work/open_source/zeroclaw/docs/aardvark-integration.md（分层/工具/启动流程）
- 示例
  - /home/mi/work/open_source/zeroclaw/docs/examples/agent-llm-output-analysis.md
  - /home/mi/work/open_source/zeroclaw/docs/examples/agent-llm-log-demo.md

---

## 附：常用命令速查

- 集成信息：zeroclaw integrations info <Name>（大小写不敏感；/src/integrations/mod.rs）
- Provider 管理/刷新本地模型清单：
  - zeroclaw providers（providers-reference.md）
  - zeroclaw models refresh --provider <id>（llamacpp/sglang/vllm/osaurus 等）
- 入门与上手：
  - zeroclaw onboard（渠道/提供商引导）
  - zeroclaw onboard --channels-only（Telegram/Discord/Slack 等）
- 浏览器/网关/计划任务：
  - zeroclaw agent（与默认模型对话）
  - zeroclaw gateway（启动 Webhooks/Gateway）
  - zeroclaw cron list（列出计划任务）

本笔记覆盖了核心集成面与落地细节。建议按“代码索引 → 文档说明 → 配置落地 → 集成测试”的节奏推进，并在生产场景下严格设置 allowed_domains、禁用不必要的本地网络访问、启用 provider 回退与 key 轮换，确保高可用与安全基线。