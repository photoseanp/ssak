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

pub fn run(_config: &AppConfig) {
    println!("Воздухопроницаемость (линейная модель)");
    println!("---------------------------------------");
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

    if area <= 0.0 || pressure <= 0.0 {
        println!("Площадь и давление должны быть больше нуля.");
        return;
    }

    let permeability = flow / (area * pressure);

    println!();
    println!("Расход воздуха: {:.4} л/мин", flow);
    println!("Площадь: {:.4} см2", area);
    println!("Перепад давления: {:.4} мбар", pressure);
    println!("Коэффициент воздухопроницаемости: {:.6} л/(мин*см2*мбар)", permeability);
}
