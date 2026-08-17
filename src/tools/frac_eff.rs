use crate::config::AppConfig;
use dialoguer::Input;
use plotters::prelude::*;
use std::fs::File;
use std::io::{BufRead, BufReader};

pub fn run(config: &AppConfig) {
    println!("Сравнение фракционной эффективности");
    println!("--------------------------------------");

    let filename: String = Input::new()
        .with_prompt("Введите имя файла с данными (в папке с исходными данными)")
        .interact_text()
        .unwrap_or_default();

    let input_path = config.input_path(&filename);

    let file = match File::open(&input_path) {
        Ok(f) => f,
        Err(e) => {
            println!("Не удалось открыть файл {}: {}", input_path.display(), e);
            return;
        }
    };

    let reader = BufReader::new(file);
    let mut size_data: Vec<f64> = Vec::new();
    let mut efficiency_data: Vec<f64> = Vec::new();

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
        if let (Ok(s), Ok(e)) = (parts[0].trim().parse::<f64>(), parts[1].trim().parse::<f64>()) {
            size_data.push(s);
            efficiency_data.push(e);
        }
    }

    if size_data.is_empty() {
        println!("Не удалось извлечь данные из файла.");
        return;
    }

    if let Err(e) = config.ensure_output_dir() {
        println!("Не удалось создать папку для результатов: {}", e);
        return;
    }

    let output_path = config.output_path("frac_eff_result.png");
    if let Err(e) = plot_data(&size_data, &efficiency_data, &output_path) {
        println!("Ошибка построения графика: {}", e);
        return;
    }

    println!();
    println!("Обработано точек: {}", size_data.len());
    println!("График сохранён: {}", output_path.display());
}

fn plot_data(
    size: &[f64],
    efficiency: &[f64],
    output_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = BitMapBackend::new(output_path, (1000, 600)).into_drawing_area();
    root.fill(&WHITE)?;

    let x_max = size.iter().cloned().fold(f64::MIN, f64::max);

    let mut chart = ChartBuilder::on(&root)
        .caption("Fractional Efficiency", ("sans-serif", 30))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(50)
        .build_cartesian_2d(0f64..x_max, 0f64..100f64)?;

    chart
        .configure_mesh()
        .x_desc("Particle Size")
        .y_desc("Efficiency, %")
        .draw()?;

    chart.draw_series(LineSeries::new(
        size.iter().zip(efficiency.iter()).map(|(x, y)| (*x, *y)),
        &BLUE,
    ))?;

    root.present()?;
    Ok(())
}
