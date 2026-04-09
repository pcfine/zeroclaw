# 08｜Skills / Skillforge / SOP 笔记

一、概述
- Skills 子系统为 ZeroClaw 提供用户/社区可扩展的“技能”，每个技能可包含元信息、说明文本、以及可调用的工具（shell/script/http）。核心职责：
  - 在工作区加载技能清单（SKILL.md / SKILL.toml）并合并可选的 open-skills 仓库。
  - 将技能内声明的 [[tools]] 转换为运行时可调用的 Tool 规范（函数调用），并在系统提示中注入技能信息（Full/Compact 两种模式）。
  - 提供安全审计、安装/卸载、测试（TEST.sh 驱动）等运维 SOP。
  - 可选的 Skillforge（feature="skill-creation"）支持自动创建与改进技能（受配置控制与冷却期约束）。
- 全局安全：shell/script 工具通过 SecurityPolicy 强制校验与速率控制；http 工具仅允许 http/https 且响应体/超时受限；安装/审计流程严格校验路径、大小、脚本文件与危险命令模式。

二、核心类型与 API
- 技能与工具模型
  - Skill: name/description/version/author/tags/tools/prompts/location
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:34-48
  - SkillTool: name/description/kind("shell"|"http"|"script")/command/args
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:51-61
- 技能提示构建（系统提示注入）
  - skills_to_prompt(skills, workspace_dir) → Full 模式包装器
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:751-758
  - skills_to_prompt_with_mode(skills, workspace_dir, mode) → Full/Compact 两种注入模式
    - Full：内联完整 instructions 与工具元数据
    - Compact：仅内联简要信息与可调用工具清单，完整说明按需 read_skill(name) 加载
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:760-864
- 技能工具转换（Skill -> Tool）
  - skills_to_tools(skills, security) → Vec<Box<dyn Tool>>
    - shell/script → SkillShellTool，http → SkillHttpTool；未知 kind 跳过并告警
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:866-905
- 工具注册入口（工具子系统）
  - register_skill_tools(registry, skills, security)
    - 将技能工具加入全局工具注册表，若重名于内建工具则跳过并告警
    - /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:308-333
- 运行时集成（代理）
  - System Prompt 拼装：调用 skills_to_prompt_with_mode 注入技能
    - /home/mi/work/open_source/zeroclaw/src/agent/prompt.rs:222-226
  - 注册技能工具与 Compact 模式下 read_skill 描述
    - 注册技能工具：/home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3696-3699, 4620-4624
    - Compact 模式追加 read_skill 工具描述：/home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3726-3734, 4638-4645
- 配置与模式
  - SkillsPromptInjectionMode { Full, Compact }
    - /home/mi/work/open_source/zeroclaw/src/config/schema.rs:1518-1526
  - SkillsConfig（open_skills_enabled/open_skills_dir/allow_scripts/prompt_injection_mode 等）
    - /home/mi/work/open_source/zeroclaw/src/config/schema.rs:1536-1561

三、技能加载与发现
- 加载 API
  - load_skills(workspace_dir) → 默认（无 open-skills 覆盖）
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:125-128
  - load_skills_with_config(workspace_dir, config) → 从运行时配置读取 open-skills/allow_scripts
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:130-138
  - load_skills_with_open_skills_settings(..) → 显式开关/路径
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:140-152
  - load_skills_with_open_skills_config(..) → 带 open-skills/allow_scripts 入口（内部）
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:154-158
- 路径与初始化
  - skills_dir(workspace_dir)
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:907-910
  - init_skills_dir(workspace_dir) → 初始化 README 与示例
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:912-940

四、工具注册与调用
- Shell/Script 工具（SkillShellTool）
  - 结构与构造：tool_name/description/command_template/args/security
    - /home/mi/work/open_source/zeroclaw/src/tools/skill_tool.rs:20-27, 34-46
  - 参数 schema 构建与模板替换
    - build_parameters_schema: /home/mi/work/open_source/zeroclaw/src/tools/skill_tool.rs:48-68
    - substitute_args: /home/mi/work/open_source/zeroclaw/src/tools/skill_tool.rs:70-82
  - 执行逻辑 execute(args)（关键安全点）
    - 速率限制与 budget：is_rate_limited()/record_action()
    - 执行审批 validate_command_execution(.., approved=true)
    - forbidden_path_argument 阻断路径
    - 超时与输出截断（60s/1MB）
    - /home/mi/work/open_source/zeroclaw/src/tools/skill_tool.rs:99-202
- HTTP 工具（SkillHttpTool）
  - 结构与构造：tool_name/description/url_template/args
    - /home/mi/work/open_source/zeroclaw/src/tools/skill_http.rs:17-23, 25-37
  - 参数 schema 与 URL 参数替换
    - build_parameters_schema: /home/mi/work/open_source/zeroclaw/src/tools/skill_http.rs:39-59
    - substitute_args: /home/mi/work/open_source/zeroclaw/src/tools/skill_http.rs:61-73
  - 执行逻辑 execute(args)（关键安全点）
    - 仅允许 http/https；30s 超时；响应体 1MB 截断（多字节边界对齐）
    - 成功以 HTTP 状态码判定；失败返回 “HTTP <status>” 或错误信息
    - /home/mi/work/open_source/zeroclaw/src/tools/skill_http.rs:76-153
- Compact 模式下的技能全文读取辅助（read_skill 工具）
  - 结构与接口
    - 工具名 "read_skill"；参数 { name: string }；适用于 Compact 模式按需读取技能源文件
    - /home/mi/work/open_source/zeroclaw/src/tools/read_skill.rs:6-24, 27-48
  - 执行逻辑 execute(args)
    - 加载技能列表，按不区分大小写匹配 name；找不到时返回已安装技能名列表；无 location 或 IO 错误时返回错误信息
    - /home/mi/work/open_source/zeroclaw/src/tools/read_skill.rs:50-113

五、系统提示注入模式（Full vs Compact）
- Full 模式
  - 注入完整 <available_skills>：包括每个技能的 <instructions>（prompts）与工具元数据
  - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:772-786, 799-810, 812-857
- Compact 模式
  - 注入简要技能信息与 <callable_tools>（可调用工具名采用 技能名前缀.skill.tool 形式），不内联 prompts；需要时调用 read_skill(name) 获取全文
  - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:779-786, 800-806, 812-857
  - 代理在 Compact 模式追加 read_skill 工具描述，提示用法
    - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3726-3734, 4638-4645
- 构建入口（Prompt 构建器）
  - /home/mi/work/open_source/zeroclaw/src/agent/prompt.rs:222-226

六、安全审计与安装 SOP
- 技能审计（静态）
  - 选项与报告：SkillAuditOptions/SkillAuditReport(is_clean/summary)
    - /home/mi/work/open_source/zeroclaw/src/skills/audit.rs:9-12, 14-28
  - 入口：audit_skill_directory_with_options(skill_dir, options)
    - 校验根目录存在、清单存在（SKILL.md/SKILL.toml），深度优先遍历，逐路径审计
    - /home/mi/work/open_source/zeroclaw/src/skills/audit.rs:30-66, 92-118
  - 每路径审计 audit_path
    - 禁止 symlink；脚本文件在 allow_scripts=false 时阻断；大文本上限；分派到 Markdown/TOML 校验
    - /home/mi/work/open_source/zeroclaw/src/skills/audit.rs:120-161
  - Markdown 校验
    - 检测高风险命令片段；遍历链接目标，校验 scheme/绝对路径/脚本后缀/越界等
    - /home/mi/work/open_source/zeroclaw/src/skills/audit.rs:163-179, 245-300
  - TOML 清单校验
    - [[tools]].command shell 链接符（如 &&, ||, |）阻断；高风险模式告警；缺失/空命令拦截；prompts 同样做高风险检测
    - /home/mi/work/open_source/zeroclaw/src/skills/audit.rs:181-243, 195-227, 230-241
- 安装来源与流程
  - ClawHub 源识别与下载 URL 构造
    - is_clawhub_source/parse_clawhub_url/clawhub_download_url
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:970-975, 956-975, 977-?（下载 URL 逻辑在 977-996、990-996 等片段），以及 1980-1996 测试用例引用
  - Git 源识别
    - is_git_source（显式排除 ClawHub）
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:1047-1051
  - ClawHub 安装（zip 解压 + 审计）
    - 路径安全（拒绝 ..、绝对路径、反斜杠、冒号），大小上限，必要时补写 SKILL.toml，再执行 enforce_skill_security_audit，失败回滚删除
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:1260-1354
- CLI 命令处理（List/Audit/Install/Remove/Test）
  - handle_command(...)
    - List：列出技能/工具/标签
    - Audit：对指定源或已安装技能目录执行审计
    - Install：分发至 ClawHub/Git/本地安装，安装后打印扫描文件数
    - Remove：防路径穿越（".."、"/"、"\"），canonicalize 验证在 skills 目录内
    - Test：单个或全部技能，根据 TEST.sh 执行（见下一节）
    - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:1356-1534

七、运行时执行与 SOP/Playbook 流程
- 代理生命周期（简化）：
  - 启动时加载技能（load_skills_with_config）
  - 将技能工具注册到工具注册表（register_skill_tools）
  - 构建系统提示，选择 Full/Compact 注入模式
  - Compact 模式下提供 read_skill 以便按需加载完整说明
  - /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3696-3699, 3726-3734
- 工具调用路径：
  - shell/script → SkillShellTool.execute：先安全策略校验/速率限制，然后在受控环境执行 sh -c 命令，超时与输出截断
    - /home/mi/work/open_source/zeroclaw/src/tools/skill_tool.rs:99-202
  - http → SkillHttpTool.execute：校验 scheme、超时、响应体限制与截断
    - /home/mi/work/open_source/zeroclaw/src/tools/skill_http.rs:90-153

八、Skillforge：自动创建与改进（feature="skill-creation"）
- 自动创建（SkillCreator）
  - create_from_execution(task_description, tool_calls, embedding_provider) → 根据多步执行记录生成 SKILL.toml；支持相似度去重（embedding）、LRU 限流、slug 生成与校验
  - generate_slug/validate_slug/generate_skill_toml 等
  - /home/mi/work/open_source/zeroclaw/src/skills/creator.rs:21-33, 37-80, 82-131, 132-168（函数体在相应范围）
- 自动改进（SkillImprover）
  - should_improve_skill(slug) → 配置与冷却期判断
  - improve_skill(slug, improved_content, reason) → 校验内容、原子写入（临时文件→rename）、记录冷却
  - validate_skill_content(content) → TOML [skill] 基础验证
  - /home/mi/work/open_source/zeroclaw/src/skills/improver.rs:14-27, 30-40, 47-60, 95-101, 125-152

九、错误处理与失败模式（要点）
- 技能提示注入
  - Compact 模式中未内联 instructions，需 read_skill(name) 拉取全文；若 name 不存在，返回已安装技能列表，便于纠正输入
  - /home/mi/work/open_source/zeroclaw/src/tools/read_skill.rs:64-83, 96-111
- Shell/Script 工具
  - 超额速率与动作预算 → 提前返回错误；命令审批/路径阻断失败 → 返回错误；超时（60s）杀死进程；大输出截断（1MB）
  - /home/mi/work/open_source/zeroclaw/src/tools/skill_tool.rs:102-138, 155-202
- HTTP 工具
  - 非 http/https 拒绝；请求错误/超时/过大响应体 → 明确错误信息或截断提示；HTTP 非 2xx → success=false 且 error=HTTP <status>
  - /home/mi/work/open_source/zeroclaw/src/tools/skill_http.rs:93-101, 104-118, 120-151
- 安装/审计
  - 审计失败阻断安装；ClawHub zip 路径不安全/过大/HTTP 429 → 明确报错并回滚目录；Remove 强路径校验防越界删除
  - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:1269-1299, 1335-1354, 1466-1481, 1487-1493
- 测试（见下节）若失败 → CLI 返回错误并打印失败详情

十、性能与测试钩子
- TEST.sh 驱动的技能测试框架（针对每个技能目录）
  - 测试用例格式：command | expected_exit_code | expected_output_pattern
  - 解析/匹配：parse_test_line、pattern_matches（优先 regex，失败再 substring）
  - 执行：run_test_case → sh -c 命令，收集 stdout/stderr 与退出码，对比期望并返回失败详情
  - 聚合与打印：test_skill/test_all_skills/print_results，汇总通过/失败统计，失败用例打印期望/实际
  - 关键实现：
    - parse_test_line: /home/mi/work/open_source/zeroclaw/src/skills/testing.rs:35-74
    - pattern_matches: /home/mi/work/open_source/zeroclaw/src/skills/testing.rs:81-93
    - run_test_case: /home/mi/work/open_source/zeroclaw/src/skills/testing.rs:96-149
    - test_skill: /home/mi/work/open_source/zeroclaw/src/skills/testing.rs:152-183
    - test_all_skills: /home/mi/work/open_source/zeroclaw/src/skills/testing.rs:186-221
    - print_results: /home/mi/work/open_source/zeroclaw/src/skills/testing.rs:223-292
- 性能评测：未见专门的基准/统计接口；HTTP/Shell 超时与输出截断是主要的运行时性能/稳定性保障点

十一、安全要点（横切关注）
- Shell/Script 工具
  - 强制经 SecurityPolicy：命令审批、路径阻断、速率限制/动作预算、环境变量白名单、工作目录限定、超时与输出截断
  - /home/mi/work/open_source/zeroclaw/src/tools/skill_tool.rs:102-153, 155-178
- HTTP 工具
  - 仅 http/https；固定超时；响应体上限与截断；错误明确化
  - /home/mi/work/open_source/zeroclaw/src/tools/skill_http.rs:93-151
- 安装与审计
  - 静态审计覆盖技能目录中文本与清单；阻断脚本/高风险模式/越界链接/大文件；ClawHub zip 路径安全检查与大小限制；审计失败回滚
  - /home/mi/work/open_source/zeroclaw/src/skills/audit.rs:120-161, 163-179, 181-243, 245-300
  - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:1260-1354（安装与回滚）
- 提示注入与最小特权
  - Compact 模式减少系统提示体积，仅在需要时获取技能全文，降低越权/注入面；工具以函数调用显式暴露，避免模糊 shell 使用

十二、SOP（标准作业流程）
- 列出技能
  - zeroclaw skills list → 展示技能名/版本/描述/工具/标签
  - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:1361-1399
- 审计技能
  - zeroclaw skills audit <source|name> → 对路径或已安装技能运行审计，打印发现项，失败退出
  - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:1401-1438
- 安装技能
  - zeroclaw skills install <source> → 自动识别 ClawHub/Git/本地，安装后执行静态审计；失败回滚
  - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:1439-1464, 1445-1454
- 移除技能
  - zeroclaw skills remove <name> → 防路径穿越校验后删除
  - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:1465-1494
- 测试技能
  - zeroclaw skills test [<name>] [--verbose] → 执行 TEST.sh 案例，汇总与错误回传
  - /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:1495-1531

十三、关键代码引用（精选）
- 模型定义
  - Skill / SkillTool: /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:34-48, 51-61
- 提示构建
  - skills_to_prompt / skills_to_prompt_with_mode: /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:751-758, 760-864
- 技能工具转换
  - skills_to_tools: /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:866-905
- 工具注册与运行时接线
  - register_skill_tools: /home/mi/work/open_source/zeroclaw/src/tools/mod.rs:308-333
  - Prompt 构建调用: /home/mi/work/open_source/zeroclaw/src/agent/prompt.rs:222-226
  - 注册技能工具 & Compact 模式 read_skill 描述: /home/mi/work/open_source/zeroclaw/src/agent/loop_.rs:3696-3699, 3726-3734, 4620-4624, 4638-4645
- 工具实现
  - SkillShellTool：结构/构造/参数/执行: /home/mi/work/open_source/zeroclaw/src/tools/skill_tool.rs:20-27, 34-46, 48-68, 70-82, 99-202
  - SkillHttpTool：结构/构造/参数/执行: /home/mi/work/open_source/zeroclaw/src/tools/skill_http.rs:17-23, 25-37, 39-59, 61-73, 76-153
  - read_skill 工具：/home/mi/work/open_source/zeroclaw/src/tools/read_skill.rs:6-24, 27-48, 50-113
- 加载与目录
  - load_skills / load_skills_with_config / load_skills_with_open_skills_settings: /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:125-138, 140-152
  - skills_dir / init_skills_dir: /home/mi/work/open_source/zeroclaw/src/skills/mod.rs:907-910, 912-940
- 安装与审计
  - 审计入口与细则：/home/mi/work/open_source/zeroclaw/src/skills/audit.rs:30-66, 92-161, 163-179, 181-243, 245-300
  - ClawHub 安装：/home/mi/work/open_source/zeroclaw/src/skills/mod.rs:1260-1354
  - Git/ClawHub 源检测：/home/mi/work/open_source/zeroclaw/src/skills/mod.rs:970-975, 1047-1051, 1980-2009（测试用例摘录）
  - CLI 命令处理（List/Audit/Install/Remove/Test）：/home/mi/work/open_source/zeroclaw/src/skills/mod.rs:1356-1534
- 测试框架（TEST.sh）
  - parse_test_line / pattern_matches / run_test_case / test_skill / test_all_skills / print_results:
    - /home/mi/work/open_source/zeroclaw/src/skills/testing.rs:35-74, 81-93, 96-149, 152-183, 186-221, 223-292
- 配置
  - SkillsPromptInjectionMode / SkillsConfig: /home/mi/work/open_source/zeroclaw/src/config/schema.rs:1518-1526, 1536-1561

附：风险与合规清单（执行前后自检）
- 是否处于 Compact 模式且需要 read_skill(name) 获取完整说明？（避免误读简要摘要）
- Shell/Script 工具是否通过 SecurityPolicy 校验并在预算内？（审批/路径/超时/输出）
- HTTP 工具是否使用 http/https 且响应体可控？（30s/1MB）
- 安装来源是否可信且通过审计？（ClawHub/Git/本地，审计失败回滚）
- TEST.sh 是否覆盖关键路径并全部通过？（失败阻断上线）