use crate::config::AppConfig;
use dialoguer::Input;

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

struct LinearFit {
    slope: f64,
    intercept: f64,
    r2: f64,
    se: f64,
    x_mean: f64,
    sxx: f64,
    n: usize,
}

fn fit_linear(x: &[f64], y: &[f64]) -> LinearFit {
    let n = x.len();
    let x_mean = x.iter().sum::<f64>() / n as f64;
    let y_mean = y.iter().sum::<f64>() / n as f64;
    let sxx: f64 = x.iter().map(|xi| (xi - x_mean).powi(2)).sum();
    let sxy: f64 = x.iter().zip(y.iter()).map(|(xi, yi)| (xi - x_mean) * (yi - y_mean)).sum();
    let syy: f64 = y.iter().map(|yi| (yi - y_mean).powi(2)).sum();
    let slope = sxy / sxx;
    let intercept = y_mean - slope * x_mean;
    let sse: f64 = x
        .iter()
        .zip(y.iter())
        .map(|(xi, yi)| {
            let pred = slope * xi + intercept;
            (yi - pred).powi(2)
        })
        .sum();
    let dof = n - 2;
    let se = (sse / dof as f64).sqrt();
    let r2 = if syy > 0.0 { 1.0 - sse / syy } else { 0.0 };
    LinearFit { slope, intercept, r2, se, x_mean, sxx, n }
}

fn predict_with_ci(fit: &LinearFit, x0: f64) -> (f64, f64, f64) {
    let y0 = fit.slope * x0 + fit.intercept;
    let se_pred = fit.se * (1.0 + 1.0 / fit.n as f64 + (x0 - fit.x_mean).powi(2) / fit.sxx).sqrt();
    let t = t_critical_95(fit.n - 2);
    let half = t * se_pred;
    (y0, y0 - half, y0 + half)
}

pub fn run(_config: &AppConfig) {
    println!("Воздухопроницаемость (линейная модель)");
    println!("---------------------------------------");
    println!("(введите q, чтобы отменить и вернуться в главное меню)");

    let area = match read_number("Введите площадь фильтроповерхности (м2)") {
        Some(v) => v,
        None => {
            println!("Отменено. Возврат в главное меню.");
            return;
        }
    };
    if area <= 0.0 {
        println!("Площадь должна быть больше нуля.");
        return;
    }

    let n = match read_count("Введите количество точек эксперимента (минимум 3)", 3) {
        Some(v) => v,
        None => {
            println!("Отменено. Возврат в главное меню.");
            return;
        }
    };

    let mut pressures: Vec<f64> = Vec::with_capacity(n);
    let mut flows: Vec<f64> = Vec::with_capacity(n);

    for i in 1..=n {
        println!();
        println!("Точка {} из {}", i, n);
        let flow = match read_number(&format!("  Установленный расход (л/мин) для точки {}", i)) {
            Some(v) => v,
            None => {
                println!("Отменено. Возврат в главное меню.");
                return;
            }
        };
        let pressure = match read_number(&format!(
            "  Перепад давления на фильтроэлементе (Па) для точки {}",
            i
        )) {
            Some(v) => v,
            None => {
                println!("Отменено. Возврат в главное меню.");
                return;
            }
        };
        if flow <= 0.0 || pressure <= 0.0 {
            println!("Расход и давление должны быть больше нуля.");
            return;
        }
        flows.push(flow);
        pressures.push(pressure);
    }

    let specific_flow: Vec<f64> = flows.iter().map(|f| f / area).collect();
    let fit = fit_linear(&pressures, &specific_flow);

    println!();
    println!("Введённые точки:");
    println!("{:>6} | {:>16} | {:>18}", "№", "Давление (Па)", "Расход (л/мин)");
    for i in 0..n {
        println!("{:>6} | {:>16.2} | {:>18.4}", i + 1, pressures[i], flows[i]);
    }

    println!();
    println!("Регрессия: удельный расход = k * ΔP + b");
    println!("k = {:.6}, b = {:.6}", fit.slope, fit.intercept);
    println!("R2 = {:.4}", fit.r2);

    println!();
    println!("Прогноз расхода с 95% доверительным интервалом:");
    println!(
        "{:>8} | {:>14} | {:>14} | {:>14} | {:>16} | {:>16} | {:>16}",
        "ΔP(Па)", "Q(л/мин)", "Q_нижн", "Q_верхн", "q(л/м2/мин)", "q_нижн", "q_верхн"
    );
    for &target in &[200.0_f64, 125.0_f64] {
        let (y0, lo, hi) = predict_with_ci(&fit, target);
        println!(
            "{:>8.1} | {:>14.4} | {:>14.4} | {:>14.4} | {:>16.4} | {:>16.4} | {:>16.4}",
            target,
            y0 * area,
            lo * area,
            hi * area,
            y0,
            lo,
            hi
        );
    }
}
