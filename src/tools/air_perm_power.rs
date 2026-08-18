use crate::config::AppConfig;
use dialoguer::Input;

fn read_number(prompt: &str) -> Option<f64> {
    let input: String = Input::new()
        .with_prompt(prompt)
        .interact_text()
        .unwrap_or_default();

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

pub fn run(_config: &AppConfig) {
    println!("Воздухопроницаемость (степенная модель)");
    println!("-----------------------------------------");
    println!("(введите q, чтобы отменить и вернуться в главное меню)");

    let flow = match read_number("Введите расход воздуха (л/мин)") {
        Some(v) => v,
        None => {
            println!("Отменено. Возврат в главное меню.");
            return;
        }
    };

    let area = match read_number("Введите площадь фильтра (см2)") {
        Some(v) => v,
        None => {
            println!("Отменено. Возврат в главное меню.");
            return;
        }
    };

    let pressure = match read_number("Введите перепад давления (мбар)") {
        Some(v) => v,
        None => {
            println!("Отменено. Возврат в главное меню.");
            return;
        }
    };

    let n = match read_number_with_default("Введите показатель степени n (по умолчанию 0.5)", 0.5) {
        Some(v) => v,
        None => {
            println!("Отменено. Возврат в главное меню.");
            return;
        }
    };

    if area <= 0.0 || pressure <= 0.0 {
        println!("Площадь и давление должны быть больше нуля.");
        return;
    }

    let permeability = flow / (area * pressure.powf(n));

    println!();
    println!("Расход воздуха: {:.4} л/мин", flow);
    println!("Площадь: {:.4} см2", area);
    println!("Перепад давления: {:.4} мбар", pressure);
    println!("Показатель степени: {:.4}", n);
    println!("Коэффициент воздухопроницаемости: {:.6}", permeability);
}
