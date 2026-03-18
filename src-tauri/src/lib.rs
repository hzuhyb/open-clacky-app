use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter, Manager};

const INSTALL_SCRIPT_URL: &str = "https://raw.githubusercontent.com/clacky-ai/open-clacky/main/scripts/install.sh";
#[cfg(target_os = "windows")]
const UBUNTU_WSL_URL: &str = "https://mirrors.tuna.tsinghua.edu.cn/ubuntu-cloud-images/wsl/jammy/20250318/ubuntu-jammy-wsl-amd64-ubuntu22.04lts.rootfs.tar.gz";
#[cfg(target_os = "windows")]
const UBUNTU_WSL_INSTALL_DIR: &str = r"C:\WSL\Ubuntu";
const SERVER_HOST: &str = "127.0.0.1";
const SERVER_PORT: u16 = 7070;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(target_os = "windows")]
macro_rules! no_window {
    ($cmd:expr) => {
        $cmd.creation_flags(CREATE_NO_WINDOW)
    };
}
#[cfg(not(target_os = "windows"))]
macro_rules! no_window {
    ($cmd:expr) => {
        $cmd
    };
}

fn emit_log(app: &AppHandle, line: &str) {
    let _ = app.emit("install-log", line.to_string());
}

fn run_streaming(app: &AppHandle, program: &str, args: &[&str]) -> Result<(), String> {
    let mut child = no_window!(Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped()))
        .spawn()
        .map_err(|e| e.to_string())?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let app1 = app.clone();
    let t1 = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            emit_log(&app1, &line);
        }
    });

    let app2 = app.clone();
    let t2 = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            emit_log(&app2, &line);
        }
    });

    let status = child.wait().map_err(|e| e.to_string())?;
    let _ = t1.join();
    let _ = t2.join();

    if status.success() {
        Ok(())
    } else {
        Err(format!("Command failed with exit code: {}", status))
    }
}

fn installed_marker(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().unwrap().join("installed")
}

fn is_installed(app: &AppHandle) -> bool {
    installed_marker(app).exists()
}

fn mark_installed(app: &AppHandle) {
    let marker = installed_marker(app);
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&marker, "1");
}

#[cfg(target_os = "windows")]
fn wsl_kernel_exists() -> bool {
    std::path::Path::new(r"C:\Windows\System32\lxss\tools\init").exists()
}

#[cfg(target_os = "windows")]
fn ubuntu_installed() -> bool {
    no_window!(Command::new("wsl")
        .args(["--list", "--quiet"]))
        .output()
        .map(|o| {
            // wsl --list outputs UTF-16 LE on Windows
            let utf16: Vec<u16> = o.stdout
                .chunks(2)
                .map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)]))
                .collect();
            let out = String::from_utf16_lossy(&utf16);
            // match case-insensitively, strip null chars
            let out = out.replace('\0', "");
            out.lines().any(|l| l.trim().to_lowercase().starts_with("ubuntu"))
        })
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn enable_wsl_features(app: &AppHandle) -> Result<(), String> {
    emit_log(app, "==> Enabling WSL components (requires admin)...");
    run_streaming(app, "dism", &[
        "/online", "/enable-feature",
        "/featurename:Microsoft-Windows-Subsystem-Linux",
        "/all", "/norestart",
    ])?;
    run_streaming(app, "dism", &[
        "/online", "/enable-feature",
        "/featurename:VirtualMachinePlatform",
        "/all", "/norestart",
    ])?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn install_ubuntu(app: &AppHandle) -> Result<(), String> {
    let tar_path = format!("{}\\ubuntu-wsl.tar.gz", std::env::temp_dir().display());
    let install_dir = UBUNTU_WSL_INSTALL_DIR;
    let url = UBUNTU_WSL_URL;

    emit_log(app, "==> Downloading Ubuntu from Tsinghua mirror (~350MB)...");
    run_streaming(app, "powershell", &[
        "-Command",
        &format!("Invoke-WebRequest -Uri '{}' -OutFile '{}' -UseBasicParsing", url, tar_path),
    ])?;
    emit_log(app, "==> Download complete.");

    emit_log(app, "==> Importing Ubuntu into WSL...");
    run_streaming(app, "powershell", &[
        "-Command",
        &format!("New-Item -ItemType Directory -Force -Path '{}' | Out-Null", install_dir),
    ])?;
    run_streaming(app, "wsl", &["--import", "Ubuntu", install_dir, &tar_path])?;
    emit_log(app, "==> Ubuntu imported successfully.");
    Ok(())
}

#[tauri::command]
async fn install(app: AppHandle) -> Result<(), String> {
    if is_installed(&app) {
        emit_log(&app, "==> OpenClacky is already installed.");
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        if !wsl_kernel_exists() {
            enable_wsl_features(&app)?;
            return Err("REBOOT_REQUIRED".to_string());
        }

        if !ubuntu_installed() {
            install_ubuntu(&app)?;
        }

        emit_log(&app, "==> Installing OpenClacky inside WSL...");
        run_streaming(&app, "wsl", &["--", "bash", "-c", &format!("curl -fsSL {} | bash", INSTALL_SCRIPT_URL)])?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        emit_log(&app, "==> Installing OpenClacky...");
        run_streaming(&app, "bash", &["-c", &format!("curl -fsSL {} | bash", INSTALL_SCRIPT_URL)])?;
    }

    mark_installed(&app);
    emit_log(&app, "==> Installation complete!");
    Ok(())
}

fn server_addr() -> String {
    format!("{}:{}", SERVER_HOST, SERVER_PORT)
}

#[tauri::command]
async fn start_server(app: AppHandle) -> Result<String, String> {
    let addr = server_addr();

    if std::net::TcpStream::connect(&addr).is_ok() {
        emit_log(&app, "==> Server already running.");
        return Ok(format!("http://{}", addr));
    }

    emit_log(&app, "==> Starting OpenClacky server...");

    #[cfg(target_os = "windows")]
    {
        no_window!(Command::new("wsl")
            .args(["--", "bash", "-lc", "nohup openclacky server > /tmp/openclacky.log 2>&1 &"]))
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        no_window!(Command::new("bash")
            .args(["-lc", "nohup openclacky server > /tmp/openclacky.log 2>&1 &"]))
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    for i in 0..60 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if std::net::TcpStream::connect(&addr).is_ok() {
            emit_log(&app, "==> Server is ready.");
            return Ok(format!("http://{}", addr));
        }
        if i % 10 == 9 {
            emit_log(&app, &format!("==> Waiting for server... ({}s)", i + 1));
        }
    }

    Err("Server did not start within 60 seconds.".to_string())
}

#[tauri::command]
async fn check_server() -> Option<String> {
    let addr = server_addr();
    if std::net::TcpStream::connect(&addr).is_ok() {
        Some(format!("http://{}", addr))
    } else {
        None
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let show = tauri::menu::MenuItemBuilder::new("Show").id("show").build(app)?;
            let quit = tauri::menu::MenuItemBuilder::new("Quit").id("quit").build(app)?;
            let menu = tauri::menu::MenuBuilder::new(app).items(&[&show, &quit]).build()?;
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png")).unwrap();
            let _tray = tauri::tray::TrayIconBuilder::new()
                .icon(tray_icon)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![install, start_server, check_server])
        .build(tauri::generate_context!())
        .expect("failed to start")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        });
}
