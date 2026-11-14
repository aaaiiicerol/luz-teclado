#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::PathBuf,
    process::Command,
};

const LEDS_DIR: &str = "/sys/class/leds";
const UDEV_RULE_FILE: &str = "/etc/udev/rules.d/99-luz-teclado.rules";

#[derive(Serialize)]
struct Estado {
    /// Quantos LEDs de teclado (scrolllock) foram encontrados
    encontrados: usize,
    /// Se algum deles está aceso
    ligado: bool,
    /// Se o programa já pode escrever neles sem senha
    permitido: bool,
}

/// Procura todos os LEDs "*::scrolllock" (o número inputN muda entre PCs e boots)
fn achar_leds() -> Vec<PathBuf> {
    let mut leds: Vec<PathBuf> = fs::read_dir(LEDS_DIR)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_name().to_string_lossy().ends_with("::scrolllock"))
                .map(|e| e.path().join("brightness"))
                .collect()
        })
        .unwrap_or_default();
    leds.sort();
    leds
}

fn pode_escrever(p: &PathBuf) -> bool {
    OpenOptions::new().write(true).open(p).is_ok()
}

fn esta_aceso(p: &PathBuf) -> bool {
    fs::read_to_string(p)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .map(|v| v > 0)
        .unwrap_or(false)
}

#[tauri::command]
fn estado() -> Estado {
    let leds = achar_leds();
    Estado {
        encontrados: leds.len(),
        ligado: leds.iter().any(esta_aceso),
        permitido: !leds.is_empty() && leds.iter().all(pode_escrever),
    }
}

#[tauri::command]
fn definir_luz(ligar: bool) -> Result<(), String> {
    let leds = achar_leds();
    if leds.is_empty() {
        return Err("sem_led".into());
    }
    let valor = if ligar { "1" } else { "0" };
    for led in leds {
        let res = OpenOptions::new()
            .write(true)
            .open(&led)
            .and_then(|mut f| f.write_all(valor.as_bytes()));
        if let Err(e) = res {
            return Err(if e.kind() == ErrorKind::PermissionDenied {
                "sem_permissao".into()
            } else {
                e.to_string()
            });
        }
    }
    Ok(())
}

/// Pede a senha (pkexec/polkit) uma única vez e instala uma regra udev
/// que libera a escrita no LED para sempre, inclusive depois de reiniciar.
#[tauri::command]
fn configurar_permissao() -> Result<(), String> {
    let script = format!(
        r#"set -e
cat > {file} <<'RULE'
ACTION=="add", SUBSYSTEM=="leds", KERNEL=="*::scrolllock", RUN+="/bin/chmod 666 /sys%p/brightness"
RULE
udevadm control --reload-rules
chmod 666 {dir}/*::scrolllock/brightness
"#,
        file = UDEV_RULE_FILE,
        dir = LEDS_DIR
    );

    let saida = Command::new("pkexec")
        .args(["/bin/sh", "-c", &script])
        // O AppImage define variáveis que podem quebrar programas do sistema
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("LD_PRELOAD")
        .output()
        .map_err(|e| format!("Não consegui abrir o pkexec: {e}"))?;

    if saida.status.success() {
        Ok(())
    } else {
        match saida.status.code() {
            Some(126) | Some(127) => Err("cancelado".into()),
            _ => Err(String::from_utf8_lossy(&saida.stderr).trim().to_string()),
        }
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            estado,
            definir_luz,
            configurar_permissao
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o aplicativo");
}
