use crate::config::AppConfig;
use dialoguer::Input;

const SIGMA: f64 = 0.0728;
const K: f64 = 4.0;

pub fn run(_config: &AppConfig) {
    println!("Расчёт размера пор по методу точки пузырька");
    println!("--------------------------------------------");

    let pressure: f64 = Input::new()
        .with_prompt("Введите давление точки пузырька (бар)")
        .interact_text()
        .unwrap_or(0.0);

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
