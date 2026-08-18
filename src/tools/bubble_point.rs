use crate::config::AppConfig;
use dialoguer::Input;

const DEFAULT_SIGMA: f64 = 0.0221; // Поверхностное натяжение ИПС (изопропиловый спирт), Н/м
const K: f64 = 4.0;

fn read_number(prompt: &str) -> Option<f64> {
    let input: String = Input::new().with_prompt(prompt).interact_text().unwrap_or_default();
    if input.trim().eq_ignore_ascii_case("q") {
        return None;
    }
    match input.trim().replace(',', ".").parse::<f64>() {
        Ok(v) => Some(v),
        Err(_) => {
            println!("Некорректное число. Повторите попытку.");
            None
        }
    }
}

fn read_number_with_default(prompt: &str, default: f64) -> Option<f64> {
    let input: String = Input::new()
        .with_prompt(prompt)
        .default(default.to_string())
        .interact_text()
        .unwrap_or_else(|_| default.to_string());

    if input.trim().eq_ignore_ascii_case("q") {
        return None;
    }

    match input.trim().replace(',', ".").parse::<f64>() {
        Ok(v) => Some(v),
        Err(_) => {
            println!("Некорректное число. Используется значение по умолчанию.");
            Some(default)
        }
    }
}

fn read_count(prompt: &str, min: usize) -> Option<usize> {
    loop {
        let input: String = Input::new().with_prompt(prompt).interact_text().unwrap_or_default();
        if input.trim().eq_ignore_ascii_case("q") {
            return None;
        }
        match input.trim().parse::<usize>() {
            Ok(v) if v >= min => return Some(v),
            Ok(_) => println!("Нужно ввести число не меньше {}.", min),
            Err(_) => println!("Некорректное число. Повторите попытку."),
        }
    }
}

fn t_critical_95(df: usize) -> f64 {
    match df {
        1 => 12.706, 2 => 4.303, 3 => 3.182, 4 => 2.776, 5 => 2.571,
        6 => 2.447, 7 => 2.365, 8 => 2.306, 9 => 2.262, 10 => 2.228,
        11 => 2.201, 12 => 2.179, 13 => 2.160, 14 => 2.145, 15 => 2.131,
        16 => 2.120, 17 => 2.110, 18 => 2.101, 19 => 2.093, 20 => 2.086,
        21 => 2.080, 22 => 2.074, 23 => 2.069, 24 => 2.064, 25 => 2.060,
        26 => 2.056, 27 => 2.052, 28 => 2.048, 29 => 2.045, 30 => 2.042,
        _ => 1.960,
    }
}

pub fn run(_config: &AppConfig) {
    println!("Расчёт размера пор по методу точки пузырька");
    println!("--------------------------------------------");
    println!("(введите q, чтобы отменить и вернуться в главное меню)");

    let sigma = match read_number_with_default(
        "Введите поверхностное натяжение смачивающей жидкости (Н/м) [по умолчанию — ИПС, изопропиловый спирт, 0.0221 Н/м]",
        DEFAULT_SIGMA,
    ) {
        Some(v) => v,
        None => {
            println!("Отменено. Возврат в главное меню.");
            return;
        }
    };
    if sigma <= 0.0 {
        println!("Поверхностное натяжение должно быть больше нуля.");
        return;
    }

    let n = match read_count("Введите количество измерений (минимум 3)", 3) {
        Some(v) => v,
        None => {
            println!("Отменено. Возврат в главное меню.");
            return;
        }
    };

    let mut pressures: Vec<f64> = Vec::with_capacity(n);
    for i in 1..=n {
        let pressure = match read_number(&format!("Давление точки пузырька (Па) — измерение {}", i)) {
            Some(v) => v,
            None => {
                println!("Отменено. Возврат в главное меню.");
                return;
            }
        };
        if pressure <= 0.0 {
            println!("Давление должно быть больше нуля.");
            return;
        }
        pressures.push(pressure);
    }

    let n_f = n as f64;
    let mean = pressures.iter().sum::<f64>() / n_f;
    let variance = pressures.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / (n_f - 1.0);
    let std_dev = variance.sqrt();
    let se = std_dev / n_f.sqrt();
    let t = t_critical_95(n - 1);
    let margin = t * se;
    let p_lower = mean - margin;
    let p_upper = mean + margin;

    let d_mean_m = (K * sigma) / mean;
    let d_lower_m = (K * sigma) / p_upper;
    let d_upper_m = (K * sigma) / p_lower;

    println!();
    println!("Введённые измерения:");
    println!("{:>6} | {:>16}", "№", "Давление (Па)");
    for (i, p) in pressures.iter().enumerate() {
        println!("{:>6} | {:>16.2}", i + 1, p);
    }

    println!();
    println!("Поверхностное натяжение: {:.4} Н/м", sigma);
    println!("Среднее давление: {:.4} Па", mean);
    println!("Стандартное отклонение: {:.4} Па", std_dev);
    println!(
        "95% доверительный интервал давления: {:.4} – {:.4} Па",
        p_lower, p_upper
    );
    println!();
    println!("Расчётный диаметр максимальных пор: {:.4} мкм", d_mean_m * 1_000_000.0);
    println!(
        "95% доверительный интервал диаметра: {:.4} – {:.4} мкм",
        d_lower_m * 1_000_000.0,
        d_upper_m * 1_000_000.0
    );
}
