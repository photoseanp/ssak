mod config;
mod tools;

use config::AppConfig;
use dialoguer::Select;

fn main() {
    let mut config = AppConfig::load();

    let items = vec![
        "Расчёт размера пор (точка пузырька)",
        "Воздухопроницаемость (линейная модель)",
        "Воздухопроницаемость (степенная модель)",
        "Парсер дифференциального давления",
        "Сравнение фракционной эффективности",
        "Настройка папок (вход/выход)",
        "Выход",
    ];

    loop {
        println!();
        println!("=== SATEC Swiss Army Knife (SSAK) ===");
        println!("Папка с исходными данными: {}", config.input_dir);
        println!("Папка для результатов: {}", config.output_dir);
        println!();

        let selection = Select::new()
            .with_prompt("Выберите инструмент")
            .items(&items)
            .default(0)
            .interact();

        let selection = match selection {
            Ok(s) => s,
            Err(_) => break,
        };

        println!();

        match selection {
            0 => tools::bubble_point::run(&config),
            1 => tools::air_perm_linear::run(&config),
            2 => tools::air_perm_power::run(&config),
            3 => tools::diffp_parser::run(&config),
            4 => tools::frac_eff::run(&config),
            5 => config::configure_paths(&mut config),
            6 => break,
            _ => {}
        }

        println!();
        println!("Нажмите Enter для возврата в меню (или введите q для выхода из программы)...");
        let mut buf = String::new();
        let _ = std::io::stdin().read_line(&mut buf);
        if buf.trim().eq_ignore_ascii_case("q") {
            break;
        }
    }

    println!("Работа программы завершена.");
}
