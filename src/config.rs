use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "ssak_config.json";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    pub input_dir: String,
    pub output_dir: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            input_dir: ".".to_string(),
            output_dir: "./output".to_string(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        if Path::new(CONFIG_FILE).exists() {
            match fs::read_to_string(CONFIG_FILE) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => AppConfig::default(),
            }
        } else {
            AppConfig::default()
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(CONFIG_FILE, content)
    }

    #[allow(dead_code)]
    pub fn input_path(&self, filename: &str) -> PathBuf {
        Path::new(&self.input_dir).join(filename)
    }

    pub fn output_path(&self, filename: &str) -> PathBuf {
        Path::new(&self.output_dir).join(filename)
    }

    pub fn ensure_output_dir(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.output_dir)
    }
}

pub fn configure_paths(config: &mut AppConfig) {
    println!("Текущая папка с исходными данными: {}", config.input_dir);
    println!("Текущая папка для результатов: {}", config.output_dir);
    println!();

    let input: String = dialoguer::Input::new()
        .with_prompt("Введите новую папку с исходными данными (Enter — оставить без изменений)")
        .default(config.input_dir.clone())
        .interact_text()
        .unwrap_or_else(|_| config.input_dir.clone());

    let output: String = dialoguer::Input::new()
        .with_prompt("Введите новую папку для результатов (Enter — оставить без изменений)")
        .default(config.output_dir.clone())
        .interact_text()
        .unwrap_or_else(|_| config.output_dir.clone());

    config.input_dir = input;
    config.output_dir = output;

    if let Err(e) = config.ensure_output_dir() {
        println!("Не удалось создать папку для результатов: {}", e);
    }

    match config.save() {
        Ok(_) => println!("Настройки сохранены в {}", CONFIG_FILE),
        Err(e) => println!("Ошибка сохранения настроек: {}", e),
    }
}
