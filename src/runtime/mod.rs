pub mod docker;
pub mod native;
pub mod traits;

pub use docker::DockerRuntime;
pub use native::NativeRuntime;
pub use traits::RuntimeAdapter;

use crate::config::RuntimeConfig;

/// Factory: create the right runtime from config
/// 根据配置选择并创建运行时适配器；仅支持 "native" 与 "docker"，
/// 其他值会返回错误以提示用户修正配置。
pub fn create_runtime(config: &RuntimeConfig) -> anyhow::Result<Box<dyn RuntimeAdapter>> {
    match config.kind.as_str() {
        // 直接在宿主机环境运行，提供完整的 shell 访问能力
        "native" => Ok(Box::new(NativeRuntime::new())),
        // 在 Docker 容器中执行任务，使用 config.docker 中的参数
        "docker" => Ok(Box::new(DockerRuntime::new(config.docker.clone()))),
        // 预留占位：Cloudflare 运行时尚未实现，明确返回错误并建议使用 native
        "cloudflare" => anyhow::bail!(
            "runtime.kind='cloudflare' is not implemented yet. Use runtime.kind='native' for now."
        ),
        // 空字符串（仅包含空白）视为配置缺失，返回格式化的错误信息
        other if other.trim().is_empty() => {
            anyhow::bail!("runtime.kind cannot be empty. Supported values: native, docker")
        }
        // 未知的运行时类型，一律报错并给出支持的枚举值
        other => anyhow::bail!("Unknown runtime kind '{other}'. Supported values: native, docker"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_native() {
        let cfg = RuntimeConfig {
            kind: "native".into(),
            ..RuntimeConfig::default()
        };
        let rt = create_runtime(&cfg).unwrap();
        assert_eq!(rt.name(), "native");
        assert!(rt.has_shell_access());
    }

    #[test]
    fn factory_docker() {
        let cfg = RuntimeConfig {
            kind: "docker".into(),
            ..RuntimeConfig::default()
        };
        let rt = create_runtime(&cfg).unwrap();
        assert_eq!(rt.name(), "docker");
        assert!(rt.has_shell_access());
    }

    #[test]
    fn factory_cloudflare_errors() {
        let cfg = RuntimeConfig {
            kind: "cloudflare".into(),
            ..RuntimeConfig::default()
        };
        match create_runtime(&cfg) {
            Err(err) => assert!(err.to_string().contains("not implemented")),
            Ok(_) => panic!("cloudflare runtime should error"),
        }
    }

    #[test]
    fn factory_unknown_errors() {
        let cfg = RuntimeConfig {
            kind: "wasm-edge-unknown".into(),
            ..RuntimeConfig::default()
        };
        match create_runtime(&cfg) {
            Err(err) => assert!(err.to_string().contains("Unknown runtime kind")),
            Ok(_) => panic!("unknown runtime should error"),
        }
    }

    #[test]
    fn factory_empty_errors() {
        let cfg = RuntimeConfig {
            kind: String::new(),
            ..RuntimeConfig::default()
        };
        match create_runtime(&cfg) {
            Err(err) => assert!(err.to_string().contains("cannot be empty")),
            Ok(_) => panic!("empty runtime should error"),
        }
    }
}
