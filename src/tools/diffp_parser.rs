use crate::config::AppConfig;
use dialoguer::{Input, Select};
use plotters::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

fn select_input_file(config: &AppConfig) -> Option<PathBuf> {
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
    files.push("[Отмена — назад в меню]".to_string());

    let selection = Select::new()
        .with_prompt("Выберите файл с данными")
        .items(&files)
        .default(0)
        .interact()
        .ok()?;

    if selection == files.len() - 1 {
        println!("Отменено. Возврат в главное меню.");
        return None;
    }

    Some(dir.join(&files[selection]))
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

fn parse_diffp_file(path: &Path) -> Option<(Vec<f64>, Vec<f64>)> {
    let content = fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = content.lines().collect();

    let mut flow_idx: Option<usize> = None;
    let mut press_idx: Option<usize> = None;
    let mut header_line: Option<usize> = None;

    for (i, line) in lines.iter().enumerate() {
        let fields: Vec<&str> = line.split('\t').collect();
        let fi = fields.iter().position(|f| f.contains("main air") && f.contains("l/min"));
        let pi = fields.iter().position(|f| f.contains("P_diff"));
        if let (Some(fi), Some(pi)) = (fi, pi) {
            flow_idx = Some(fi);
            press_idx = Some(pi);
            header_line = Some(i);
            break;
        }
    }

    let (fi, pi, header_i) = match (flow_idx, press_idx, header_line) {
        (Some(a), Some(b), Some(h)) => (a, b, h),
        _ => return None,
    };

    let mut flows = Vec::new();
    let mut pressures = Vec::new();

    for line in lines.iter().skip(header_i + 1) {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.is_empty() {
            continue;
        }
        let first = fields[0].trim().to_lowercase();
        if !first.starts_with("dpk") {
            continue;
        }
        if fields.len() <= fi.max(pi) {
            continue;
        }
        let flow_val = fields[fi].trim().replace(',', ".").parse::<f64>();
        let press_val = fields[pi].trim().replace(',', ".").parse::<f64>();
        if let (Ok(f), Ok(p)) = (flow_val, press_val) {
            flows.push(f);
            pressures.push(p);
        }
    }

    if flows.len() < 2 {
        return None;
    }
    Some((flows, pressures))
}

pub fn run(config: &AppConfig) {
    println!("Парсер дифференциального давления");
    println!("------------------------------------");

    let input_path = match select_input_file(config) {
        Some(p) => p,
        None => return,
    };

    let (flows, pressures) = match parse_diffp_file(&input_path) {
        Some(v) => v,
        None => {
            println!(
                "Не удалось найти данные дифференциального давления в файле {}.",
                input_path.display()
            );
            println!("Файл должен содержать столбцы 'P_diff [Pa]' и 'main air#1 [l/min]'.");
            return;
        }
    };

    println!("Обработано точек: {}", flows.len());

    let default_label = input_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Series".to_string());

    let label = match read_text_or_default("Введите название для легенды графика", &default_label) {
        Some(v) => v,
        None => {
            println!("Отменено. Возврат в главное меню.");
            return;
        }
    };

    let mut output_name = match read_text_or_default(
        "Введите имя файла для сохранения графика (PNG)",
        "diffp_result.png",
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
    if let Err(e) = plot_data(&flows, &pressures, &label, &output_path) {
        println!("Ошибка построения графика: {}", e);
        return;
    }

    println!();
    println!("График сохранён: {}", output_path.display());
}

fn plot_data(
    flows: &[f64],
    pressures: &[f64],
    label: &str,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = BitMapBackend::new(output_path, (1000, 600)).into_drawing_area();
    root.fill(&WHITE)?;

    let x_max = flows.iter().cloned().fold(f64::MIN, f64::max);
    let y_max = pressures.iter().cloned().fold(f64::MIN, f64::max);
    let y_min = pressures.iter().cloned().fold(f64::MAX, f64::min);

    let mut chart = ChartBuilder::on(&root)
        .caption("Differential Pressure", ("sans-serif", 30))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0f64..x_max, y_min..y_max)?;

    chart
        .configure_mesh()
        .x_desc("Flow (l/min)")
        .y_desc("Differential Pressure (Pa)")
        .draw()?;

    chart
        .draw_series(LineSeries::new(
            flows.iter().zip(pressures.iter()).map(|(x, y)| (*x, *y)),
            &RED,
        ))?
        .label(label)
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));

    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()?;

    root.present()?;
    Ok(())
}
