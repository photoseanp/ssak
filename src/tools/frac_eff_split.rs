// src/tools/frac_eff_split.rs
//
// Сравнение фракционной эффективности для раздельных сенсоров (SP1/SP2).
//
// В отличие от основного модуля фракционной эффективности (frac_eff.rs), где
// upstream и downstream концентрации содержатся в одном файле, здесь измерения
// сняты отдельно двумя сенсорами:
//   - SP1: файл содержит реальные значения dCmup (upstream), dCmdown = 0
//   - SP2: файл содержит реальные значения dCmdown (downstream), dCmup = 0
//
// Настоящая эффективность вычисляется путём объединения пары файлов SP1+SP2
// для одного и того же измерения (тот же фильтр и тот же расход):
//
//   E[%] = (1 - dCmdown_SP2 / dCmup_SP1) * 100
//
// ПРИМЕЧАНИЕ: этот файл пока НЕ подключён к src/tools/mod.rs и не вызывается
// из src/main.rs. Он самодостаточен и не влияет на текущую сборку. Чтобы
// включить новый пункт меню, нужно:
//   1. добавить строку `pub mod frac_eff_split;` в src/tools/mod.rs;
//   2. добавить пункт "Сравнение фракционной эффективности для раздельных
//      сенсоров" в список меню в src/main.rs и обработчик, вызывающий
//      tools::frac_eff_split::run(&config);
// См. RELEASE_NOTES_0.2.3.md для подробностей.
//
// Реализация предполагает, что AppConfig (src/config.rs) предоставляет поля
// input_dir/output_dir и методы ensure_output_dir()/output_path(&str) --
// как в исходной версии config.rs из первого коммита проекта. Если сигнатуры
// AppConfig изменились, эту часть нужно будет подправить при подключении.

use crate::config::AppConfig;
use dialoguer::Input;
use plotters::prelude::*;
use plotters::coord::ranged1d::LogScalable;
use std::fs;
use std::path::{Path, PathBuf};

struct ParsedFile {
    label: String,
    x: Vec<f64>,
    up: Vec<f64>,
    down: Vec<f64>,
}

fn list_txt_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext.to_string_lossy().to_lowercase() == "txt" {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    files
}

fn parse_measurement_file(path: &Path) -> Result<ParsedFile, String> {
    let raw = fs::read(path).map_err(|e| format!("не удалось открыть файл: {}", e))?;
    let content = String::from_utf8_lossy(&raw).into_owned();

    let lines: Vec<&str> = content.lines().collect();

    let mut label: Option<String> = None;
    for line in &lines {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.starts_with("rem.1:") {
            label = Some(trimmed["rem.1:".len()..].trim().to_string());
            break;
        }
    }
    let label = label.unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    });

    let mut header_idx: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.contains("Xu") && trimmed.contains("E [%]") {
            header_idx = Some(i + 1);
            break;
        }
    }

    let data_start = header_idx.ok_or_else(|| "не найден заголовок таблицы данных (Xu ... E [%])".to_string())?;

    let mut x_vals = Vec::new();
    let mut up_vals = Vec::new();
    let mut down_vals = Vec::new();

    for line in &lines[data_start..] {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.split('\t').collect();
        if parts.len() < 10 {
            continue;
        }

        let x: f64 = match parts[1].trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let up: f64 = match parts[5].trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let down: f64 = match parts[6].trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };

        x_vals.push(x);
        up_vals.push(up);
        down_vals.push(down);
    }

    if x_vals.is_empty() {
        return Err("не найдено ни одной валидной строки данных".to_string());
    }

    Ok(ParsedFile {
        label,
        x: x_vals,
        up: up_vals,
        down: down_vals,
    })
}

fn combined_label(sp1_label: &str) -> String {
    let cleaned = sp1_label
        .replace("_SP1", "")
        .replace("_sp1", "")
        .replace(" SP1", "")
        .replace(" sp1", "")
        .replace("SP1", "")
        .replace("sp1", "");
    let cleaned = cleaned.trim();
    let mut result = String::new();
    let mut prev_space = false;
    for ch in cleaned.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                result.push(' ');
            }
            prev_space = true;
        } else {
            result.push(ch);
            prev_space = false;
        }
    }
    result.trim().to_string()
}

fn parse_indices(input: &str, max_len: usize) -> Vec<usize> {
    input
        .split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
        .filter(|i| *i >= 1 && *i <= max_len)
        .map(|i| i - 1)
        .collect()
}

pub fn run(config: &AppConfig) {
    println!("Сравнение фракционной эффективности для раздельных сенсоров (SP1/SP2)");
    println!("-------------------------------------------------------------------------");

    let input_dir = Path::new(&config.input_dir);
    let available_files = list_txt_files(input_dir);

    if available_files.is_empty() {
        println!("В папке с исходными данными не найдено файлов .txt: {}", input_dir.display());
        return;
    }

    println!("\nДоступные файлы в папке с исходными данными:");
    for (i, f) in available_files.iter().enumerate() {
        println!("  {}. {}", i + 1, f.file_name().unwrap_or_default().to_string_lossy());
    }

    let sp1_input: String = Input::new()
        .with_prompt("\nВведите номера файлов SP1 (upstream), через запятую, в порядке соответствия")
        .interact_text()
        .unwrap_or_default();
    let sp1_indices = parse_indices(&sp1_input, available_files.len());

    if sp1_indices.is_empty() {
        println!("Не выбрано ни одного файла SP1.");
        return;
    }

    let sp2_input: String = Input::new()
        .with_prompt("Введите номера файлов SP2 (downstream), в том же порядке, что и SP1")
        .interact_text()
        .unwrap_or_default();
    let sp2_indices = parse_indices(&sp2_input, available_files.len());

    if sp2_indices.len() != sp1_indices.len() {
        println!(
            "Количество файлов SP1 ({}) не совпадает с количеством файлов SP2 ({}). Проверьте ввод.",
            sp1_indices.len(),
            sp2_indices.len()
        );
        return;
    }

    let mut data: Vec<(String, Vec<f64>, Vec<f64>)> = Vec::new();

    println!("\nОбработка пар файлов:");
    for (sp1_idx, sp2_idx) in sp1_indices.iter().zip(sp2_indices.iter()) {
        let sp1_path = &available_files[*sp1_idx];
        let sp2_path = &available_files[*sp2_idx];

        let sp1_parsed = match parse_measurement_file(sp1_path) {
            Ok(p) => p,
            Err(e) => {
                println!("  Ошибка чтения {}: {}", sp1_path.display(), e);
                continue;
            }
        };
        let sp2_parsed = match parse_measurement_file(sp2_path) {
            Ok(p) => p,
            Err(e) => {
                println!("  Ошибка чтения {}: {}", sp2_path.display(), e);
                continue;
            }
        };

        let n = sp1_parsed.x.len().min(sp2_parsed.down.len());
        if n == 0 {
            println!(
                "  Пропущена пара {} + {}: нет совпадающих точек данных",
                sp1_path.file_name().unwrap_or_default().to_string_lossy(),
                sp2_path.file_name().unwrap_or_default().to_string_lossy()
            );
            continue;
        }

        let mut x_out = Vec::new();
        let mut e_out = Vec::new();

        for i in 0..n {
            let up = sp1_parsed.up[i];
            let down = sp2_parsed.down[i];
            if up <= 0.0 {
                continue;
            }
            let e = (1.0 - down / up) * 100.0;
            if e.is_finite() {
                x_out.push(sp1_parsed.x[i]);
                e_out.push(e);
            }
        }

        if x_out.is_empty() {
            println!(
                "  Пропущена пара {} + {}: не удалось рассчитать эффективность",
                sp1_path.file_name().unwrap_or_default().to_string_lossy(),
                sp2_path.file_name().unwrap_or_default().to_string_lossy()
            );
            continue;
        }

        let label = combined_label(&sp1_parsed.label);
        println!(
            "  {} + {} -> \"{}\" ({} точек)",
            sp1_path.file_name().unwrap_or_default().to_string_lossy(),
            sp2_path.file_name().unwrap_or_default().to_string_lossy(),
            label,
            x_out.len()
        );

        data.push((label, x_out, e_out));
    }

    if data.is_empty() {
        println!("\nНе удалось рассчитать ни одной кривой эффективности.");
        return;
    }

    if let Err(e) = config.ensure_output_dir() {
        println!("Не удалось создать папку для результатов: {}", e);
        return;
    }

    let output_name: String = Input::new()
        .with_prompt("\nВведите имя файла для графика (без расширения, по умолчанию frac_eff_split_result)")
        .default("frac_eff_split_result".to_string())
        .interact_text()
        .unwrap_or_else(|_| "frac_eff_split_result".to_string());

    let output_path = config.output_path(&format!("{}.png", output_name));

    if let Err(e) = plot_data(&data, &output_path) {
        println!("Ошибка построения графика: {}", e);
        return;
    }

    println!("\nОбработано кривых: {}", data.len());
    println!("График сохранён: {}", output_path.display());
}

fn plot_data(
    data: &[(String, Vec<f64>, Vec<f64>)],
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = BitMapBackend::new(output_path, (1200, 800)).into_drawing_area();
    root.fill(&WHITE)?;

    let x_min = data
        .iter()
        .flat_map(|(_, x, _)| x.iter().cloned())
        .fold(f64::MAX, f64::min)
        .max(0.1);
    let x_max = data
        .iter()
        .flat_map(|(_, x, _)| x.iter().cloned())
        .fold(f64::MIN, f64::max);

    let e_min = data
        .iter()
        .flat_map(|(_, _, e)| e.iter().cloned())
        .fold(f64::MAX, f64::min);
    let e_max = data
        .iter()
        .flat_map(|(_, _, e)| e.iter().cloned())
        .fold(f64::MIN, f64::max);

    let mut chart = ChartBuilder::on(&root)
        .caption(
            "Фракционная эффективность (раздельные сенсоры SP1/SP2)",
            ("sans-serif", 28),
        )
        .margin(20)
        .x_label_area_size(45)
        .y_label_area_size(55)
        .build_cartesian_2d(
            (x_min..x_max).log_scale(),
            (e_min - 0.5)..(e_max + 0.5),
        )?;

    chart
        .configure_mesh()
        .x_desc("X, мкм")
        .y_desc("E, %")
        .draw()?;

    let palette = [
        &RED, &BLUE, &GREEN, &MAGENTA, &CYAN, &BLACK,
    ];

    for (i, (label, x, e)) in data.iter().enumerate() {
        let color = palette[i % palette.len()];
        chart
            .draw_series(LineSeries::new(
                x.iter().zip(e.iter()).map(|(a, b)| (*a, *b)),
                color,
            ))?
            .label(label.clone())
            .legend(move |(lx, ly)| PathElement::new(vec![(lx, ly), (lx + 20, ly)], *color));

        chart.draw_series(
            x.iter()
                .zip(e.iter())
                .map(|(a, b)| Circle::new((*a, *b), 3, color.filled())),
        )?;
    }

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()?;

    root.present()?;
    Ok(())
}
