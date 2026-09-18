use crate::config::AppConfig;
use crate::text_io::read_text_lossy;
use dialoguer::{Confirm, Input, MultiSelect, Select};
use plotters::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

/// Режим сравнения нескольких файлов дифференциального давления между собой:
/// по абсолютному расходу (без учёта площади фильтроэлемента) или по
/// удельному расходу, приведённому к 1 м² фильтроэлемента.
///
/// Подписи пунктов сознательно короткие (умещаются в одну строку терминала
/// без переноса) — при длинных подписях, переносящихся на вторую строку,
/// dialoguer::Select некорректно пересчитывает число строк для перерисовки
/// при переключении между вариантами, из-за чего строки меню "прыгают" и
/// частично стираются.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CompareMode {
    Absolute,
    PerSquareMeter,
}

impl CompareMode {
    fn label(self) -> &'static str {
        match self {
            CompareMode::Absolute => "По абсолютному расходу (без площади ФЭ)",
            CompareMode::PerSquareMeter => "По удельному расходу (на 1 м² ФЭ)",
        }
    }
}

fn select_compare_mode() -> Option<CompareMode> {
    let items = [CompareMode::Absolute.label(), CompareMode::PerSquareMeter.label()];
    let selection = Select::new()
        .with_prompt("Выбрано несколько файлов. Вид сравнения")
        .items(&items)
        .default(0)
        .interact()
        .ok()?;

    Some(if selection == 0 {
        CompareMode::Absolute
    } else {
        CompareMode::PerSquareMeter
    })
}

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
        .with_prompt("Выберите один или несколько файлов с данными (Space — выбрать, Enter — подтвердить)")
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

/// Сопротивление потоку пустого фильтродержателя (без фильтроэлемента) —
/// аппроксимация экспериментальной кривой полиномом 6-й степени по расходу
/// x [л/мин]:
/// y = -6e-18*x^6 + 7e-14*x^5 - 3e-10*x^4 + 5e-7*x^3 + 6e-5*x^2 + 0.1415*x + 174.57
/// Возвращает сопротивление держателя [Pa] для данного расхода. Вычисляется
/// схемой Горнера для устойчивости при больших степенях x.
fn holder_resistance_pa(flow_lmin: f64) -> f64 {
    let x = flow_lmin;
    (((((-6e-18 * x + 7e-14) * x - 3e-10) * x + 5e-7) * x + 6e-5) * x + 0.1415) * x + 174.57
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

    let paths = match select_input_files(config) {
        Some(p) => p,
        None => return,
    };

    if paths.len() == 1 {
        run_single(config, &paths[0]);
    } else {
        run_multi(config, &paths);
    }
}

fn run_single(config: &AppConfig, input_path: &Path) {
    let (flows, mut pressures) = match parse_diffp_file(input_path) {
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

    let subtract_holder = Confirm::new()
        .with_prompt("Вычесть сопротивление фильтродержателя из измеренного перепада давления?")
        .default(false)
        .interact()
        .unwrap_or(false);

    if subtract_holder {
        for (p, f) in pressures.iter_mut().zip(flows.iter()) {
            *p -= holder_resistance_pa(*f);
        }
        println!("Сопротивление фильтродержателя вычтено из всех точек (полином 6-й степени по расходу).");
    }

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

fn run_multi(config: &AppConfig, paths: &[PathBuf]) {
    let mut parsed: Vec<(PathBuf, Vec<f64>, Vec<f64>)> = Vec::new();
    for path in paths {
        match parse_diffp_file(path) {
            Some((flows, pressures)) => parsed.push((path.clone(), flows, pressures)),
            None => println!(
                "Не удалось найти данные дифференциального давления в файле {} — файл пропущен.",
                path.display()
            ),
        }
    }

    if parsed.is_empty() {
        println!("Не удалось обработать ни один из выбранных файлов.");
        return;
    }

    let subtract_holder = Confirm::new()
        .with_prompt("Вычесть сопротивление фильтродержателя из измеренного перепада давления во всех файлах?")
        .default(false)
        .interact()
        .unwrap_or(false);

    if subtract_holder {
        for (_, flows, pressures) in parsed.iter_mut() {
            for (p, f) in pressures.iter_mut().zip(flows.iter()) {
                *p -= holder_resistance_pa(*f);
            }
        }
        println!("Сопротивление фильтродержателя вычтено из всех точек во всех файлах.");
    }

    let mode = match select_compare_mode() {
        Some(m) => m,
        None => return,
    };

    let mut series: Vec<(String, Vec<f64>, Vec<f64>)> = Vec::new();
    let x_desc: &str;

    match mode {
        CompareMode::Absolute => {
            x_desc = "Flow (l/min)";
            for (path, flows, pressures) in &parsed {
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
                series.push((label, flows.clone(), pressures.clone()));
            }
        }
        CompareMode::PerSquareMeter => {
            x_desc = "Flow (fm3/(m2*h))";
            println!();
            println!("Общие условия испытания для приведения расхода к 1 м²:");
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

            if p_abs_bar <= 0.0 {
                println!("Давление должно быть больше нуля. Отменено.");
                return;
            }

            for (path, flows, pressures) in &parsed {
                let default_label = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Series".to_string());
                println!();
                println!(
                    "Файл: {}",
                    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                );
                let area_m2: f64 = Input::new()
                    .with_prompt("площадь фильтроэлемента (м2)")
                    .interact_text()
                    .unwrap_or(0.0);
                if area_m2 <= 0.0 {
                    println!("Площадь должна быть больше нуля. Файл пропущен.");
                    continue;
                }
                let label = match read_text_or_default("Название для легенды", &default_label) {
                    Some(v) => v,
                    None => {
                        println!("Отменено. Возврат в главное меню.");
                        return;
                    }
                };
                let conv_factor = conv_factor_nlmin_to_fm3m2h(temp_c, p_abs_bar, area_m2);
                let specific_flows: Vec<f64> = flows.iter().map(|f| f * conv_factor).collect();
                series.push((label, specific_flows, pressures.clone()));
            }
        }
    }

    if series.is_empty() {
        println!("Не удалось подготовить ни одной серии для графика.");
        return;
    }

    let mut output_name = match read_text_or_default(
        "Введите имя файла для сохранения графика (PNG)",
        "diffp_compare_result.png",
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
    if let Err(e) = plot_compare(&series, x_desc, &output_path) {
        println!("Ошибка построения графика: {}", e);
        return;
    }

    println!();
    println!("Обработано файлов: {}", series.len());
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

fn plot_compare(
    series: &[(String, Vec<f64>, Vec<f64>)],
    x_desc: &str,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = BitMapBackend::new(output_path, (1100, 700)).into_drawing_area();
    root.fill(&WHITE)?;

    let x_max = series
        .iter()
        .flat_map(|(_, x, _)| x.iter())
        .cloned()
        .fold(f64::MIN, f64::max)
        .max(1.0);
    let y_max = series
        .iter()
        .flat_map(|(_, _, y)| y.iter())
        .cloned()
        .fold(f64::MIN, f64::max);
    let y_min = series
        .iter()
        .flat_map(|(_, _, y)| y.iter())
        .cloned()
        .fold(f64::MAX, f64::min)
        .min(0.0);

    let mut chart = ChartBuilder::on(&root)
        .margin(20)
        .x_label_area_size(45)
        .y_label_area_size(70)
        .build_cartesian_2d(0f64..x_max, y_min..y_max)?;

    chart
        .configure_mesh()
        .x_desc(x_desc)
        .y_desc("Differential Pressure (Pa)")
        .x_label_formatter(&|v| format!("{:.0}", v))
        .y_label_formatter(&|v| format!("{:.0}", v))
        .draw()?;

    let palette: [&RGBColor; 6] = [&RED, &BLUE, &GREEN, &MAGENTA, &CYAN, &BLACK];

    for (i, (label, xs, ys)) in series.iter().enumerate() {
        let color = palette[i % palette.len()];
        chart
            .draw_series(LineSeries::new(
                xs.iter().zip(ys.iter()).map(|(x, y)| (*x, *y)),
                color,
            ))?
            .label(label.clone())
            .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], color));

        chart.draw_series(
            xs.iter()
                .zip(ys.iter())
                .map(|(x, y)| Circle::new((*x, *y), 3, color.filled())),
        )?;
    }

    chart
        .configure_series_labels()
        .position(SeriesLabelPosition::LowerRight)
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()?;

    root.present()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holder_resistance_matches_polynomial_at_zero() {
        assert!((holder_resistance_pa(0.0) - 174.57).abs() < 1e-9);
    }

    #[test]
    fn holder_resistance_matches_polynomial_at_reference_flow() {
        let x: f64 = 1000.0;
        let expected = -6e-18 * x.powi(6) + 7e-14 * x.powi(5) - 3e-10 * x.powi(4)
            + 5e-7 * x.powi(3)
            + 6e-5 * x.powi(2)
            + 0.1415 * x
            + 174.57;
        assert!((holder_resistance_pa(x) - expected).abs() < 1e-6);
    }

    #[test]
    fn conv_factor_is_positive_for_normal_conditions() {
        let f = conv_factor_nlmin_to_fm3m2h(20.0, 1.01325, 0.01);
        assert!(f > 0.0);
    }
}
