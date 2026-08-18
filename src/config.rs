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

/// Считывает строку с клавиатуры. Возвращает None, если введено "q" —
/// это сигнал отмены и возврата в главное меню.
pub fn read_or_quit(prompt: &str, default: &str) -> Option<String> {
    let value: String = dialoguer::Input::new()
        .with_prompt(prompt)
        .default(default.to_string())
        .interact_text()
        .unwrap_or_else(|_| default.to_string());

    if value.trim().eq_ignore_ascii_case("q") {
        None
    } else {
        Some(value)
    }
}

pub fn configure_paths(config: &mut AppConfig) {
    println!("Текущая папка с исходными данными: {}", config.input_dir);
    println!("Текущая папка для результатов: {}", config.output_dir);
    println!();
    println!("(Введите q в любом поле, чтобы отменить и вернуться в главное меню)");
    println!();

    let input = match read_or_quit(
        "Введите новую папку с исходными данными",
        &config.input_dir,
    ) {
        Some(v) => v,
        None => {
            println!("Отменено. Возврат в главное меню.");
            return;
        }
    };

    let input_path = Path::new(&input);
    if input_path.exists() && input_path.is_dir() {
        println!("Папка найдена: {}", input_path.display());
        config.input_dir = input;
    } else {
        println!(
            "Папка не найдена: {}. Папка с исходными данными не изменена.",
            input
        );
    }

    let output = match read_or_quit(
        "Введите новую папку для результатов",
        &config.output_dir,
    ) {
        Some(v) => v,
        None => {
            println!("Отменено. Возврат в главное меню.");
            if let Err(e) = config.save() {
                println!("Ошибка сохранения настроек: {}", e);
            }
            return;
        }
    };

    let output_path = Path::new(&output);
    if output_path.exists() && output_path.is_dir() {
        println!("Папка найдена: {}", output_path.display());
        config.output_dir = output;
    } else {
        let create = dialoguer::Confirm::new()
            .with_prompt(format!("Папка '{}' не найдена. Создать её?", output))
            .default(true)
            .interact()
            .unwrap_or(false);

        if create {
            match fs::create_dir_all(&output) {
                Ok(_) => {
                    println!("Папка создана: {}", output);
                    config.output_dir = output;
                }
                Err(e) => println!("Не удалось создать папку: {}", e),
            }
        } else {
            println!("Папка для результатов не изменена.");
        }
    }

    match config.save() {
        Ok(_) => println!("Настройки сохранены в {}", CONFIG_FILE),
        Err(e) => println!("Ошибка сохранения настроек: {}", e),
    }
}
