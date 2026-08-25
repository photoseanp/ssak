//! src/tools/sensor_data_merge.rs
//!
//! "Sensor data merge" — объединяет пару файлов фракционной эффективности
//! (uid_up.frac.pal / uid_down.frac.pal, экспорт вида *SP1*/*SP2* или *_up*/*_down*),
//! в одном из которых заполнена только колонка `dCmup [mg/m³]`, а во втором —
//! только `dCmdown [mg/m³]`, в единый TXT файл, пригодный для дальнейшей
//! обработки (эквивалент по структуре штатному "слитому" отчёту прибора).
//!
//! Формат входных/выходных файлов — TSV (табуляция), CRLF, с текстовой
//! "шапкой" метаданных измерения и таблицей вида:
//!
//! Xu [µm]\tX [µm]\tXo [µm]\tdX [µm]\t \tdCmup [mg/m³]\tdCmdown [mg/m³]\t \tP [%]\tE [%]
//!
//! Строки таблицы объединяются по индексу (порядковому номеру канала),
//! с проверкой совпадения границ каналов (Xu/X/Xo/dX) между двумя файлами.
//! P (проскок) и E (эффективность) пересчитываются заново по объединённым
//! значениям dCmup/dCmdown.

use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Заголовок строки таблицы данных — по нему находим начало табличной части.
const TABLE_HEADER_PREFIX: &str = "Xu [";

#[derive(Debug)]
pub enum MergeError {
    Io(std::io::Error),
    Parse(String),
    BinMismatch { row: usize, up: String, down: String },
    RowCountMismatch { up: usize, down: usize },
}

impl fmt::Display for MergeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MergeError::Io(e) => write!(f, "ошибка ввода/вывода: {e}"),
            MergeError::Parse(msg) => write!(f, "ошибка разбора файла: {msg}"),
            MergeError::BinMismatch { row, up, down } => write!(
                f,
                "несовпадение границ канала в строке {row}: up=[{up}] down=[{down}]"
            ),
            MergeError::RowCountMismatch { up, down } => write!(
                f,
                "разное число строк данных: up={up}, down={down}"
            ),
        }
    }
}

impl Error for MergeError {}

impl From<std::io::Error> for MergeError {
    fn from(e: std::io::Error) -> Self {
        MergeError::Io(e)
    }
}

/// Один канал (бин) таблицы фракционной эффективности.
#[derive(Debug, Clone)]
struct BinRow {
    xu: String,
    x: String,
    xo: String,
    dx: String,
    d_cm_up: f64,
    d_cm_down: f64,
}

impl BinRow {
    fn bin_key(&self) -> String {
        format!("{}\t{}\t{}\t{}", self.xu, self.x, self.xo, self.dx)
    }
}

/// Разобранный файл фракционной эффективности: шапка + таблица.
struct FracEffFile {
    header_lines: Vec<String>,
    rows: Vec<BinRow>,
}

fn read_lines(path: &Path) -> Result<Vec<String>, MergeError> {
    let raw = fs::read(path)?;
    let text = String::from_utf8_lossy(&raw);
    Ok(text
        .replace("\r\n", "\n")
        .split('\n')
        .map(|l| l.to_string())
        .collect())
}

fn parse_frac_eff_file(path: &Path) -> Result<FracEffFile, MergeError> {
    let lines = read_lines(path)?;

    let header_idx = lines
        .iter()
        .position(|l| l.trim_start().starts_with(TABLE_HEADER_PREFIX))
        .ok_or_else(|| {
            MergeError::Parse(format!(
                "{}: не найдена строка заголовка таблицы (начинается с '{}')",
                path.display(),
                TABLE_HEADER_PREFIX
            ))
        })?;

    let header_lines: Vec<String> = lines[..=header_idx].to_vec();

    let mut rows = Vec::new();
    for (offset, line) in lines[header_idx + 1..].iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 7 {
            return Err(MergeError::Parse(format!(
                "{}: строка {} содержит меньше колонок, чем ожидалось: '{}'",
                path.display(),
                header_idx + 2 + offset,
                line
            )));
        }
        let parse_f64 = |s: &str, col: &str| -> Result<f64, MergeError> {
            s.trim().replace(',', ".").parse::<f64>().map_err(|_| {
                MergeError::Parse(format!(
                    "{}: не удалось разобрать число в колонке '{}': '{}'",
                    path.display(),
                    col,
                    s
                ))
            })
        };

        rows.push(BinRow {
            xu: cols[0].trim().to_string(),
            x: cols[1].trim().to_string(),
            xo: cols[2].trim().to_string(),
            dx: cols[3].trim().to_string(),
            d_cm_up: parse_f64(cols[5], "dCmup")?,
            d_cm_down: parse_f64(cols[6], "dCmdown")?,
        });
    }

    Ok(FracEffFile { header_lines, rows })
}

/// Пересчитывает проскок P[%] и эффективность E[%] по объединённым значениям.
fn compute_p_e(d_cm_up: f64, d_cm_down: f64) -> (f64, f64) {
    if d_cm_up == 0.0 && d_cm_down == 0.0 {
        (f64::NAN, f64::NAN)
    } else if d_cm_up == 0.0 && d_cm_down > 0.0 {
        (200.0, -100.0)
    } else {
        let p = d_cm_down / (d_cm_up + d_cm_down) * 100.0;
        let e = 100.0 - p;
        (p, e)
    }
}

fn fmt_pct(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else {
        format!("{:.6}", v)
    }
}

/// Основная точка входа инструмента: принимает путь к файлу "up" (заполнена
/// dCmup) и файлу "down" (заполнена dCmdown) и путь выходного файла.
pub fn merge_sensor_data(
    up_path: &Path,
    down_path: &Path,
    out_path: &Path,
) -> Result<(), MergeError> {
    let up = parse_frac_eff_file(up_path)?;
    let down = parse_frac_eff_file(down_path)?;

    if up.rows.len() != down.rows.len() {
        return Err(MergeError::RowCountMismatch {
            up: up.rows.len(),
            down: down.rows.len(),
        });
    }

    let mut merged_rows = Vec::with_capacity(up.rows.len());
    for (i, (u, d)) in up.rows.iter().zip(down.rows.iter()).enumerate() {
        if u.bin_key() != d.bin_key() {
            return Err(MergeError::BinMismatch {
                row: i + 1,
                up: u.bin_key(),
                down: d.bin_key(),
            });
        }
        merged_rows.push(BinRow {
            xu: u.xu.clone(),
            x: u.x.clone(),
            xo: u.xo.clone(),
            dx: u.dx.clone(),
            d_cm_up: u.d_cm_up,
            d_cm_down: d.d_cm_down,
        });
    }

    let mut out = String::new();
    for line in &up.header_lines {
        out.push_str(line);
        out.push_str("\r\n");
    }

    out.push_str(&format!(
        "# sensor data merge: dCmup <- {}, dCmdown <- {}\r\n",
        up_path.file_name().unwrap_or_default().to_string_lossy(),
        down_path.file_name().unwrap_or_default().to_string_lossy(),
    ));

    for row in &merged_rows {
        let (p, e) = compute_p_e(row.d_cm_up, row.d_cm_down);
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t \t{:.3}\t{:.3}\t \t{}\t{}\r\n",
            row.xu,
            row.x,
            row.xo,
            row.dx,
            row.d_cm_up,
            row.d_cm_down,
            fmt_pct(p),
            fmt_pct(e),
        ));
    }

    fs::write(out_path, out)?;
    Ok(())
}

/// Определяет имя выходного файла по паре входных, если пользователь не
/// задал его явно (используется общий префикс имени файла).
pub fn default_output_path(up_path: &Path, down_path: &Path) -> PathBuf {
    let stem_up = up_path.file_stem().and_then(|s| s.to_str()).unwrap_or("up");
    let stem_down = down_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("down");

    let common: String = stem_up
        .chars()
        .zip(stem_down.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a)
        .collect();
    let common = common.trim_end_matches(['_', '-']).to_string();
    let base = if common.is_empty() { stem_up } else { &common };

    let dir = up_path.parent().unwrap_or_else(|| Path::new("."));
    dir.join(format!("{base}_merged.txt"))
}

/// Точка входа для регистрации в TUI-меню проекта (`src/tools/mod.rs`).
/// Использует `text_io` крейта проекта для интерактивного ввода путей —
/// приведите имена функций к тем, что реально экспортирует ваш `text_io.rs`.
pub fn run() -> Result<(), Box<dyn Error>> {
    use crate::text_io::prompt;

    println!("=== Sensor data merge (dCmup + dCmdown -> единый TXT) ===");
    let up_str = prompt("Путь к файлу с данными dCmup (upstream): ")?;
    let down_str = prompt("Путь к файлу с данными dCmdown (downstream): ")?;
    let out_str = prompt("Путь выходного файла (Enter — сгенерировать автоматически): ")?;

    let up_path = PathBuf::from(up_str.trim());
    let down_path = PathBuf::from(down_str.trim());
    let out_path = if out_str.trim().is_empty() {
        default_output_path(&up_path, &down_path)
    } else {
        PathBuf::from(out_str.trim())
    };

    merge_sensor_data(&up_path, &down_path, &out_path)?;
    println!("Готово: объединённые данные записаны в {}", out_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_p_e_both_zero_is_nan() {
        let (p, e) = compute_p_e(0.0, 0.0);
        assert!(p.is_nan() && e.is_nan());
    }

    #[test]
    fn compute_p_e_up_zero_down_positive_is_flagged() {
        let (p, e) = compute_p_e(0.0, 0.5);
        assert_eq!(p, 200.0);
        assert_eq!(e, -100.0);
    }

    #[test]
    fn compute_p_e_normal_case() {
        let (p, e) = compute_p_e(0.99, 0.01);
        assert!((p - 1.0).abs() < 1e-9);
        assert!((e - 99.0).abs() < 1e-9);
    }

    #[test]
    fn default_output_path_uses_common_prefix() {
        let up = Path::new("FEG-PN1-10_SP1_1000lpm.txt");
        let down = Path::new("FEG-PN1-10_SP2_1000lpm.txt");
        let out = default_output_path(up, down);
        assert_eq!(out, PathBuf::from("FEG-PN1-10_SP_merged.txt"));
    }
}
