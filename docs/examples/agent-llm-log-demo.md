# Agent 与 LLM 交互日志示例

本示例演示如何编译、运行「从网上下载 README 并保存到工作区」的任务，并**将与 LLM 的交互日志打印到 stderr**；同时 agent 会将**每次交互的标注日志追加到文件**，便于事后查看。

## 0. 交互日志文件（标注每次交互做了哪些事情）

每次以**单条消息**模式运行 agent（`zeroclaw agent -m "..."`）时，会在工作区下生成/追加：

- **路径**：`<workspace_dir>/state/agent-interaction.log`  
  默认工作区为 `~/.zeroclaw/workspace`，即日志文件为 `~/.zeroclaw/workspace/state/agent-interaction.log`。

**每行含义**：

| 行格式 | 含义 |
|--------|------|
| `=== Agent 与 LLM 交互日志 === ... (provider=..., model=...)` | 本次对话开始，时间戳与所用 provider/model |
| `[请求] 第 N 轮 - 向大模型发送请求，消息数: M` | 第 N 轮向大模型发送请求，当前历史消息条数为 M |
| `[大模型输出] 第 N 轮 - 请求调用工具: <工具名>，参数摘要: ...` | 大模型本轮要求调用的工具及参数摘要（敏感信息已脱敏） |
| `[工具结果] <工具名> 执行完成，成功: true/false，输出长度: X 字符，耗时: Y ms` | 该工具执行完毕后的结果摘要 |
| `[最终回复] 第 N 轮 - 大模型返回最终文本（无工具调用），回复长度: X 字符` | 大模型不再调工具，直接返回最终文本 |
| `  └ 回复内容（共 X 字）:` 及后续 `  │ ...` 行 | 当轮大模型回复的完整字符串（脱敏、过长会截断） |
| `  └ 本轮大模型附带文本（共 X 字）:` 及后续 `  │ ...` 行 | 大模型在返回工具调用的同时产出的附带文本（若有） |
| `--- 第 N 轮 请求（发往大模型）原始 JSON ---` 及后续 `  │ ...` 行 | 当轮发往大模型的**原始请求**（model、temperature、messages 等），JSON 格式，脱敏后写入，超长会截断 |
| `--- 第 N 轮 响应（大模型返回）原始 JSON ---` 及后续 `  │ ...` 行 | 当轮大模型返回的**原始响应**（text、tool_calls、usage、reasoning_content 等），JSON 格式，脱敏后写入，超长会截断 |

同一会话内多轮会按顺序追加；多次运行 agent 会在同一文件中按时间顺序追加（每次以 `===` 开头的新段为一次新对话）。

## 1. 编译

```bash
cd /path/to/zeroclaw
cargo build --release
```

## 2. 配置（首次运行需要）

若尚未配置，请先完成引导并设置 API Key（任选其一）：

```bash
# 方式 A：交互式引导
zeroclaw onboard --interactive

# 方式 B：快速指定（示例为 OpenRouter）
zeroclaw onboard --api-key "sk-..." --provider openrouter --model "anthropic/claude-sonnet-4.6"
```

或仅通过环境变量指定 Key（不修改 config）：

```bash
export OPENROUTER_API_KEY="sk-..."   # 使用 OpenRouter 时
# 或
export API_KEY="sk-..."              # 通用覆盖
```

## 3. 运行示例并查看与 LLM 的交互日志

使用 **`RUST_LOG=info`** 将 agent 与 LLM 的交互打到 stderr：

```bash
RUST_LOG=info ./target/release/zeroclaw agent -m "从网上下载 https://raw.githubusercontent.com/zeroclaw-labs/zeroclaw/main/README.md 的内容，保存到工作区的 download/readme.md"
```

如需更详细日志（含依赖库）：

```bash
RUST_LOG=zeroclaw=info,info ./target/release/zeroclaw agent -m "从网上下载 https://raw.githubusercontent.com/zeroclaw-labs/zeroclaw/main/README.md 的内容，保存到工作区的 download/readme.md"
```

## 4. 预期日志含义

在 `RUST_LOG=info` 下，你会看到类似下面的与 LLM 交流的日志（时间戳略）：

| 日志行 | 含义 |
|--------|------|
| `agent: sending to LLM iteration=1 messages_count=2` | 第 1 轮：向 LLM 发送 2 条消息（system + user） |
| `agent: LLM requested tool call iteration=1 tool=web_fetch arguments={"url":"https://..."}` | LLM 返回要调用 `web_fetch`，参数为 url |
| `agent: tool execution completed tool=web_fetch success=true output_len=12345 duration_ms=...` | 工具 `web_fetch` 执行完成，成功，输出长度 12345 |
| `agent: sending to LLM iteration=2 messages_count=5` | 第 2 轮：把工具结果追加后，共 5 条消息再发给 LLM |
| `agent: LLM requested tool call iteration=2 tool=file_write arguments={"path":"download/readme.md",...}` | LLM 返回要调用 `file_write` |
| `agent: tool execution completed tool=file_write success=true output_len=... duration_ms=...` | 工具 `file_write` 执行完成 |
| `agent: sending to LLM iteration=3 messages_count=7` | 第 3 轮 |
| `agent: LLM final response (no tool calls) response_len=...` | LLM 不再调工具，返回最终文本 |
| `agent: response excerpt ...` | 最终回复内容摘要（前 400 字符） |

最终答案会打印到 **stdout**；上述与 LLM 的交互过程在 **stderr**。

## 5. 仅看 agent 相关日志（过滤）

若只想看 agent 与 LLM 的交互，可过滤：

```bash
RUST_LOG=info ./target/release/zeroclaw agent -m "..." 2>&1 | grep -E "agent:|tool=|iteration="
```

或只看 stderr 的 info：

```bash
RUST_LOG=info ./target/release/zeroclaw agent -m "..." 2>&1 >/dev/null
```

## 6. 无 API Key 时的行为

若未设置 `OPENROUTER_API_KEY`（或当前 provider 的 API key），会看到：

- `agent: sending to LLM iteration=1 messages_count=2`（说明日志已生效）
- 随后报错：`OpenRouter API key not set. Run zeroclaw onboard or set OPENROUTER_API_KEY env var.`

补全 API Key 后重新执行上述命令即可完成下载并保存，并看到完整多轮 tool call 日志。
