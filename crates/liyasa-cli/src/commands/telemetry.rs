//! CLI-26: `liyasa telemetry on|off|status`. Off unless turned on.

use crate::Exit;
use crate::cli::{Global, Telemetry};
use crate::home;

pub fn run(global: &Global, command: &Telemetry) -> Exit {
    match command {
        Telemetry::Status => {
            let on = home::telemetry_enabled();
            let forced = std::env::var_os("LIYASA_TELEMETRY").is_some();
            if global.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "enabled": on,
                        "source": if forced { "LIYASA_TELEMETRY" } else { "config" },
                        "path": home::config_dir().join(home::TELEMETRY_FILE).display().to_string(),
                    })
                );
            } else {
                println!("telemetry is {}", if on { "on" } else { "off" });
                if forced {
                    println!("set by LIYASA_TELEMETRY in the environment");
                }
            }
            Exit::Success
        }
        Telemetry::On | Telemetry::Off => {
            let on = matches!(command, Telemetry::On);
            if global.dry_run {
                println!(
                    "would turn telemetry {} in {}",
                    if on { "on" } else { "off" },
                    home::config_dir().join(home::TELEMETRY_FILE).display()
                );
                return Exit::Success;
            }
            match home::set_telemetry(on) {
                Ok(path) => {
                    if !global.quiet {
                        println!(
                            "telemetry {} ({})",
                            if on { "on" } else { "off" },
                            path.display()
                        );
                    }
                    Exit::Success
                }
                Err(error) => {
                    eprintln!("could not record the choice: {error}");
                    Exit::Errors
                }
            }
        }
    }
}
