use crate::config::AppConfig;
use dialoguer::Input;

const SIGMA: f64 = 0.0728;
const K: f64 = 4.0;

pub fn run(_config: &AppConfig) {
    println!("Расчёт размера пор по методу точки пузырька");
    println!("--------------------------------------------");
    println!("(введите q, чтобы отменить и вернуться в главное меню)");

    let input: String = Input::new()
        .with_prompt("Введите давление точки пузырька (бар)")
        .interact_text()
        .unwrap_or_default();

    if input.trim().eq_ignore_ascii_case("q") {
        println!("Отменено. Возврат в главное меню.");
        return;
    }

    let pressure: f64 = match input.trim().replace(',', ".").parse() {
        Ok(v) => v,
        Err(_) => {
            println!("Некорректное число. Повторите попытку.");
            return;
        }
    };

    if pressure <= 0.0 {
        println!("Давление должно быть больше нуля.");
        return;
    }

    let pressure_pa = pressure * 100_000.0;
    let pore_diameter_m = (K * SIGMA) / pressure_pa;
    let pore_diameter_um = pore_diameter_m * 1_000_000.0;

    println!();
    println!("Давление точки пузырька: {:.4} бар", pressure);
    println!("Расчётный диаметр пор: {:.4} мкм", pore_diameter_um);
}
