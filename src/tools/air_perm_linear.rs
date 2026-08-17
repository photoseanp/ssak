use crate::config::AppConfig;
use dialoguer::Input;

pub fn run(_config: &AppConfig) {
    println!("Воздухопроницаемость (линейная модель)");
    println!("---------------------------------------");

    let flow: f64 = Input::new()
        .with_prompt("Введите расход воздуха (л/мин)")
        .interact_text()
        .unwrap_or(0.0);

    let area: f64 = Input::new()
        .with_prompt("Введите площадь фильтра (см2)")
        .interact_text()
        .unwrap_or(1.0);

    let pressure: f64 = Input::new()
        .with_prompt("Введите перепад давления (мбар)")
        .interact_text()
        .unwrap_or(1.0);

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
