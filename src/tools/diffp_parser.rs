use crate::config::AppConfig;
use crate::text_io::read_text_lossy;
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
    let content = read_text_lossy(path)?;
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

/// Линейная регрессия методом наименьших квадратов: y = slope*x + intercept.
fn linear_regression(x: &[f64], y: &[f64]) -> (f64, f64) {
    let n = x.len() as f64;
    let sum_x: f64 = x.iter().sum();
    let sum_y: f64 = y.iter().sum();
    let sum_xy: f64 = x.iter().zip(y).map(|(a, b)| a * b).sum();
    let sum_xx: f64 = x.iter().map(|a| a * a).sum();
    let denom = n * sum_xx - sum_x * sum_x;
    if denom.abs() < f64::EPSILON {
        return (0.0, sum_y / n);
    }
    let slope = (n * sum_xy - sum_x * sum_y) / denom;
    let intercept = (sum_y - slope * sum_x) / n;
    (slope, intercept)
}

/// Коэффициент детерминации R^2 для линейной регрессии.
fn r_squared(x: &[f64], y: &[f64], slope: f64, intercept: f64) -> f64 {
    let mean_y: f64 = y.iter().sum::<f64>() / y.len() as f64;
    let ss_tot: f64 = y.iter().map(|v| (v - mean_y).powi(2)).sum();
    let ss_res: f64 = x
        .iter()
        .zip(y.iter())
        .map(|(xi, yi)| {
            let pred = slope * xi + intercept;
            (yi - pred).powi(2)
        })
        .sum();
    if ss_tot.abs() < f64::EPSILON {
        1.0
    } else {
        1.0 - ss_res / ss_tot
    }
}

/// Перевод нормальных л/мин -> фм3/(м2*ч) с учётом температуры, абс. давления
/// в контуре и площади фильтроэлемента (порт из diffP_parser.py).
/// Нормальные условия: T_std = 0°C (273.15 K), P_std = 1.01325 бар.
fn conv_factor_nlmin_to_fm3m2h(temp_c: f64, p_abs_bar: f64, area_m2: f64) -> f64 {
    const T_STD: f64 = 273.15;
    const P_STD: f64 = 1.01325;
    let t_act = temp_c + 273.15;
    0.06 * (P_STD / p_abs_bar) * (t_act / T_STD) / area_m2
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

    println!();
    println!("Для верхней оси X в фм3/(м2*ч) укажите условия испытания:");
    let p_abs_bar: f64 = Input::new()
        .with_prompt("Давление в контуре (бар, абс.)")
        .default(1.01325)
        .interact_text()
        .unwrap_or(1.01325);

    let temp_c: f64 = Input::new()
        .with_prompt("температура в контуре (°C)")
        .default(20.0)
        .interact_text()
        .unwrap_or(20.0);

    let area_m2: f64 = Input::new()
        .with_prompt("площадь фильтроэлемента (м2)")
        .interact_text()
        .unwrap_or(0.0);

    if p_abs_bar <= 0.0 || area_m2 <= 0.0 {
        println!("Давление и площадь должны быть больше нуля. Отменено.");
        return;
    }

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
    if let Err(e) = plot_data(&flows, &pressures, &label, temp_c, p_abs_bar, area_m2, &output_path) {
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
    temp_c: f64,
    p_abs_bar: f64,
    area_m2: f64,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = BitMapBackend::new(output_path, (1100, 700)).into_drawing_area();
    root.fill(&WHITE)?;

    let x_max = flows.iter().cloned().fold(f64::MIN, f64::max).max(1.0);
    let y_max = pressures.iter().cloned().fold(f64::MIN, f64::max);
    let y_min = pressures.iter().cloned().fold(f64::MAX, f64::min).min(0.0);

    let (slope, intercept) = linear_regression(flows, pressures);
    let r2 = r_squared(flows, pressures, slope, intercept);
    let conv_factor = conv_factor_nlmin_to_fm3m2h(temp_c, p_abs_bar, area_m2);

    let chart = ChartBuilder::on(&root)
        .margin(20)
        .x_label_area_size(45)
        .y_label_area_size(70)
        .right_y_label_area_size(85)
        .top_x_label_area_size(45)
        .build_cartesian_2d(0f64..x_max, y_min..y_max)?;

    let mut chart = chart.set_secondary_coord(
        0f64..(x_max * conv_factor),
        (y_min / 1e6)..(y_max / 1e6),
    );

    chart
        .configure_mesh()
        .x_desc("Flow (l/min)")
        .y_desc("Differential Pressure (Pa)")
        .x_label_formatter(&|v| format!("{:.0}", v))
        .y_label_formatter(&|v| format!("{:.0}", v))
        .draw()?;

    chart
        .configure_secondary_axes()
        .x_desc("Flow (fm3/(m2*h))")
        .y_desc("Differential Pressure (MPa)")
        .x_label_formatter(&|v| format!("{:.0}", v))
        .y_label_formatter(&|v| format!("{:.4}", v))
        .draw()?;

    chart
        .draw_series(LineSeries::new(
            flows.iter().zip(pressures.iter()).map(|(x, y)| (*x, *y)),
            &RED,
        ))?
        .label(label)
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));

    chart.draw_series(
        flows
            .iter()
            .zip(pressures.iter())
            .map(|(x, y)| Circle::new((*x, *y), 3, RED.filled())),
    )?;

    let trend_label = format!("y = {:.2}x + {:.2} (R2 = {:.4})", slope, intercept, r2);

    chart
        .draw_series(LineSeries::new(
            vec![(0f64, intercept), (x_max, slope * x_max + intercept)],
            &BLUE,
        ))?
        .label(trend_label)
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLUE));

    chart
        .configure_series_labels()
        .position(SeriesLabelPosition::LowerRight)
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()?;

    root.present()?;
    Ok(())
}
