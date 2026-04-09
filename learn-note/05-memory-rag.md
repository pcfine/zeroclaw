# A5：Memory & RAG 子系统

1. 概述
- 职责边界
  - Memory：面向代理的长期/会话记忆（存/取/删/导出/清理/健康检查），支持命名空间与时间范围过滤。
  - Retrieval：SQLite FTS5（BM25）+ 向量检索（余弦相似度）+ 加权融合（Hybrid）与 LIKE 兜底搜索。
  - Embeddings：OpenAI 兼容式提供者（含 OpenRouter、自定义 URL），无配置时降级为 Noop（仅关键词）。
  - RAG（硬件场景示例）：Markdown/PDF 文档切块与关键词检索，板卡别名解析，构建查询上下文。
  - Tools：对外暴露存储/检索/导出/删除/批删操作，统一返回 ToolResult，写操作受 SecurityPolicy 执行限制。
- 关键路径（高频）
  - memory_store → SecurityPolicy 执行检查 → Memory::store（SQLite: 先算 embedding，再入库/upsert）
  - memory_recall → 参数校验（RFC 3339 时间等）→ Memory::recall → FTS5/BM25 与向量相似度 → hybrid_merge → 批量取回 → LIKE 兜底
  - memory_export → 构造 ExportFilter → Memory::export（SQLite 覆盖高效查询；默认回退 list+后过滤）
  - memory_forget / memory_purge → SecurityPolicy 检查 → Memory::forget / purge_*（SQLite 覆盖批删，默认不支持）

2. 核心类型与存储后端
- Memory 抽象（基础定义）
  - /home/mi/work/open_source/zeroclaw/src/memory/traits.rs
    - ExportFilter（4–14）
    - ProceduralMessage（20–26）
    - MemoryEntry（28–47，Debug 53–66）
    - MemoryCategory 枚举 + serde/Display（68–108）
    - trait Memory（110–258）
      - store/recall/get/list/forget
      - purge_namespace/purge_session（默认不支持，需后端覆盖）
      - count/health_check
      - store_procedural（默认 no-op）、recall_namespaced（默认委托 recall 再过滤）
      - export（默认 list+后过滤）
      - store_with_metadata（默认委托 store）
- 存储后端（选择与工厂）
  - /home/mi/work/open_source/zeroclaw/src/memory/mod.rs
    - create_memory / create_memory_with_storage[_and_routes]（220–364，239–364）
    - effective_memory_backend_name（83–95）：支持 StorageProvider 覆盖
    - 嵌入配置解析 resolve_embedding_config（153–218）：路由 hint 与 provider 专属 env 优先权
    - response cache 工厂（384–408）
  - 后端分类与默认回退
    - Sqlite（主后端，混合检索+FTS5+embedding 缓存）：/src/memory/sqlite.rs（见第 4 节）
    - Lucid（基于 SQLite 包装）：/src/memory/lucid.rs（按需）
    - Markdown（平面文件）：/src/memory/markdown.rs
    - Qdrant（向量库）：/src/memory/qdrant.rs（需 URL/集合配置）
    - None（禁用持久化）：/src/memory/none.rs

3. 工具与 API（对外）
- 统一 Tool 接口与返回 ToolResult
  - /home/mi/work/open_source/zeroclaw/src/tools/traits.rs（接口定义，略）
- Memory Tools
  - memory_store（写入）
    - /home/mi/work/open_source/zeroclaw/src/tools/memory_store.rs（1–94，测试 96–228）
    - 参数：key（必填）、content（必填）、category（可选：core/daily/conversation/自定义）
    - 执行：SecurityPolicy.enforce_tool_operation(ToolOperation::Act,"memory_store") → memory.store
  - memory_recall（检索）
    - /home/mi/work/open_source/zeroclaw/src/tools/memory_recall.rs（1–148，测试 150–258）
    - 参数：query（可选，当提供 since/until 时可为空）、limit、since/until（RFC3339）、search_mode（bm25/embedding/hybrid）
    - 执行：参数校验 → memory.recall → “Found N memories” 格式化输出（含可选得分）
  - memory_export（GDPR 批量导出）
    - /home/mi/work/open_source/zeroclaw/src/tools/memory_export.rs（1–105，测试 107–195）
    - 参数：namespace/session_id/category/since/until（任意组合）
    - 执行：构造 ExportFilter → memory.export → 输出 JSON 数组字符串
  - memory_forget（按 key 删除）
    - /home/mi/work/open_source/zeroclaw/src/tools/memory_forget.rs（1–79，测试 81–183）
    - 参数：key（必填）
    - 执行：SecurityPolicy 检查 → memory.forget → 输出是否找到并删除
  - memory_purge（批删）
    - /home/mi/work/open_source/zeroclaw/src/tools/memory_purge.rs（1–114，测试 116–283）
    - 参数：namespace 或 session_id（二者至少一项）
    - 执行：SecurityPolicy 检查 → 调用 purge_namespace/purge_session → 汇总删除计数

4. 执行链路（以 SQLite 为例）
- 混合检索与兜底
  - /home/mi/work/open_source/zeroclaw/src/memory/sqlite.rs
    - recall（604–840 段内）
      - 空查询 → recall_by_time_only（486–555）
      - 非空查询：
        - 当 SearchMode != Bm25：计算查询 embedding（619–625）
        - 关键词检索（FTS5 BM25）：fts5_search（641–647）
        - 向量检索（余弦相似度）：vector_search（648–655）
        - 结果融合：vector::hybrid_merge（679–686）
        - 批量取回命中的完整行，按 session/time 过滤（688–759）
        - 若为空 → LIKE 兜底，动态 WHERE content/key LIKE… + 时间边界（761–835）
        - 截断到 limit（836–837）
    - recall_by_time_only（486–555）：构造 SQL，支持 session/since/until，按 updated_at DESC，LIMIT N
    - store（564–602）：先算内容 embedding（可为 None），UPSERT（更新内容/类别/embedding/updated_at/session_id）
    - get（842–873）、list（875–942）
    - forget（944–954）、purge_namespace（956–970，以 category 为 namespace）、purge_session（972–986）
    - count（988–999）、health_check（1001–1006）
    - export（1008–1073）：按可选 namespace/session/category/since/until 过滤并按 created_at 排序
- 向量与融合核心
  - /home/mi/work/open_source/zeroclaw/src/memory/vector.rs
    - cosine_similarity（3–35）
    - vec_to_bytes/bytes_to_vec（37–55）
    - hybrid_merge（66–133）：归一化 BM25 分数后按 vector_weight/keyword_weight 线性融合，去重并截断
- 嵌入提供者
  - /home/mi/work/open_source/zeroclaw/src/memory/embeddings.rs
    - EmbeddingProvider trait（3–22）；NoopEmbedding（24–41）
    - OpenAiEmbedding HTTP 客户端与 /v1/embeddings 拼接（45–94，96–155）
    - create_embedding_provider 工厂（159–191）：openai/openrouter/custom:URL/others→Noop

5. 配置要点（MemoryConfig 摘要）
- /home/mi/work/open_source/zeroclaw/src/config/schema.rs
  - SearchMode（Embedding/Bm25/Hybrid，默认 Hybrid）（4838–4843）
  - MemoryConfig 结构（4847–4970，及默认实现 5054–5087）
    - backend: sqlite/lucid/qdrant/markdown/none
    - auto_save、hygiene_enabled/retention/归档清理
    - embedding_provider/model/dimensions
    - vector_weight/keyword_weight、search_mode、min_relevance_score
    - embedding_cache_size、chunk_max_tokens
    - response cache（enabled/ttl/max/hot_entries）
    - snapshot（enabled/on_hygiene/auto_hydrate）
    - retrieval_stages/rerank 开关阈值/fts_early_return_score
    - default_namespace、conflict_threshold、audit 开关与保留天数
    - sqlite_open_timeout_secs
    - qdrant 子配置
- 嵌入路由与密钥优先级
  - /home/mi/work/open_source/zeroclaw/src/memory/mod.rs
    - embedding_provider_env_key（137–151）：OPENAI_API_KEY 等优先于调用方默认 key，避免跨提供者泄漏
    - resolve_embedding_config（153–218）：支持 hint: 路由，校验 provider/model/dims；route.api_key 优先

6. 常见失败模式与返回
- 工具层输入缺失
  - memory_store 缺 key/content → Err(anyhow)（memory_store.rs 52–61）
  - memory_forget 缺 key → Err(anyhow)（memory_forget.rs 44–48）
  - memory_purge 未给 namespace 与 session_id → Err(anyhow)（memory_purge.rs 52–56）
  - memory_recall 同时缺 query/since/until → ToolResult.success=false + 错误提示（memory_recall.rs 63–71）
- 时间格式与区间
  - since/until 需 RFC3339；无效则 ToolResult.success=false（memory_recall.rs 73–95）
  - since ≥ until → 返回 "'since' must be before 'until'"（96–109，103–106）
- 安全策略与速率
  - 只读模式或速率限制：SecurityPolicy.enforce_tool_operation 返回错误 → ToolResult.success=false 且不落库/不删除（memory_store.rs 70–79 等；memory_forget.rs 50–58；memory_purge.rs 58–67）
- 后端能力差异
  - 默认 Memory::purge_* 不支持（traits.rs 153–162）→ SQLite 已覆盖；其他后端可能返回错误
  - backend=unknown → fallback 到 markdown（memory/mod.rs 74–80）
  - backend=none → 禁用持久化，迁移时被显式拒绝（create_memory_for_migration，366–383）
- 嵌入相关
  - 无 API key 或 provider=none → NoopEmbedding（仅关键词，可能影响向量检索效果）
  - 远端 embeddings API 返回非 2xx → anyhow::bail!（embeddings.rs 125–129）

7. 代码索引（文件/符号与行号）
- Memory 基础与工厂
  - /home/mi/work/open_source/zeroclaw/src/memory/traits.rs
    - ExportFilter（4–14）、ProceduralMessage（20–26）、MemoryEntry（28–47, 53–66）
    - MemoryCategory（68–108）、trait Memory（110–258）
  - /home/mi/work/open_source/zeroclaw/src/memory/mod.rs
    - effective_memory_backend_name（83–95）
    - resolve_embedding_config（153–218）
    - create_memory[_with_storage_and_routes]（220–364，239–364）
    - create_response_cache（384–408）
- SQLite 实现（部分关键）
  - /home/mi/work/open_source/zeroclaw/src/memory/sqlite.rs
    - recall_by_time_only（486–555）
    - store（564–602）
    - recall（604–840 片段）
    - get（842–873）、list（875–942）、forget（944–954）
    - purge_namespace（956–970）、purge_session（972–986）
    - count（988–999）、health_check（1001–1006）
    - export（1008–1073）
- 向量/融合/嵌入
  - /home/mi/work/open_source/zeroclaw/src/memory/vector.rs
    - cosine_similarity（3–35）、hybrid_merge（66–133）
  - /home/mi/work/open_source/zeroclaw/src/memory/embeddings.rs
    - EmbeddingProvider（3–22）、NoopEmbedding（24–41）
    - OpenAiEmbedding（45–94, 96–155）、create_embedding_provider（159–191）
- RAG 示例（硬件）
  - /home/mi/work/open_source/zeroclaw/src/rag/mod.rs
    - DatasheetChunk（13–22）
    - parse_pin_aliases（30–99）
    - HardwareRag（141–216）
    - pin_alias_context（223–248）
    - retrieve（250–286）

8. 附：关键片段
- Memory trait（核心签名）
  - /home/mi/work/open_source/zeroclaw/src/memory/traits.rs（125–135）
    - async fn recall(&self, query: &str, limit: usize, session_id: Option<&str>, since: Option<&str>, until: Option<&str>) -> anyhow::Result<Vec<MemoryEntry>>;
- SQLite 检索融合与兜底
  - /home/mi/work/open_source/zeroclaw/src/memory/sqlite.rs（679–686）
    - vector::hybrid_merge(&vector_results, &keyword_results, vector_weight, keyword_weight, limit)
  - /home/mi/work/open_source/zeroclaw/src/memory/sqlite.rs（761–835）
    - LIKE 兜底：动态拼接 (content LIKE ? OR key LIKE ?) + 时间边界，按 updated_at DESC
- 融合打分公式
  - /home/mi/work/open_source/zeroclaw/src/memory/vector.rs（114–123）
    - final_score = vector_weight * vector_score + keyword_weight * keyword_score（BM25 先归一化）
- 嵌入路由与密钥优先级
  - /home/mi/work/open_source/zeroclaw/src/memory/mod.rs（137–151, 153–218）
    - provider 专属环境变量（如 OPENAI_API_KEY/COHERE_API_KEY）优先于默认提供者的调用方密钥；支持 hint:semantic 路由覆盖 provider/model/dims
- 工具安全检查模板
  - /home/mi/work/open_source/zeroclaw/src/tools/memory_store.rs（70–79）
    - security.enforce_tool_operation(ToolOperation::Act, "memory_store") → ToolResult{success: false, error: Some(..)}

——

附注
- 本笔记覆盖：概述、核心类型与存储后端、工具与 API、执行链路、配置、常见失败模式与返回、代码索引、关键片段。
- 所有路径为绝对路径，行号以当前仓库版本为准。