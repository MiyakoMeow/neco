//! `NeoCo`路径管理模块
//!
//! 统一管理所有数据目录和配置目录，遵循XDG标准。
//!
//! ## XDG标准
//!
//! - `XDG_DATA_HOME`: 默认 `~/.local/share`
//! - `XDG_CONFIG_HOME`: 默认 `~/.config`
//!
//! ## 目录结构
//!
//! ```text
//! XDG_DATA_HOME/neoco/
//!   └── sessions/          # Session数据
//!
//! XDG_CONFIG_HOME/neoco/
//!   ├── skills/            # Skill定义
//!   ├── prompts/           # Prompt模板
//!   └── skill_index.json   # Skill索引
//! ```

use std::path::{Path, PathBuf};

/// XDG标准目录路径管理器
#[derive(Debug, Clone)]
pub struct Paths {
    /// 数据目录 (`XDG_DATA_HOME/neoco`)
    pub data_dir: PathBuf,
    /// 配置目录 (`XDG_CONFIG_HOME/neoco`)
    pub config_dir: PathBuf,
    /// Session数据目录 (`data_dir/sessions`)
    pub session_dir: PathBuf,
    /// Skills目录 (`config_dir/skills`)
    pub skills_dir: PathBuf,
    /// Prompts目录 (`config_dir/prompts`)
    pub prompts_dir: PathBuf,
    /// Skill索引文件 (`config_dir/skill_index.json`)
    pub skill_index_path: PathBuf,
}

impl Paths {
    /// 使用默认XDG目录创建`Paths`
    ///
    /// 优先使用环境变量 `XDG_DATA_HOME` 和 `XDG_CONFIG_HOME`。
    /// 全平台统一逻辑: 默认使用 `~/.local/share/neoco` 和 `~/.config/neoco`
    #[must_use]
    pub fn new() -> Self {
        let data_dir = std::env::var("XDG_DATA_HOME")
            .map(|p| PathBuf::from(p).join("neoco"))
            .ok()
            .or_else(dirs::data_dir)
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".local")
                    .join("share")
                    .join("neoco")
            });

        let config_dir = std::env::var("XDG_CONFIG_HOME")
            .map(|p| PathBuf::from(p).join("neoco"))
            .ok()
            .or_else(dirs::config_dir)
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".config")
                    .join("neoco")
            });

        Self {
            data_dir: data_dir.clone(),
            config_dir: config_dir.clone(),
            session_dir: data_dir.join("sessions"),
            skills_dir: config_dir.join("skills"),
            prompts_dir: config_dir.join("prompts"),
            skill_index_path: config_dir.join("skill_index.json"),
        }
    }

    /// 使用自定义基础目录创建`Paths`
    #[must_use]
    pub fn with_base(data_base: &Path, config_base: &Path) -> Self {
        let data_dir = data_base.join("neoco");
        let config_dir = config_base.join("neoco");

        Self {
            data_dir: data_dir.clone(),
            config_dir: config_dir.clone(),
            session_dir: data_dir.join("sessions"),
            skills_dir: config_dir.join("skills"),
            prompts_dir: config_dir.join("prompts"),
            skill_index_path: config_dir.join("skill_index.json"),
        }
    }

    /// 确保所有必需的目录存在
    ///
    /// # Errors
    ///
    /// Returns an error if directory creation fails.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.session_dir)?;
        std::fs::create_dir_all(&self.skills_dir)?;
        std::fs::create_dir_all(&self.prompts_dir)?;
        Ok(())
    }
}

impl Default for Paths {
    fn default() -> Self {
        Self::new()
    }
}
