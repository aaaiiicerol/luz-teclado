#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::{
    env,
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::PathBuf,
    process::Command,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

const LEDS_DIR: &str = "/sys/class/leds";
const UDEV_RULE_FILE: &str = "/etc/udev/rules.d/99-luz-teclado.rules";

// ---------------------------------------------------------------- LEDs

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

fn ler_estado() -> Estado {
    let leds = achar_leds();
    Estado {
        encontrados: leds.len(),
        ligado: leds.iter().any(esta_aceso),
        permitido: !leds.is_empty() && leds.iter().all(pode_escrever),
    }
}

/// Liga ou desliga a luz. Usada pela janela, pela bandeja e pela linha de comando.
fn aplicar_luz(ligar: bool) -> Result<(), String> {
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

#[tauri::command]
fn estado() -> Estado {
    ler_estado()
}

#[tauri::command]
fn definir_luz(ligar: bool) -> Result<(), String> {
    aplicar_luz(ligar)
}

// ---------------------------------------------------------- Permissão

/// Roda um script como administrador (pede a senha pela janela do sistema).
fn rodar_pkexec(script: &str) -> Result<(), String> {
    let saida = Command::new("pkexec")
        .args(["/bin/sh", "-c", script])
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

/// Pede a senha uma única vez e instala uma regra udev que libera a escrita
/// no LED para sempre, inclusive depois de reiniciar.
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
    rodar_pkexec(&script)
}

/// Desfaz o que configurar_permissao fez.
#[tauri::command]
fn remover_permissao() -> Result<(), String> {
    let script = format!(
        r#"rm -f {file}
udevadm control --reload-rules
chmod 644 {dir}/*::scrolllock/brightness || true
"#,
        file = UDEV_RULE_FILE,
        dir = LEDS_DIR
    );
    rodar_pkexec(&script)
}

// ------------------------------------------------------ Configurações

fn config_dir() -> PathBuf {
    if let Ok(x) = env::var("XDG_CONFIG_HOME") {
        if !x.is_empty() {
            return PathBuf::from(x);
        }
    }
    PathBuf::from(env::var("HOME").unwrap_or_default()).join(".config")
}

fn arquivo_bandeja() -> PathBuf {
    config_dir().join("luz-teclado").join("bandeja")
}

fn arquivo_autostart() -> PathBuf {
    config_dir().join("autostart").join("luz-teclado.desktop")
}

/// Caminho do programa: no AppImage vem em $APPIMAGE (o executável real fica numa pasta temporária)
fn caminho_exec() -> String {
    env::var("APPIMAGE")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            env::current_exe()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        })
}

fn bandeja_ativa() -> bool {
    arquivo_bandeja().exists()
}

#[tauri::command]
fn bandeja_estado() -> bool {
    bandeja_ativa()
}

#[tauri::command]
fn bandeja_definir(ativar: bool) -> Result<(), String> {
    let arq = arquivo_bandeja();
    if ativar {
        if let Some(pai) = arq.parent() {
            fs::create_dir_all(pai).map_err(|e| e.to_string())?;
        }
        fs::write(&arq, "1").map_err(|e| e.to_string())
    } else {
        match fs::remove_file(&arq) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[tauri::command]
fn autostart_estado() -> bool {
    arquivo_autostart().exists()
}

/// Liga a luz sozinha quando a pessoa entra no computador (roda "programa --on", sem janela).
#[tauri::command]
fn autostart_definir(ativar: bool) -> Result<(), String> {
    let arq = arquivo_autostart();
    if ativar {
        if let Some(pai) = arq.parent() {
            fs::create_dir_all(pai).map_err(|e| e.to_string())?;
        }
        let conteudo = format!(
            "[Desktop Entry]\nType=Application\nName=Luz do Teclado\nComment=Liga a luz do teclado ao iniciar a sessao\nExec=\"{}\" --on\nTerminal=false\nX-GNOME-Autostart-enabled=true\nX-GNOME-Autostart-Delay=3\n",
            caminho_exec()
        );
        fs::write(&arq, conteudo).map_err(|e| e.to_string())
    } else {
        match fs::remove_file(&arq) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

// ------------------------------------------------------------ Suporte

fn ler_arquivo(p: &str) -> String {
    fs::read_to_string(p)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "?".into())
}

fn nome_distro() -> String {
    fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("PRETTY_NAME="))
                .map(|l| l.trim_start_matches("PRETTY_NAME=").trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "?".into())
}

/// Texto para o usuário colar nos comentários quando algo não funcionar.
#[tauri::command]
fn info_suporte() -> String {
    let mut t = String::new();
    t.push_str(&format!("Luz do Teclado v{}\n", env!("CARGO_PKG_VERSION")));
    t.push_str(&format!("Distro: {}\n", nome_distro()));
    t.push_str(&format!("Kernel: {}\n", ler_arquivo("/proc/sys/kernel/osrelease")));
    t.push_str(&format!(
        "Desktop: {} ({})\n",
        env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "?".into()),
        env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "?".into())
    ));
    t.push_str(&format!(
        "Regra udev: {}\n",
        if PathBuf::from(UDEV_RULE_FILE).exists() { "instalada" } else { "nao instalada" }
    ));
    t.push_str(&format!("\nLEDs em {LEDS_DIR}:\n"));
    match fs::read_dir(LEDS_DIR) {
        Ok(rd) => {
            let mut nomes: Vec<_> = rd.flatten().collect();
            nomes.sort_by_key(|e| e.file_name());
            if nomes.is_empty() {
                t.push_str("  (nenhum)\n");
            }
            for e in nomes {
                let p = e.path();
                let b = p.join("brightness");
                t.push_str(&format!(
                    "  - {} (brilho={}, max={}, escrita={})\n",
                    e.file_name().to_string_lossy(),
                    ler_arquivo(&b.to_string_lossy()),
                    ler_arquivo(&p.join("max_brightness").to_string_lossy()),
                    if pode_escrever(&b) { "sim" } else { "nao" }
                ));
            }
        }
        Err(_) => t.push_str("  (nao foi possivel ler a pasta)\n"),
    }
    t
}

// ------------------------------------------------------ Linha de comando

fn ajuda() {
    println!(
        "Luz do Teclado v{}\n\nUso:\n  luz-teclado           abre a janela\n  luz-teclado --on      liga a luz\n  luz-teclado --off     desliga a luz\n  luz-teclado --toggle  alterna ligada/desligada\n  luz-teclado --status  mostra se esta ligada\n  luz-teclado --help    esta ajuda",
        env!("CARGO_PKG_VERSION")
    );
}

fn cli_definir(ligar: bool) -> i32 {
    match aplicar_luz(ligar) {
        Ok(_) => {
            println!("Luz {}", if ligar { "ligada" } else { "desligada" });
            0
        }
        Err(e) => {
            match e.as_str() {
                "sem_led" => eprintln!("Nenhum LED de teclado (scrolllock) encontrado."),
                "sem_permissao" => eprintln!("Sem permissao. Abra o programa uma vez (sem argumentos) para liberar o acesso."),
                outro => eprintln!("Erro: {outro}"),
            }
            1
        }
    }
}

/// Retorna Some(codigo_de_saida) se algum argumento de linha de comando foi tratado.
fn cli(args: &[String]) -> Option<i32> {
    for a in args {
        match a.as_str() {
            "--on" | "--ligar" => return Some(cli_definir(true)),
            "--off" | "--desligar" => return Some(cli_definir(false)),
            "--toggle" | "--alternar" => return Some(cli_definir(!ler_estado().ligado)),
            "--status" => {
                let e = ler_estado();
                println!("{}", if e.ligado { "ligada" } else { "desligada" });
                return Some(if e.encontrados == 0 { 1 } else { 0 });
            }
            "--help" | "-h" | "--ajuda" => {
                ajuda();
                return Some(0);
            }
            _ => {}
        }
    }
    None
}

// ------------------------------------------------------------ Bandeja

fn mostrar_janela(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Cria o ícone da bandeja. Se o sistema não suportar, o programa segue normalmente sem ele.
fn configurar_bandeja(app: &AppHandle) {
    let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<(), tauri::Error> {
            let abrir = MenuItem::with_id(app, "abrir", "Abrir janela", true, None::<&str>)?;
            let ligar = MenuItem::with_id(app, "ligar", "Ligar luz", true, None::<&str>)?;
            let desligar = MenuItem::with_id(app, "desligar", "Desligar luz", true, None::<&str>)?;
            let sair = MenuItem::with_id(app, "sair", "Sair", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&abrir, &ligar, &desligar, &sair])?;

            let mut tray = TrayIconBuilder::new()
                .tooltip("Luz do Teclado")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "abrir" => mostrar_janela(app),
                    "ligar" => {
                        let _ = aplicar_luz(true);
                    }
                    "desligar" => {
                        let _ = aplicar_luz(false);
                    }
                    "sair" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        mostrar_janela(tray.app_handle());
                    }
                });

            if let Some(icone) = app.default_window_icon() {
                tray = tray.icon(icone.clone());
            }
            tray.build(app)?;
            Ok(())
        },
    ));

    match resultado {
        Ok(Err(e)) => eprintln!("Bandeja indisponivel: {e}"),
        Err(_) => eprintln!("Bandeja indisponivel neste sistema."),
        Ok(Ok(())) => {}
    }
}

// --------------------------------------------------------------- main

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Some(codigo) = cli(&args) {
        std::process::exit(codigo);
    }

    tauri::Builder::default()
        .setup(|app| {
            configurar_bandeja(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Só esconde na bandeja se a pessoa ativou essa opção
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if bandeja_ativa() {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            estado,
            definir_luz,
            configurar_permissao,
            remover_permissao,
            autostart_estado,
            autostart_definir,
            bandeja_estado,
            bandeja_definir,
            info_suporte
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o aplicativo");
}
