//! Sensor data merge — объединяет пару файлов фракционной эффективности
//! (экспорт вида *SP1*/*SP2*, *_up*/*_down*), в одном из которых заполнена
//! только колонка dCmup (upstream), а в другом — только dCmdown (downstream),
//! в единый TXT-файл, пригодный для дальнейшей обработки.
//!
//! Формат входных/выходных файлов совпадает с экспортом прибора: текстовая
//! "шапка" метаданных измерения, затем таблица вида
//! Xu [µm]\tX [µm]\tXo [µm]\tdX [µm]\t \tdCmup [mg/m³]\tdCmdown [mg/m³]\t \tP [%]\tE [%]
//!
//! Строки объединяются по индексу канала, с проверкой совпадения границ
//! канала (Xu/X/Xo/dX) между двумя файлами. Проскок P[%] и эффективность
//! E[%] пересчитываются заново по объединённым значениям dCmup/dCmdown.

use crate::config::AppConfig;
use crate::text_io::read_text_lossy;
use dialoguer::{Input, MultiSelect};
use std::fs;
use std::path::{Path, PathBuf};

fn find_col(fields: &[&str], target: &str) -> Option<usize> {
    fields.iter().position(|f| {
        let t = f.trim();
        if let Some(rest) = t.strip_prefix(target) {
            rest.starts_with(' ') || rest.starts_with('[')
        } else {
            false
        }
    })
}

#[derive(Clone)]
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

struct FracTable {
    header_lines: Vec<String>,
    rows: Vec<BinRow>,
}

fn parse_table(path: &Path) -> Option<FracTable> {
    let content = read_text_lossy(path)?;
    let lines: Vec<&str> = content.lines().collect();

    let mut xu_idx = None;
    let mut x_idx = None;
    let mut xo_idx = None;
    let mut dx_idx = None;
    let mut up_idx = None;
    let mut down_idx = None;
    let mut header_line = None;

    for (i, line) in lines.iter().enumerate() {
        let fields: Vec<&str> = line.split('\t').collect();
        if let (Some(u), Some(d)) = (find_col(&fields, "dCmup"), find_col(&fields, "dCmdown")) {
            xu_idx = find_col(&fields, "Xu");
            x_idx = find_col(&fields, "X");
            xo_idx = find_col(&fields, "Xo");
            dx_idx = find_col(&fields, "dX");
            up_idx = Some(u);
            down_idx = Some(d);
            header_line = Some(i);
            break;
        }
    }

    let (xu_i, x_i, xo_i, dx_i, up_i, down_i, hl) =
        match (xu_idx, x_idx, xo_idx, dx_idx, up_idx, down_idx, header_line) {
            (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f), Some(h)) => {
                (a, b, c, d, e, f, h)
            }
            _ => {
                println!(
                    "{}: не найдены колонки Xu/X/Xo/dX/dCmup/dCmdown в таблице.",
                    path.display()
                );
                return None;
            }
        };

    let header_lines: Vec<String> = lines[..=hl].iter().map(|l| l.to_string()).collect();

    let max_idx = xu_i.max(x_i).max(xo_i).max(dx_i).max(up_i).max(down_i);
    let mut rows = Vec::new();
    for line in lines.iter().skip(hl + 1) {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() <= max_idx {
            continue;
        }
        let up_val = fields[up_i].trim().replace(',', ".").parse::<f64>();
        let down_val = fields[down_i].trim().replace(',', ".").parse::<f64>();
        if let (Ok(d_cm_up), Ok(d_cm_down)) = (up_val, down_val) {
            rows.push(BinRow {
                xu: fields[xu_i].trim().to_string(),
                x: fields[x_i].trim().to_string(),
                xo: fields[xo_i].trim().to_string(),
                dx: fields[dx_i].trim().to_string(),
                d_cm_up,
                d_cm_down,
            });
        }
    }

    if rows.is_empty() {
        println!("{}: не найдено ни одной строки с данными.", path.display());
        return None;
    }

    Some(FracTable { header_lines, rows })
}

fn compute_p_e(d_cm_up: f64, d_cm_down: f64) -> (f64, f64) {
    if d_cm_up == 0.0 && d_cm_down == 0.0 {
        (f64::NAN, f64::NAN)
    } else if d_cm_up == 0.0 && d_cm_down > 0.0 {
        (200.0, -100.0)
    } else {
        let p = d_cm_down / (d_cm_up + d_cm_down) * 100.0;
        (p, 100.0 - p)
    }
}

fn fmt_pct(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else {
        format!("{:.6}", v)
    }
}

fn select_two_files(config: &AppConfig) -> Option<(PathBuf, PathBuf)> {
    let dir = Path::new(&config.input_dir);

    if !dir.exists() || !dir.is_dir() {
        println!("Папка с исходными данными не найдена: {}", dir.display());
        println!("Проверьте путь в меню \"Настройка папок\".");
        return None;
    }

    let mut files: Vec<String> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect(),
        Err(e) => {
            println!("Не удалось открыть папку {}: {}", dir.display(), e);
            return None;
        }
    };

    if files.len() < 2 {
        println!(
            "В папке с исходными данными нужно минимум два файла: {}",
            dir.display()
        );
        return None;
    }

    files.sort();

    loop {
        let selections = MultiSelect::new()
            .with_prompt("Выберите ровно два файла для объединения (Space — выбрать/снять, Enter — подтвердить)")
            .items(&files)
            .interact()
            .ok()?;

        match selections.len() {
            2 => {
                let a = dir.join(&files[selections[0]]);
                let b = dir.join(&files[selections[1]]);
                return Some((a, b));
            }
            0 => {
                println!("Файлы не выбраны. Возврат в главное меню.");
                return None;
            }
            n => {
                println!(
                    "Нужно выбрать ровно два файла (сейчас выбрано: {}). Попробуйте ещё раз.",
                    n
                );
                continue;
            }
        }
    }
}

/// Определяет, какой из двух файлов содержит данные upstream (dCmup), а какой —
/// downstream (dCmdown), по сумме значений соответствующей колонки: в файле "upstream"
/// сумма dCmup заведомо больше суммы dCmup во втором файле (там эта колонка нулевая),
/// и наоборот для dCmdown.
fn order_up_down(a: (PathBuf, FracTable), b: (PathBuf, FracTable)) -> ((PathBuf, FracTable), (PathBuf, FracTable)) {
    let sum_up_a: f64 = a.1.rows.iter().map(|r| r.d_cm_up).sum();
    let sum_up_b: f64 = b.1.rows.iter().map(|r| r.d_cm_up).sum();

    if sum_up_a >= sum_up_b {
        (a, b)
    } else {
        (b, a)
    }
}

fn read_text_or_default(prompt: &str, default: &str) -> Option<String> {
    let input: String = Input::new()
        .with_prompt(prompt)
        .default(default.to_string())
        .interact_text()
        .unwrap_or_else(|_| default.to_string());

    if input.trim().eq_ignore_ascii_case("q") {
        None
    } else {
        Some(input)
    }
}

fn default_output_name(up_path: &Path, down_path: &Path) -> String {
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
    let base = if common.is_empty() { stem_up.to_string() } else { common };

    format!("{base}_merged.txt")
}

pub fn run(config: &AppConfig) {
    println!("Объединение данных датчиков (dCmup/dCmdown)");
    println!("--------------------------------------------");

    let (path_a, path_b) = match select_two_files(config) {
        Some(v) => v,
        None => return,
    };

    let table_a = match parse_table(&path_a) {
        Some(t) => t,
        None => {
            println!("Не удалось разобрать файл {}", path_a.display());
            return;
        }
    };
    let table_b = match parse_table(&path_b) {
        Some(t) => t,
        None => {
            println!("Не удалось разобрать файл {}", path_b.display());
            return;
        }
    };

    let ((up_path, up_table), (down_path, down_table)) =
        order_up_down((path_a, table_a), (path_b, table_b));

    println!(
        "Определено автоматически: dCmup <- {}, dCmdown <- {}",
        up_path.file_name().unwrap_or_default().to_string_lossy(),
        down_path.file_name().unwrap_or_default().to_string_lossy(),
    );

    if up_table.rows.len() != down_table.rows.len() {
        println!(
            "Разное число строк данных: {} (dCmup) = {}, {} (dCmdown) = {}",
            up_path.display(),
            up_table.rows.len(),
            down_path.display(),
            down_table.rows.len()
        );
        return;
    }

    let mut merged_rows = Vec::with_capacity(up_table.rows.len());
    for (i, (u, d)) in up_table.rows.iter().zip(down_table.rows.iter()).enumerate() {
        if u.bin_key() != d.bin_key() {
            println!(
                "Несовпадение границ канала в строке {}: dCmup=[{}] dCmdown=[{}]",
                i + 1,
                u.bin_key(),
                d.bin_key()
            );
            return;
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

    let default_name = default_output_name(&up_path, &down_path);
    let output_name = match read_text_or_default(
        "Введите имя выходного файла (Enter — использовать предложенное, q — отмена)",
        &default_name,
    ) {
        Some(v) => v,
        None => {
            println!("Отменено. Возврат в главное меню.");
            return;
        }
    };

    let mut out = String::new();
    for line in &up_table.header_lines {
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
            row.xu, row.x, row.xo, row.dx, row.d_cm_up, row.d_cm_down, fmt_pct(p), fmt_pct(e),
        ));
    }

    if let Err(e) = config.ensure_output_dir() {
        println!("Не удалось создать папку для результатов: {}", e);
        return;
    }

    let output_path = config.output_path(&output_name);
    if let Err(e) = fs::write(&output_path, out) {
        println!("Ошибка записи файла: {}", e);
        return;
    }

    println!();
    println!("Готово: объединённые данные записаны в {}", output_path.display());
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
    fn default_output_name_uses_common_prefix() {
        let up = Path::new("FEG-PN1-10_SP1_1000lpm.txt");
        let down = Path::new("FEG-PN1-10_SP2_1000lpm.txt");
        assert_eq!(default_output_name(up, down), "FEG-PN1-10_SP_merged.txt");
    }

    #[test]
    fn order_up_down_picks_larger_up_sum_first() {
        let table_up = FracTable {
            header_lines: vec![],
            rows: vec![BinRow {
                xu: "0.316".into(),
                x: "0.328".into(),
                xo: "0.340".into(),
                dx: "0.024".into(),
                d_cm_up: 1.0,
                d_cm_down: 0.0,
            }],
        };
        let table_down = FracTable {
            header_lines: vec![],
            rows: vec![BinRow {
                xu: "0.316".into(),
                x: "0.328".into(),
                xo: "0.340".into(),
                dx: "0.024".into(),
                d_cm_up: 0.0,
                d_cm_down: 1.0,
            }],
        };
        let a = (PathBuf::from("down.txt"), table_down);
        let b = (PathBuf::from("up.txt"), table_up);
        let (up, down) = order_up_down(a, b);
        assert_eq!(up.0, PathBuf::from("up.txt"));
        assert_eq!(down.0, PathBuf::from("down.txt"));
    }
}
