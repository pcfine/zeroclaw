use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Filter criteria for bulk memory export (GDPR Art. 20 data portability).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportFilter {
    pub namespace: Option<String>,
    pub session_id: Option<String>,
    pub category: Option<MemoryCategory>,
    /// RFC 3339 lower bound (inclusive) on created_at.
    pub since: Option<String>,
    /// RFC 3339 upper bound (inclusive) on created_at.
    pub until: Option<String>,
}

/// A single message in a conversation trace for procedural memory.
///
/// Used to capture "how to" patterns from tool-calling turns so that
/// backends that support procedural storage can learn from them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProceduralMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A single memory entry
#[derive(Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub key: String,
    pub content: String,
    pub category: MemoryCategory,
    pub timestamp: String,
    pub session_id: Option<String>,
    pub score: Option<f64>,
    /// Namespace for isolation between agents/contexts.
    #[serde(default = "default_namespace")]
    pub namespace: String,
    /// Importance score (0.0–1.0) for prioritized retrieval.
    #[serde(default)]
    pub importance: Option<f64>,
    /// If this entry was superseded by a newer conflicting entry.
    #[serde(default)]
    pub superseded_by: Option<String>,
}

fn default_namespace() -> String {
    "default".into()
}

impl std::fmt::Debug for MemoryEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryEntry")
            .field("id", &self.id)
            .field("key", &self.key)
            .field("content", &self.content)
            .field("category", &self.category)
            .field("timestamp", &self.timestamp)
            .field("score", &self.score)
            .field("namespace", &self.namespace)
            .field("importance", &self.importance)
            .finish_non_exhaustive()
    }
}

/// Memory categories for organization
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryCategory {
    /// Long-term facts, preferences, decisions
    Core,
    /// Daily session logs
    Daily,
    /// Conversation context
    Conversation,
    /// User-defined custom category
    Custom(String),
}

impl serde::Serialize for MemoryCategory {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for MemoryCategory {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "core" => Self::Core,
            "daily" => Self::Daily,
            "conversation" => Self::Conversation,
            _ => Self::Custom(s),
        })
    }
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core => write!(f, "core"),
            Self::Daily => write!(f, "daily"),
            Self::Conversation => write!(f, "conversation"),
            Self::Custom(name) => write!(f, "{name}"),
        }
    }
}

/// Core memory trait — implement for any persistence backend
#[async_trait]
pub trait Memory: Send + Sync {
    /// Backend name
    /// 后端名称
    fn name(&self) -> &str;

    /// Store a memory entry, optionally scoped to a session
    /// 存储一条记忆项，可选指定所属会话范围
    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()>;

    /// Recall memories matching a query (keyword search), optionally scoped to a session
    /// and time range. Time bounds use RFC 3339 / ISO 8601 format
    /// (e.g. "2025-03-01T00:00:00Z"); inclusive (created_at >= since, created_at <= until).
    /// 根据查询关键词召回记忆，可选按会话与时间范围过滤（RFC 3339/ISO 8601，区间含边界）
    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    /// Get a specific memory by key
    /// 通过键获取单个记忆项
    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>>;

    /// List all memory keys, optionally filtered by category and/or session
    /// 列出所有记忆（可按类别和/或会话过滤）
    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    /// Remove a memory by key
    /// 通过键删除记忆
    async fn forget(&self, key: &str) -> anyhow::Result<bool>;

    /// Remove all memories in a namespace (category).
    /// 删除某命名空间（类别）下的全部记忆
    /// Returns the number of deleted entries.
    /// 返回删除的条目数
    /// Default: returns unsupported error. Backends that support bulk deletion override this.
    /// 默认：返回“不支持”错误；支持批量删除的后端应覆盖实现
    async fn purge_namespace(&self, _namespace: &str) -> anyhow::Result<usize> {
        anyhow::bail!("purge_namespace not supported by this memory backend")
    }

    /// Remove all memories in a session.
    /// Returns the number of deleted entries.
    /// 返回删除的条目数
    /// Default: returns unsupported error. Backends that support bulk deletion override this.
    /// 默认：返回“不支持”错误；支持批量删除的后端应覆盖实现
    async fn purge_session(&self, _session_id: &str) -> anyhow::Result<usize> {
        anyhow::bail!("purge_session not supported by this memory backend")
    }

    /// Count total memories
    /// 统计记忆总数
    async fn count(&self) -> anyhow::Result<usize>;

    /// Health check
    /// 健康检查
    async fn health_check(&self) -> bool;

    /// Store a conversation trace as procedural memory.
    /// 将对话轨迹存为过程性记忆
    ///
    /// Backends that support procedural storage override this
    /// to extract "how to" patterns from tool-calling turns.  The default
    /// implementation is a no-op.
    async fn store_procedural(
        &self,
        _messages: &[ProceduralMessage],
        _session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// Recall memories scoped to a specific namespace.
    ///
    /// Default implementation delegates to `recall()` and filters by namespace.
    /// 默认实现委托给 `recall()` 并按命名空间过滤
    /// Backends with native namespace support should override for efficiency.
    /// 有原生命名空间支持的后端应覆盖以提升效率
    async fn recall_namespaced(
        &self,
        namespace: &str,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let entries = self
            .recall(query, limit * 2, session_id, since, until)
            .await?;
        let filtered: Vec<MemoryEntry> = entries
            .into_iter()
            .filter(|e| e.namespace == namespace)
            .take(limit)
            .collect();
        Ok(filtered)
    }

    /// Bulk-export memories matching the given filter criteria.
    /// 按过滤条件批量导出记忆
    ///
    /// Intended for GDPR Art. 20 data portability. Returns entries ordered by
    /// 用于满足 GDPR 第20条数据可携带性；结果按创建时间升序，排除向量嵌入
    /// creation time (ascending). Embeddings are excluded.
    ///
    /// Default implementation delegates to `list()` and post-filters on
    /// 默认实现基于 `list()` 并在命名空间与时间范围上进行后过滤
    /// namespace and time range. Backends with native query support should
    /// override for efficiency.
    async fn export(&self, filter: &ExportFilter) -> anyhow::Result<Vec<MemoryEntry>> {
        let entries = self
            .list(filter.category.as_ref(), filter.session_id.as_deref())
            .await?;
        let filtered: Vec<MemoryEntry> = entries
            .into_iter()
            .filter(|e| {
                if let Some(ref ns) = filter.namespace {
                    if e.namespace != *ns {
                        return false;
                    }
                }
                if let Some(ref since) = filter.since {
                    if e.timestamp.as_str() < since.as_str() {
                        return false;
                    }
                }
                if let Some(ref until) = filter.until {
                    if e.timestamp.as_str() > until.as_str() {
                        return false;
                    }
                }
                true
            })
            .collect();
        Ok(filtered)
    }

    /// Store a memory entry with namespace and importance.
    ///
    /// Default implementation delegates to `store()`. Backends with native
    /// namespace/importance support should override.
    /// 默认实现委托给 `store()`；具备命名空间/重要性原生支持的后端应覆盖
    async fn store_with_metadata(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
        _namespace: Option<&str>,
        _importance: Option<f64>,
    ) -> anyhow::Result<()> {
        self.store(key, content, category, session_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_category_display_outputs_expected_values() {
        assert_eq!(MemoryCategory::Core.to_string(), "core");
        assert_eq!(MemoryCategory::Daily.to_string(), "daily");
        assert_eq!(MemoryCategory::Conversation.to_string(), "conversation");
        assert_eq!(
            MemoryCategory::Custom("project_notes".into()).to_string(),
            "project_notes"
        );
    }

    #[test]
    fn memory_category_serde_uses_snake_case() {
        let core = serde_json::to_string(&MemoryCategory::Core).unwrap();
        let daily = serde_json::to_string(&MemoryCategory::Daily).unwrap();
        let conversation = serde_json::to_string(&MemoryCategory::Conversation).unwrap();

        assert_eq!(core, "\"core\"");
        assert_eq!(daily, "\"daily\"");
        assert_eq!(conversation, "\"conversation\"");
    }

    #[test]
    fn memory_category_custom_roundtrip() {
        let custom = MemoryCategory::Custom("project_notes".into());
        let json = serde_json::to_string(&custom).unwrap();
        assert_eq!(json, "\"project_notes\"");
        let parsed: MemoryCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, custom);
    }

    #[test]
    fn memory_entry_roundtrip_preserves_optional_fields() {
        let entry = MemoryEntry {
            id: "id-1".into(),
            key: "favorite_language".into(),
            content: "Rust".into(),
            category: MemoryCategory::Core,
            timestamp: "2026-02-16T00:00:00Z".into(),
            session_id: Some("session-abc".into()),
            score: Some(0.98),
            namespace: "default".into(),
            importance: Some(0.7),
            superseded_by: None,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: MemoryEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "id-1");
        assert_eq!(parsed.key, "favorite_language");
        assert_eq!(parsed.content, "Rust");
        assert_eq!(parsed.category, MemoryCategory::Core);
        assert_eq!(parsed.session_id.as_deref(), Some("session-abc"));
        assert_eq!(parsed.score, Some(0.98));
        assert_eq!(parsed.namespace, "default");
        assert_eq!(parsed.importance, Some(0.7));
        assert!(parsed.superseded_by.is_none());
    }
}
