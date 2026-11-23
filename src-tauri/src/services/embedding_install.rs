//! 把随安装包分发的内置向量模型就位到 `data_dir`。
//!
//! candle 加载 safetensors 需要真实文件路径（`from_mmaped_safetensors`），
//! 而包内资源在桌面是 `resource_dir` 下的普通文件、在安卓是 APK 的 `assets`
//! 条目（只能经 `AssetManager` 以流读取，拿不到路径）。因此启动时需要一次
//! 「把随包模型搬到数据目录」的动作：
//!
//! - 桌面：`resource_dir/embedding-models/<model>` → `data_dir/embedding-models/<model>`
//! - 安卓：APK `assets/embedding-models` → `data_dir/embedding-models`（经 native-bridge）
//!
//! 该动作幂等：目标文件齐全时直接跳过，正常启动几乎没有开销，并且放在
//! 后台线程里执行，不阻塞窗口创建。失败只记日志，不阻断启动；检索会自动
//! 退回纯关键词。
//!
//! 未启用 `local-embedding` 的构建根本用不到模型文件，因此整个步骤直接跳过，
//! 避免无谓地拷贝 90MB。

use std::path::Path;

/// 随包模型在资源根下的目录名（桌面 `resource_dir` 与安卓 `assets` 一致）。
#[cfg(all(feature = "local-embedding", target_os = "android"))]
const BUNDLED_MODEL_ROOT: &str = "embedding-models";

/// 启动时调用：确保内置模型在 `data_dir` 下可用。失败仅记录日志。
pub fn ensure_bundled_embedding_model(app: &tauri::AppHandle, data_dir: &Path) {
    #[cfg(not(feature = "local-embedding"))]
    {
        let _ = (app, data_dir);
    }
    #[cfg(feature = "local-embedding")]
    {
        use crate::services::game_engine::memory::{
            builtin_local_model_dir, builtin_local_model_ready, BUILTIN_LOCAL_EMBEDDING_MODEL_ID,
        };

        let target = builtin_local_model_dir(data_dir, BUILTIN_LOCAL_EMBEDDING_MODEL_ID);
        if builtin_local_model_ready(&target) {
            return;
        }
        match install(app, data_dir) {
            Ok(true) => eprintln!(
                "[embedding] bundled model installed to {}",
                target.display()
            ),
            Ok(false) => {}
            Err(error) => {
                // 常见原因：打包时的 resources 里没有 model.safetensors（该文件不入 git）。
                eprintln!(
                    "[embedding] bundled model unavailable, vector retrieval disabled: {error}"
                );
            }
        }
    }
}

#[cfg(all(feature = "local-embedding", not(target_os = "android")))]
fn install(app: &tauri::AppHandle, data_dir: &Path) -> Result<bool, String> {
    use crate::services::game_engine::memory::{
        builtin_local_model_dir, install_builtin_model_files, BUILTIN_LOCAL_EMBEDDING_MODEL_ID,
    };
    use tauri::Manager;

    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("无法解析资源目录：{error}"))?;
    let source = builtin_local_model_dir(&resource_dir, BUILTIN_LOCAL_EMBEDDING_MODEL_ID);
    if !source.is_dir() {
        return Err(format!("资源目录缺少 {}", source.display()));
    }
    let target = builtin_local_model_dir(data_dir, BUILTIN_LOCAL_EMBEDDING_MODEL_ID);
    Ok(install_builtin_model_files(&source, &target)? > 0)
}

#[cfg(all(feature = "local-embedding", target_os = "android"))]
fn install(app: &tauri::AppHandle, data_dir: &Path) -> Result<bool, String> {
    use crate::services::game_engine::memory::{
        builtin_local_model_dir, builtin_local_model_ready, BUILTIN_LOCAL_EMBEDDING_MODEL_ID,
    };
    use tauri::Manager;

    let bridge = app.state::<tauri_plugin_native_bridge::NativeBridge<tauri::Wry>>();
    // assets 里是 embedding-models/<model_id>（"BAAI/bge-small-zh-v1.5" 被落成
    // BAAI_bge-small-zh-v1.5），直接整体解包到 data_dir 下的同名层级。
    let target_root = data_dir.join(BUNDLED_MODEL_ROOT);
    bridge.copy_asset_dir(BUNDLED_MODEL_ROOT, &target_root.to_string_lossy())?;
    if builtin_local_model_ready(&builtin_local_model_dir(
        data_dir,
        BUILTIN_LOCAL_EMBEDDING_MODEL_ID,
    )) {
        Ok(true)
    } else {
        Err(format!(
            "安卓资源拷贝后模型仍不完整：{}",
            target_root.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::game_engine::memory::{
        builtin_local_model_dir, builtin_local_model_ready, install_builtin_model_files,
        BUILTIN_LOCAL_EMBEDDING_MODEL_ID,
    };
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dream-narrative-engine-embedding-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn seed_model(dir: &Path) {
        fs::create_dir_all(dir).expect("create model dir");
        fs::write(dir.join("model.safetensors"), b"weights").expect("write weights");
        fs::write(dir.join("tokenizer.json"), b"{}").expect("write tokenizer");
        fs::write(dir.join("config.json"), b"{}").expect("write config");
        fs::write(dir.join("special_tokens_map.json"), b"{}").expect("write map");
        fs::write(dir.join("tokenizer_config.json"), b"{}").expect("write tokenizer config");
    }

    #[test]
    fn install_copies_missing_files_and_is_idempotent() {
        let root = temp_dir("install");
        let source = root.join("source");
        let target = root.join("target");
        seed_model(&source);

        assert!(!builtin_local_model_ready(&target));
        assert_eq!(
            install_builtin_model_files(&source, &target).expect("first install"),
            5
        );
        assert!(builtin_local_model_ready(&target));
        // 第二次没有文件需要复制。
        assert_eq!(
            install_builtin_model_files(&source, &target).expect("second install"),
            0
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn install_reports_missing_weight_file() {
        let root = temp_dir("missing");
        let source = root.join("source");
        seed_model(&source);
        fs::remove_file(source.join("model.safetensors")).expect("remove weights");
        let target = root.join("target");

        let error = install_builtin_model_files(&source, &target).expect_err("must fail");
        assert!(error.contains("model.safetensors"), "{error}");
        assert!(!builtin_local_model_ready(&target));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn model_dir_follows_data_dir_layout() {
        let root = temp_dir("layout");
        let dir = builtin_local_model_dir(&root, BUILTIN_LOCAL_EMBEDDING_MODEL_ID);
        assert!(dir.ends_with("BAAI_bge-small-zh-v1.5"));
        assert!(dir.starts_with(&root));
        let _ = fs::remove_dir_all(&root);
    }
}
