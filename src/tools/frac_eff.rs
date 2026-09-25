use crate::config::AppConfig;
use crate::text_io::read_text_lossy;
use dialoguer::{Input, MultiSelect, Select};
use plotters::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quantity {
    Mass,
    Count,
}

impl Quantity {
    fn up_col(self) -> &'static str {
        match self {
            Quantity::Mass => "dCmup",
            Quantity::Count => "dCnup",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Quantity::Mass => "Массовая концентрация (dCmup/dCmdown)",
            Quantity::Count => "Счётная концентрация (dCnup/dCndown)",
        }
    }
}

fn select_quantity() -> Option<Quantity> {
    let items = [Quantity::Mass.label(), Quantity::Count.label()];
    let selection = Select::new()
        .with_prompt("Какую величину использовать для сравнения эффективности?")
        .items(&items)
        .default(0)
        .interact()
        .ok()?;

    Some(if selection == 0 { Quantity::Mass } else { Quantity::Count })
}

/// Режим шкалы оси Y (эффективность, %): обычная линейная шкала 0-100% или
/// логарифмическая шкала "недосепарации" (100% - E), которая растягивает
/// область высокой эффективности (90%, 99%, 99.9%, 99.99%) — так же, как на
/// бумажных бланках сепарационной эффективности фильтров (см. образец с
/// подписями "Complete separation" / "Vollständige Abscheidung" сверху).
#[derive(Clone, Copy, PartialEq, Eq)]
enum YScale {
    Linear,
    Logarithmic,
}

impl YScale {
    fn label(self) -> &'static str {
        match self {
            YScale::Linear => "Обычная (линейная), 0-100%",
            YScale::Logarithmic => "Логарифмическая (0% / 90% / 99% / 99.9% / 99.99%)",
        }
    }
}

fn select_y_scale() -> Option<YScale> {
    let items = [YScale::Linear.label(), YScale::Logarithmic.label()];
    let selection = Select::new()
        .with_prompt("Выберите шкалу оси Y (эффективность)")
        .items(&items)
        .default(0)
        .interact()
        .ok()?;

    Some(if selection == 0 { YScale::Linear } else { YScale::Logarithmic })
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

fn find_col_from(fields: &[&str], target: &str, from: usize) -> Option<usize> {
    fields.iter().enumerate().skip(from).find_map(|(i, f)| {
        let t = f.trim();
        if let Some(rest) = t.strip_prefix(target) {
            if rest.starts_with(' ') || rest.starts_with('[') {
                Some(i)
            } else {
                None
            }
        } else {
            None
        }
    })
}

fn parse_frac_eff_file(path: &Path, quantity: Quantity) -> Option<(Vec<f64>, Vec<f64>)> {
    let content = read_text_lossy(path)?;
    let lines: Vec<&str> = content.lines().collect();

    let mut size_idx: Option<usize> = None;
    let mut eff_idx: Option<usize> = None;
    let mut header_line: Option<usize> = None;

    for (i, line) in lines.iter().enumerate() {
        let fields: Vec<&str> = line.split('\t').collect();
        let si = find_col(&fields, "X");
        let ei = match si.and_then(|s| find_col(&fields, quantity.up_col()).map(|m| (s, m))) {
            Some((_, marker)) => find_col_from(&fields, "E", marker),
            None => find_col(&fields, "E"),
        };
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
            if s > 0.0 && !e.is_nan() {
                sizes.push(s);
                effs.push(e);
            }
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

    let quantity = match select_quantity() {
        Some(q) => q,
        None => return,
    };

    let paths = match select_input_files(config) {
        Some(p) => p,
        None => return,
    };

    let mut series: Vec<(String, Vec<f64>, Vec<f64>)> = Vec::new();

    for path in &paths {
        match parse_frac_eff_file(path, quantity) {
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
                    "Не удалось найти данные фракционной эффективности ({}) в файле {} — файл пропущен.",
                    quantity.label(),
                    path.display()
                );
            }
        }
    }

    if series.is_empty() {
        println!("Не удалось обработать ни один из выбранных файлов.");
        return;
    }

    // Шкала оси Y выбирается после выбора и разбора входных файлов — так
    // пользователь сначала видит, что файлы обработаны успешно, а затем
    // решает, в каком виде представить график.
    let y_scale = match select_y_scale() {
        Some(s) => s,
        None => return,
    };

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
    let plot_result = match y_scale {
        YScale::Linear => plot_data_linear(&series, &output_path),
        YScale::Logarithmic => plot_data_log(&series, &output_path),
    };
    if let Err(e) = plot_result {
        println!("Ошибка построения графика: {}", e);
        return;
    }

    println!();
    println!("Обработано файлов: {}", series.len());
    println!("График сохранён: {}", output_path.display());
}

/// Качественная (категориальная) палитра из 10 визуально различимых цветов.
/// В сочетании с 3 формами маркеров (см. `MarkerShape`) даёт 10*3 = 30
/// уникальных комбинаций "цвет+форма", не повторяющихся при сравнении до
/// 30 файлов одновременно (индексы 0..29 дают 30 разных пар, так как
/// 10 и 3 взаимно просты — цикл по цвету и цикл по форме расходятся в фазе).
const PALETTE: [(u8, u8, u8); 10] = [
    (230, 25, 75),
    (60, 180, 75),
    (255, 196, 12),
    (0, 130, 200),
    (245, 130, 48),
    (145, 30, 180),
    (70, 240, 240),
    (240, 50, 230),
    (170, 110, 40),
    (0, 0, 0),
];

fn palette_color(i: usize) -> RGBColor {
    let (r, g, b) = PALETTE[i % PALETTE.len()];
    RGBColor(r, g, b)
}

/// Форма маркера точки на графике: кружок, крест или треугольник.
#[derive(Clone, Copy)]
enum MarkerShape {
    Circle,
    Cross,
    Triangle,
}

const MARKER_SHAPES: [MarkerShape; 3] = [MarkerShape::Circle, MarkerShape::Cross, MarkerShape::Triangle];

fn marker_shape(i: usize) -> MarkerShape {
    MARKER_SHAPES[i % MARKER_SHAPES.len()]
}

/// Форматирует значение эффективности (%) с точностью, растущей по мере
/// приближения к 100% — так подписи совпадают с ключевыми точками шкалы
/// логарифмического графика: 0%, 90%, 99%, 99.9%, 99.99%.
fn format_eff_label(pct: f64) -> String {
    if pct <= 0.0 {
        "0%".to_string()
    } else if pct < 99.0 {
        format!("{:.0}%", pct)
    } else if pct < 99.9 {
        format!("{:.1}%", pct)
    } else if pct <= 99.99 {
        format!("{:.2}%", pct)
    } else {
        format!("{:.3}%", pct)
    }
}

fn plot_data_linear(
    series: &[(String, Vec<f64>, Vec<f64>)],
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = BitMapBackend::new(output_path, (1100, 700)).into_drawing_area();
    root.fill(&WHITE)?;

    let x_max_raw = series
        .iter()
        .flat_map(|(_, s, _)| s.iter())
        .cloned()
        .fold(f64::MIN, f64::max);
    let x_min_raw = series
        .iter()
        .flat_map(|(_, s, _)| s.iter())
        .cloned()
        .fold(f64::MAX, f64::min);
    let y_min = series
        .iter()
        .flat_map(|(_, _, e)| e.iter())
        .cloned()
        .fold(f64::MAX, f64::min);

    let x_min = x_min_raw / 1.18;
    let x_max = x_max_raw * 1.05;

    let key_points: Vec<f64> = vec![
        0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0,
        10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0,
    ];

    let mut chart = ChartBuilder::on(&root)
        .margin(20)
        .x_label_area_size(45)
        .y_label_area_size(60)
        .build_cartesian_2d((x_min..x_max).log_scale().with_key_points(key_points), y_min..101f64)?;

    chart
        .configure_mesh()
        .x_desc("Particle Size (um)")
        .y_desc("Efficiency (%)")
        .x_label_formatter(&|v| format!("{:.1}", v))
        .y_label_formatter(&|v| format!("{:.1}", v))
        .draw()?;

    for (i, (label, sizes, effs)) in series.iter().enumerate() {
        let color = palette_color(i);
        let shape = marker_shape(i);
        let points: Vec<(f64, f64)> = sizes.iter().zip(effs.iter()).map(|(x, y)| (*x, *y)).collect();

        let series_anno = chart.draw_series(LineSeries::new(points.iter().cloned(), &color))?;
        series_anno.label(label.clone());
        // Значок легенды рисуем как линию + маркер той же формы, что и у точек
        // данной серии, чтобы легенда однозначно ассоциировала цвет и форму
        // маркера с конкретной серией (важно при сравнении до 30 файлов).
        match shape {
            MarkerShape::Circle => {
                series_anno.legend(move |(x, y)| {
                    EmptyElement::at((x, y))
                        + PathElement::new(vec![(0, 0), (20, 0)], color)
                        + Circle::new((10, 0), 3, color.filled())
                });
                chart.draw_series(points.iter().map(|(x, y)| Circle::new((*x, *y), 3, color.filled())))?;
            }
            MarkerShape::Cross => {
                series_anno.legend(move |(x, y)| {
                    EmptyElement::at((x, y))
                        + PathElement::new(vec![(0, 0), (20, 0)], color)
                        + Cross::new((10, 0), 4, color.filled())
                });
                chart.draw_series(points.iter().map(|(x, y)| Cross::new((*x, *y), 4, color.filled())))?;
            }
            MarkerShape::Triangle => {
                series_anno.legend(move |(x, y)| {
                    EmptyElement::at((x, y))
                        + PathElement::new(vec![(0, 0), (20, 0)], color)
                        + TriangleMarker::new((10, 0), 4, color.filled())
                });
                chart.draw_series(points.iter().map(|(x, y)| TriangleMarker::new((*x, *y), 4, color.filled())))?;
            }
        }
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

/// Логарифмический график сепарационной эффективности (аналог бумажных бланков
/// вида "cumulative volumetric efficiency"): по оси Y откладывается не сама
/// эффективность E [%], а "недосепарация" (100 - E) в логарифмическом
/// масштабе. Это растягивает область высокой эффективности так, что 90%,
/// 99%, 99.9% и 99.99% ложатся на равномерно расставленные горизонтальные
/// линии, а верхняя граница графика (100 - E -> 0, недостижимо на лог.
/// шкале) подписана как "100% / Complete separation" — по образцу эталонного
/// бланка. Точка E >= 100% на графике невозможна физически и обрезается по
/// минимально допустимому "недосепарации" 0.001% (т. е. 99.999%), чтобы
/// избежать log(0).
fn plot_data_log(
    series: &[(String, Vec<f64>, Vec<f64>)],
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    const MIN_SHORTFALL: f64 = 0.001; // соответствует 99.999%, верхний предел шкалы
    const MAX_SHORTFALL: f64 = 100.0; // соответствует 0%, нижняя граница шкалы

    let root = BitMapBackend::new(output_path, (1100, 700)).into_drawing_area();
    root.fill(&WHITE)?;

    let x_max_raw = series
        .iter()
        .flat_map(|(_, s, _)| s.iter())
        .cloned()
        .fold(f64::MIN, f64::max);
    let x_min_raw = series
        .iter()
        .flat_map(|(_, s, _)| s.iter())
        .cloned()
        .fold(f64::MAX, f64::min);

    let x_min = x_min_raw / 1.18;
    let x_max = x_max_raw * 1.05;

    let x_key_points: Vec<f64> = vec![
        0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0,
        10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0,
    ];

    // Ключевые уровни эффективности, которые должны лечь на подписанные
    // горизонтальные линии сетки: 0%, 90%, 99%, 99.9%, 99.99%.
    let y_eff_key_points = [0.0f64, 90.0, 99.0, 99.9, 99.99];
    let y_key_points: Vec<f64> = y_eff_key_points
        .iter()
        .map(|pct| (100.0 - pct).max(MIN_SHORTFALL))
        .collect();

    // Ось Y задана диапазоном "недосепарации" по убыванию: внизу графика
    // 100.0 (соответствует 0% эффективности), вверху MIN_SHORTFALL
    // (соответствует ~100% эффективности). Логарифмическая шкала плотнее
    // всего растягивает область у верхней границы, где 100-E близко к нулю.
    let mut chart = ChartBuilder::on(&root)
        .margin(20)
        .x_label_area_size(45)
        .y_label_area_size(70)
        .caption(
            "Complete separation / Vollstandige Abscheidung",
            ("sans-serif", 16),
        )
        .build_cartesian_2d(
            (x_min..x_max).log_scale().with_key_points(x_key_points),
            (MAX_SHORTFALL..MIN_SHORTFALL).log_scale().with_key_points(y_key_points),
        )?;

    chart
        .configure_mesh()
        .x_desc("Particle Size (um)")
        .y_desc("Separation efficiency")
        .x_label_formatter(&|v| format!("{:.1}", v))
        .y_label_formatter(&|v| format_eff_label(100.0 - v))
        .draw()?;

    for (i, (label, sizes, effs)) in series.iter().enumerate() {
        let color = palette_color(i);
        let shape = marker_shape(i);
        let points: Vec<(f64, f64)> = sizes
            .iter()
            .zip(effs.iter())
            .map(|(x, y)| (*x, (100.0 - *y).clamp(MIN_SHORTFALL, MAX_SHORTFALL)))
            .collect();

        let series_anno = chart.draw_series(LineSeries::new(points.iter().cloned(), &color))?;
        series_anno.label(label.clone());
        match shape {
            MarkerShape::Circle => {
                series_anno.legend(move |(x, y)| {
                    EmptyElement::at((x, y))
                        + PathElement::new(vec![(0, 0), (20, 0)], color)
                        + Circle::new((10, 0), 3, color.filled())
                });
                chart.draw_series(points.iter().map(|(x, y)| Circle::new((*x, *y), 3, color.filled())))?;
            }
            MarkerShape::Cross => {
                series_anno.legend(move |(x, y)| {
                    EmptyElement::at((x, y))
                        + PathElement::new(vec![(0, 0), (20, 0)], color)
                        + Cross::new((10, 0), 4, color.filled())
                });
                chart.draw_series(points.iter().map(|(x, y)| Cross::new((*x, *y), 4, color.filled())))?;
            }
            MarkerShape::Triangle => {
                series_anno.legend(move |(x, y)| {
                    EmptyElement::at((x, y))
                        + PathElement::new(vec![(0, 0), (20, 0)], color)
                        + TriangleMarker::new((10, 0), 4, color.filled())
                });
                chart.draw_series(points.iter().map(|(x, y)| TriangleMarker::new((*x, *y), 4, color.filled())))?;
            }
        }
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
    fn palette_has_ten_distinct_colors() {
        let colors: std::collections::HashSet<(u8, u8, u8)> = PALETTE.iter().cloned().collect();
        assert_eq!(colors.len(), 10);
    }

    #[test]
    fn palette_and_marker_cycle_lengths_give_30_combinations() {
        assert_eq!(PALETTE.len() * MARKER_SHAPES.len(), 30);
    }

    #[test]
    fn format_eff_label_matches_expected_key_points() {
        assert_eq!(format_eff_label(0.0), "0%");
        assert_eq!(format_eff_label(90.0), "90%");
        assert_eq!(format_eff_label(99.0), "99%");
        assert_eq!(format_eff_label(99.9), "99.9%");
        assert_eq!(format_eff_label(99.99), "99.99%");
    }
}
