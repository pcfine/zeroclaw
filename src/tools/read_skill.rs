use super::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;

/// Compact-mode helper for loading a skill's source file on demand.
/// 中文：在“紧凑模式（compact skills mode）”下的辅助工具：
/// 当系统提示只展示技能摘要而不包含完整源文件时，按需读取指定技能的源文件内容，
/// 以便在推理时获取完整的技能指令。
pub struct ReadSkillTool {
    // 中文：工作区根目录，用于定位 skills/<name>/SKILL.md 或 SKILL.toml

    workspace_dir: PathBuf,
    // 中文：是否启用“开放技能”（允许从外部目录挂载/加载技能）
    open_skills_enabled: bool,
    // 中文：开放技能的根目录（可选）；如果启用，将在该目录下查找技能
    open_skills_dir: Option<String>,
}

impl ReadSkillTool {
    pub fn new(
        workspace_dir: PathBuf,
        open_skills_enabled: bool,
        open_skills_dir: Option<String>,
    ) -> Self {
        Self {
            workspace_dir,
            open_skills_enabled,
            open_skills_dir,
        }
    }
}

#[async_trait]
impl Tool for ReadSkillTool {
    fn name(&self) -> &str {
        "read_skill"
    }

    fn description(&self) -> &str {
        // 中文：在“紧凑模式”下，按名称读取某个技能的完整源文件（无需知道路径）
        "Read the full source file for an available skill by name. Use this in compact skills mode when you need the complete skill instructions without remembering file paths."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        // 中文：参数只需要技能名（与 <available_skills> 列表中的名称一致）
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The skill name exactly as listed in <available_skills>."
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        // 中文：执行流程
        // 1) 解析 name 参数（必填）
        // 2) 按工作区与开放技能设置加载技能列表
        // 3) 按名称查找技能，未找到则返回错误并列出可用名称
        // 4) 若技能存在但无可读位置（location），返回错误
        // 5) 读取源文件并返回内容
        let requested = args
            .get("name")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing 'name' parameter"))?;

        let skills = crate::skills::load_skills_with_open_skills_settings(
            &self.workspace_dir,
            self.open_skills_enabled,
            self.open_skills_dir.as_deref(),
        );

        let Some(skill) = skills
            .iter()
            .find(|skill| skill.name.eq_ignore_ascii_case(requested))
        else {
            let mut names: Vec<&str> = skills.iter().map(|skill| skill.name.as_str()).collect();
            names.sort_unstable();
            let available = if names.is_empty() {
                "none".to_string()
            } else {
                names.join(", ")
            };

            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Unknown skill '{requested}'. Available skills: {available}"
                )),
            });
        };

        let Some(location) = skill.location.as_ref() else {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Skill '{}' has no readable source location.",
                    skill.name
                )),
            });
        };

        match tokio::fs::read_to_string(location).await {
            Ok(output) => Ok(ToolResult {
                success: true,
                output,
                error: None,
            }),
            Err(err) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Failed to read skill '{}' from {}: {err}",
                    skill.name,
                    location.display()
                )),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_tool(tmp: &TempDir) -> ReadSkillTool {
        ReadSkillTool::new(tmp.path().join("workspace"), false, None)
    }

    #[tokio::test]
    async fn reads_markdown_skill_by_name() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("workspace/skills/weather");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "# Weather\n\nUse this skill for forecast lookups.\n",
        )
        .unwrap();

        let result = make_tool(&tmp)
            .execute(json!({ "name": "weather" }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("# Weather"));
        assert!(result.output.contains("forecast lookups"));
    }

    #[tokio::test]
    async fn reads_toml_skill_manifest_by_name() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("workspace/skills/deploy");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.toml"),
            r#"[skill]
name = "deploy"
description = "Ship safely"
"#,
        )
        .unwrap();

        let result = make_tool(&tmp)
            .execute(json!({ "name": "deploy" }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("[skill]"));
        assert!(result.output.contains("Ship safely"));
    }

    #[tokio::test]
    async fn unknown_skill_lists_available_names() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("workspace/skills/weather");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "# Weather\n").unwrap();

        let result = make_tool(&tmp)
            .execute(json!({ "name": "calendar" }))
            .await
            .unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error.as_deref(),
            Some("Unknown skill 'calendar'. Available skills: weather")
        );
    }
}
