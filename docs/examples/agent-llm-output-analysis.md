# Agent 示例运行与 LLM 输出分析

本文档基于「先用 web_fetch 抓取 README URL，再用 file_write 写入 download/readme.md」示例的一次完整运行，分析 Mify 网关下 mimo-v2-flash 大模型的输出行为。

## 1. 运行环境与配置

- **模型**：xiaomi/mimo-v2-flash（Mify 网关）
- **用户指令**：先用 web_fetch 抓取 https://raw.githubusercontent.com/zeroclaw/zeroclaw/main/README.md 的内容，再用 file_write 写入工作区 download/readme.md
- **审批**：supervised，部分工具需人工 Y/N/A；本次运行通过管道输入多个 `Y` 做自动确认
- **结果**：在约 10 轮后触发「超过最大工具调用次数」而结束，任务未完成

## 2. 大模型输出行为概览

| 轮次 | 模型自然语言摘要 | 请求的工具 | 工具参数要点 | 执行结果 |
|------|------------------|------------|--------------|----------|
| 1 | 说「web_fetch 不在当前工具集中」，改用 shell | shell | `curl -s ... -o /tmp/readme.md` | 未执行/失败 |
| 2 | 尝试「更安全」的方式，先建目录 | shell | `"approved":true, "command":"mkdir -p download"` | 未执行/失败 |
| 3 | 先查看工作区内容 | shell | `"approved":true, "command":"mkdir -p download"` → 后为 ls -la | success |
| 4 | 用 Python urlretrieve 下载 | shell | python3 -c "urllib.request.urlretrieve(...)" | 未执行/失败 |
| 5 | 尝试 wget | shell | wget -q ... -O download/readme.md | 未执行/失败 |
| 6 | 称直接下载被拦截，改用 git | git_operations | operation: status | 未执行/失败 |
| 7 | 检查 curl/wget/python 是否可用 | shell | which curl wget python3 python | 未执行/失败 |
| 8 | 称文件操作也受限，写脚本再执行 | file_write | 写入 download/fetch_readme.py | 未执行/失败 |
| 9 | 用「最简单」的 shell 测试 | shell | echo "Testing basic shell access" | 未执行/失败 |
| 10 | 将需求记入 memory，待后续处理 | memory_store | 存 pending_download_request | 未执行/失败 → 达最大轮次 |

## 3. 关键发现

### 3.1 为何模型说「web_fetch 不可用」

- ZeroClaw 中 **web_fetch 默认关闭**（`config.web_fetch.enabled` 默认为 `false`）。
- 只有在配置里显式开启 `[web_fetch] enabled = true` 时，`web_fetch` 才会被加入 `tools_registry` 并出现在 system 的「Available Tools」里。
- 因此模型看到的工具列表里本来就没有 web_fetch，其输出「web_fetch 不在当前工具集中」与实现一致，并非幻觉。

### 3.2 工具选择与指令遵循

- 用户明确要求：**先用 web_fetch，再用 file_write**。
- 因 web_fetch 未出现在工具列表，模型未使用 web_fetch/file_write，而是反复尝试 **shell**（curl、wget、python3）、**git_operations**、**file_write**（写脚本）、**memory_store**。
- 若希望模型按「web_fetch → file_write」执行，需在配置中开启 web_fetch，并在 system 中提供对应工具说明。

### 3.3 输出格式与协议符合度

- 模型始终通过 **标准 tool_calls**（name + arguments JSON）发起调用，未出现纯自然语言描述工具、无标签 JSON 等；与 ZeroClaw 的 `<tool_call>...</tool_call>` 解析侧一致（若 provider 返回的是结构化 tool_calls，agent 会按此解析）。
- 自然语言与工具调用混合：每轮先有一段说明再跟 tool_calls，可读性好，也便于调试。

### 3.4 参数错误与多余字段

- 多次出现 **shell** 调用带 **`"approved": true`**（如第 2、3、4、5、9 轮）。
- `approved` 是 ZeroClaw 审批交互的 UX 字段，**不是** shell 工具的 schema 字段；shell 只接受如 `command`。
- 模型可能把界面上看到的「Approved」当成了工具参数，导致参数不符合 schema，执行层可能拒绝或忽略。

### 3.5 执行结果与策略

- 多数工具调用为 **success=false** 或未真正执行：部分因审批/管道输入时机，部分因参数错误或安全策略（如禁止写 `/tmp`、限制 shell 命令等）。
- 唯一明确 **success=true** 的是一次 **shell**（`ls -la`），说明在「允许执行」的前提下，模型能正确发起并完成一次简单 shell 调用。
- 模型在多次失败后转向「写脚本」「记 memory」等策略，属于合理的补救尝试，但因轮次上限而未能完成任务。

## 4. 建议

1. **若要按示例使用 web_fetch + file_write**  
   在 `config.toml` 中启用 web_fetch，例如：
   ```toml
   [web_fetch]
   enabled = true
   # 按需配置 allowed_domains、max_response_size、timeout_secs
   ```
   并确保 allowed_domains 包含 `raw.githubusercontent.com`（或使用 `*` 等开放策略）。

2. **减少「approved」被误当工具参数**  
   - 在 system 或工具描述中明确说明：工具参数仅包含文档中列出的字段，不要包含审批相关字段。  
   - 或在 agent 侧在把消息交给模型前，从 tool call 的 arguments 中剥离非 schema 字段（如 `approved`），避免执行层收到多余键。

3. **提高任务完成率**  
   - 对「下载 + 写文件」类示例，可临时将 `web_fetch`、`file_write` 加入 `auto_approve`，或提高 `max_tool_iterations`，以便在多轮工具调用下跑通完整流程。  
   - 或在 prompt 中强调「优先使用 web_fetch 和 file_write，不要用 shell 下载」，在开启 web_fetch 后更易被遵守。

## 5. 小结

- 本次运行中，大模型输出**格式正确**（tool_calls 结构清晰），**工具选择**受当前配置约束（无 web_fetch）而合理偏到 shell 等替代方案。
- 主要问题集中在：**web_fetch 默认未开启**、**shell 参数中混入 `approved`**、**审批/安全策略与轮次上限**导致多数调用未成功，任务未在 10 轮内完成。
- 按上述建议开启 web_fetch、规范参数并适当放宽审批或轮次后，可复现「web_fetch 抓取 → file_write 写入」的完整示例并再次观察模型输出。
