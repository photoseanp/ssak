use crate::config::AppConfig;
use dialoguer::Select;
use plotters::prelude::*;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
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

pub fn run(config: &AppConfig) {
    println!("Парсер дифференциального давления");
    println!("------------------------------------");

    let input_path = match select_input_file(config) {
        Some(p) => p,
        None => return,
    };

    let file = match File::open(&input_path) {
        Ok(f) => f,
        Err(e) => {
            println!("Не удалось открыть файл {}: {}", input_path.display(), e);
            return;
        }
    };

    let reader = BufReader::new(file);
    let mut time_data: Vec<f64> = Vec::new();
    let mut pressure_data: Vec<f64> = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if i == 0 {
            continue;
        }
        let parts: Vec<&str> = line.split(&[',', ';', '\t'][..]).collect();
        if parts.len() < 2 {
            continue;
        }
        if let (Ok(t), Ok(p)) = (parts[0].trim().parse::<f64>(), parts[1].trim().parse::<f64>()) {
            time_data.push(t);
            pressure_data.push(p);
        }
    }

    if time_data.is_empty() {
        println!("Не удалось извлечь данные из файла.");
        return;
    }

    if let Err(e) = config.ensure_output_dir() {
        println!("Не удалось создать папку для результатов: {}", e);
        return;
    }

    let output_path = config.output_path("diffp_result.png");
    if let Err(e) = plot_data(&time_data, &pressure_data, &output_path) {
        println!("Ошибка построения графика: {}", e);
        return;
    }

    println!();
    println!("Обработано точек: {}", time_data.len());
    println!("График сохранён: {}", output_path.display());
}

fn plot_data(
    time: &[f64],
    pressure: &[f64],
    output_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = BitMapBackend::new(output_path, (1000, 600)).into_drawing_area();
    root.fill(&WHITE)?;

    let x_max = time.iter().cloned().fold(f64::MIN, f64::max);
    let y_max = pressure.iter().cloned().fold(f64::MIN, f64::max);
    let y_min = pressure.iter().cloned().fold(f64::MAX, f64::min);

    let mut chart = ChartBuilder::on(&root)
        .caption("Differential Pressure", ("sans-serif", 30))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(50)
        .build_cartesian_2d(0f64..x_max, y_min..y_max)?;

    chart.configure_mesh().x_desc("Time").y_desc("Pressure").draw()?;

    chart.draw_series(LineSeries::new(
        time.iter().zip(pressure.iter()).map(|(x, y)| (*x, *y)),
        &RED,
    ))?;

    root.present()?;
    Ok(())
}
