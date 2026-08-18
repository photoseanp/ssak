use crate::config::AppConfig;
use crate::text_io::read_text_lossy;
use dialoguer::{Input, MultiSelect};
use plotters::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

fn select_input_files(config: &AppConfig) -> Option<Vec<PathBuf>> {
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

    if files.is_empty() {
        println!("В папке с исходными данными не найдено файлов: {}", dir.display());
        return None;
    }

    files.sort();

    let selections = MultiSelect::new()
        .with_prompt("Выберите один или несколько файлов (Space — выбрать, Enter — подтвердить)")
        .items(&files)
        .interact()
        .ok()?;

    if selections.is_empty() {
        println!("Файлы не выбраны. Возврат в главное меню.");
            return None;
    }

    Some(selections.into_iter().map(|i| dir.join(&files[i])).collect())
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

fn parse_frac_eff_file(path: &Path) -> Option<(Vec<f64>, Vec<f64>)> {
    let content = read_text_lossy(path)?;
    let lines: Vec<&str> = content.lines().collect();

    let mut size_idx: Option<usize> = None;
    let mut eff_idx: Option<usize> = None;
    let mut header_line: Option<usize> = None;

    for (i, line) in lines.iter().enumerate() {
        let fields: Vec<&str> = line.split('\t').collect();
        let si = find_col(&fields, "X");
        let ei = find_col(&fields, "E");
        if let (Some(si), Some(ei)) = (si, ei) {
            size_idx = Some(si);
            eff_idx = Some(ei);
            header_line = Some(i);
            break;
        }
    }

    let (si, ei, hl) = match (size_idx, eff_idx, header_line) {
        (Some(a), Some(b), Some(h)) => (a, b, h),
        _ => return None,
    };

    let mut sizes = Vec::new();
    let mut effs = Vec::new();

    for line in lines.iter().skip(hl + 1) {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() <= si.max(ei) {
            continue;
        }
        let size_val = fields[si].trim().replace(',', ".").parse::<f64>();
        let eff_val = fields[ei].trim().replace(',', ".").parse::<f64>();
        if let (Ok(s), Ok(e)) = (size_val, eff_val) {
            sizes.push(s);
            effs.push(e);
        }
    }

    if sizes.len() < 2 {
        return None;
    }
    Some((sizes, effs))
}

pub fn run(config: &AppConfig) {
    println!("Сравнение фракционной эффективности");
    println!("--------------------------------------");

    let paths = match select_input_files(config) {
        Some(p) => p,
        None => return,
    };

    let mut series: Vec<(String, Vec<f64>, Vec<f64>)> = Vec::new();

    for path in &paths {
        match parse_frac_eff_file(path) {
            Some((sizes, effs)) => {
                let default_label = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Series".to_string());
                let prompt = format!(
                    "Название для легенды (файл: {})",
                    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                );
                let label = match read_text_or_default(&prompt, &default_label) {
                    Some(v) => v,
                    None => {
                        println!("Отменено. Возврат в главное меню.");
                        return;
                    }
                };
                series.push((label, sizes, effs));
            }
            None => {
                println!(
                    "Не удалось найти данные фракционной эффективности в файле {} — файл пропущен.",
                    path.display()
                );
            }
        }
    }

    if series.is_empty() {
        println!("Не удалось обработать ни один из выбранных файлов.");
        return;
    }

    let mut output_name = match read_text_or_default(
        "Введите имя файла для сохранения графика (PNG)",
        "frac_eff_result.png",
    ) {
        Some(v) => v,
        None => {
            println!("Отменено. Возврат в главное меню.");
            return;
        }
    };
    if !output_name.to_lowercase().ends_with(".png") {
        output_name.push_str(".png");
    }

    if let Err(e) = config.ensure_output_dir() {
        println!("Не удалось создать папку для результатов: {}", e);
        return;
    }

    let output_path = config.output_path(&output_name);
    if let Err(e) = plot_data(&series, &output_path) {
        println!("Ошибка построения графика: {}", e);
        return;
    }

    println!();
    println!("Обработано файлов: {}", series.len());
    println!("График сохранён: {}", output_path.display());
}

fn plot_data(
    series: &[(String, Vec<f64>, Vec<f64>)],
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = BitMapBackend::new(output_path, (1000, 600)).into_drawing_area();
    root.fill(&WHITE)?;

    let x_max = series
        .iter()
        .flat_map(|(_, s, _)| s.iter())
        .cloned()
        .fold(f64::MIN, f64::max);
    let x_min = series
        .iter()
        .flat_map(|(_, s, _)| s.iter())
        .cloned()
        .fold(f64::MAX, f64::min);

    let mut chart = ChartBuilder::on(&root)
        .caption("Fractional Efficiency", ("sans-serif", 30))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(50)
        .build_cartesian_2d(x_min..x_max, 0f64..100f64)?;

    chart
        .configure_mesh()
        .x_desc("Particle Size")
        .y_desc("Efficiency, %")
        .draw()?;

    let palette: [&RGBColor; 6] = [&RED, &BLUE, &GREEN, &MAGENTA, &CYAN, &BLACK];

    for (i, (label, sizes, effs)) in series.iter().enumerate() {
        let color = palette[i % palette.len()];
        chart
            .draw_series(LineSeries::new(
                sizes.iter().zip(effs.iter()).map(|(x, y)| (*x, *y)),
                color,
            ))?
            .label(label.clone())
            .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], color));
    }

    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()?;

    root.present()?;
    Ok(())
}
