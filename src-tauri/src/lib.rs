use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{atomic::{AtomicBool, Ordering}, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};

const INSTALL_SCRIPT_URL: &str = "https://clackyai-1258723534.cos.ap-guangzhou.myqcloud.com/install.sh";
#[cfg(target_os = "windows")]
const UBUNTU_WSL_URL: &str = "https://clackyai-1258723534.cos.ap-guangzhou.myqcloud.com/ubuntu-jammy-wsl-amd64-ubuntu22.04lts.rootfs.tar.gz";
#[cfg(target_os = "windows")]
const WSL_UPDATE_URL: &str = "https://clackyai-1258723534.cos.ap-guangzhou.myqcloud.com/wsl_update_x64.msi";
#[cfg(target_os = "windows")]
const UBUNTU_WSL_INSTALL_DIR: &str = r"C:\WSL\Ubuntu";
const SERVER_HOST: &str = "127.0.0.1";
const SERVER_PORT: u16 = 7070;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TrayBusyState {
    Idle,
    Starting,
    Stopping,
}

static TRAY_BUSY_STATE: OnceLock<Mutex<TrayBusyState>> = OnceLock::new();
static APP_IS_QUITTING: AtomicBool = AtomicBool::new(false);

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

#[tauri::command]
fn reboot_system() {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("shutdown").args(["/r", "/t", "0"]).spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = Command::new("shutdown").args(["-r", "now"]).spawn();
    }
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

fn tray_busy_state() -> &'static Mutex<TrayBusyState> {
    TRAY_BUSY_STATE.get_or_init(|| Mutex::new(TrayBusyState::Idle))
}

fn set_tray_busy_state(state: TrayBusyState) {
    if let Ok(mut busy_state) = tray_busy_state().lock() {
        *busy_state = state;
    }
}

fn get_tray_busy_state() -> TrayBusyState {
    tray_busy_state()
        .lock()
        .map(|busy_state| *busy_state)
        .unwrap_or(TrayBusyState::Idle)
}

fn is_installed(app: &AppHandle) -> bool {
    installed_marker(app).exists()
}

fn clear_installed_marker(app: &AppHandle) {
    let _ = std::fs::remove_file(installed_marker(app));
}

fn mark_installed(app: &AppHandle) {
    let marker = installed_marker(app);
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&marker, "1");
}

#[cfg(target_os = "windows")]
fn is_install_valid(_app: &AppHandle) -> bool {
    if !wsl_feature_enabled() || !wsl_kernel_exists() || !ubuntu_installed() {
        return false;
    }

    no_window!(Command::new("wsl")
        .args(["-d", "Ubuntu", "-u", "root", "--", "bash", "-lc", "command -v openclacky >/dev/null 2>&1"]))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
fn is_install_valid(_app: &AppHandle) -> bool {
    no_window!(Command::new("bash")
        .args(["-lc", "command -v openclacky >/dev/null 2>&1"]))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn wsl_feature_enabled() -> bool {
    // exit 1 = WSL feature not enabled (stub wsl.exe)
    // exit -1 = feature enabled but kernel missing
    // exit 0 = fully functional
    no_window!(Command::new("wsl.exe").args(["--list"]))
        .output()
        .map(|o| o.status.code() != Some(1))
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn wsl_kernel_exists() -> bool {
    // exit -1 means WSL2 kernel is missing
    no_window!(Command::new("wsl.exe").arg("--list"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn ubuntu_installed() -> bool {
    no_window!(Command::new("wsl")
        .args(["--list", "--quiet"]))
        .output()
        .map(|o| {
            let utf16: Vec<u16> = o.stdout
                .chunks(2)
                .map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)]))
                .collect();
            let out = String::from_utf16_lossy(&utf16).replace('\0', "");
            out.lines().any(|l| l.trim().to_lowercase().starts_with("ubuntu"))
        })
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn enable_wsl_features(app: &AppHandle) -> Result<(), String> {
    emit_log(app, "==> Enabling WSL components (requires admin)...");
    let script = r#"
        Start-Process powershell -Verb RunAs -Wait -ArgumentList '-Command',
        'dism /online /enable-feature /featurename:Microsoft-Windows-Subsystem-Linux /all /norestart;
         dism /online /enable-feature /featurename:VirtualMachinePlatform /all /norestart'
    "#;
    run_streaming(app, "powershell", &["-Command", script])?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn install_wsl_kernel(app: &AppHandle) -> Result<(), String> {
    let msi_path = format!("{}\\wsl_update.msi", std::env::temp_dir().display());
    emit_log(app, "==> Downloading WSL2 kernel update...");
    if run_streaming(app, "curl", &["-L", "--progress-bar", WSL_UPDATE_URL, "-o", &msi_path]).is_err() {
        run_streaming(app, "powershell", &[
            "-Command",
            &format!("Invoke-WebRequest -Uri '{}' -OutFile '{}' -UseBasicParsing", WSL_UPDATE_URL, msi_path),
        ])?;
    }
    emit_log(app, "==> Download complete. Installing WSL2 kernel...");
    run_streaming(app, "powershell", &[
        "-Command",
        &format!("Start-Process msiexec -Verb RunAs -Wait -ArgumentList '/i','{}','/quiet','/norestart'", msi_path),
    ])?;
    emit_log(app, "==> WSL2 kernel installed.");
    Ok(())
}

#[cfg(target_os = "windows")]
fn install_ubuntu(app: &AppHandle) -> Result<(), String> {
    let tar_path = format!("{}\\ubuntu-wsl.tar.gz", std::env::temp_dir().display());
    let install_dir = UBUNTU_WSL_INSTALL_DIR;
    let url = UBUNTU_WSL_URL;

    emit_log(app, "==> Downloading Ubuntu (~350MB)...");
    if run_streaming(app, "curl", &["-L", "--progress-bar", url, "-o", &tar_path]).is_err() {
        run_streaming(app, "powershell", &[
            "-Command",
            &format!("Invoke-WebRequest -Uri '{}' -OutFile '{}' -UseBasicParsing", url, tar_path),
        ])?;
    }
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

fn do_install(app: &AppHandle) -> Result<(), String> {
    if is_installed(app) {
        if is_install_valid(app) {
            return Ok(());
        }
        emit_log(app, "==> Existing installation marker is stale. Reinstalling OpenClacky...");
        clear_installed_marker(app);
    }

    #[cfg(target_os = "windows")]
    {
        if !wsl_feature_enabled() {
            enable_wsl_features(app)?;
            return Err("REBOOT_REQUIRED".to_string());
        }
        if !wsl_kernel_exists() {
            install_wsl_kernel(app)?;
        }
        if !ubuntu_installed() {
            install_ubuntu(app)?;
        }
        emit_log(app, "==> Installing OpenClacky inside WSL...");
        run_streaming(app, "wsl", &["-d", "Ubuntu", "-u", "root", "--", "bash", "-c", &format!("cd ~ && curl -fsSL {} | bash", INSTALL_SCRIPT_URL)])?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        emit_log(app, "==> Installing OpenClacky...");
        run_streaming(app, "bash", &["-c", &format!("curl -fsSL {} | bash", INSTALL_SCRIPT_URL)])?;
    }

    mark_installed(app);
    Ok(())
}

fn server_addr() -> String {
    format!("{}:{}", SERVER_HOST, SERVER_PORT)
}

fn is_server_running() -> bool {
    std::net::TcpStream::connect(server_addr()).is_ok()
}

fn do_start_server() -> Result<(), String> {
    if is_server_running() {
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        no_window!(Command::new("wsl")
            .args(["-d", "Ubuntu", "-u", "root", "--", "bash", "-lc", "cd ~ && ~/.local/bin/mise exec ruby -- openclacky server > /tmp/openclacky.log 2>&1"])
            .stdout(Stdio::null())
            .stderr(Stdio::null()))
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        no_window!(Command::new("bash")
            .args(["-lc", "openclacky server > /tmp/openclacky.log 2>&1"])
            .stdout(Stdio::null())
            .stderr(Stdio::null()))
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if is_server_running() {
            return Ok(());
        }
    }

    Err("Server did not start within 60 seconds.".to_string())
}

fn do_stop_server() {
    #[cfg(target_os = "windows")]
    {
        let _ = no_window!(Command::new("wsl")
            .args(["-d", "Ubuntu", "-u", "root", "--", "bash", "-c", "pkill -f 'openclacky server'"]))
            .output();
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = Command::new("pkill")
            .args(["-f", "openclacky server"])
            .output();
    }
}

fn wait_for_server_stop() {
    for _ in 0..60 {
        if !is_server_running() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

fn update_tray_menu(app: &AppHandle) {
    let running = is_server_running();
    let busy_state = get_tray_busy_state();
    let is_busy = busy_state != TrayBusyState::Idle;
    if let Some(tray) = app.tray_by_id("main") {
        let open = tauri::menu::MenuItemBuilder::new("Open in Browser")
            .id("open")
            .enabled(running && !is_busy)
            .build(app)
            .unwrap();
        let start_label = if busy_state == TrayBusyState::Starting { "Starting..." } else { "Start" };
        let stop_label = if busy_state == TrayBusyState::Stopping { "Stopping..." } else { "Stop" };
        let start = tauri::menu::MenuItemBuilder::new(start_label)
            .id("start")
            .enabled(!running && !is_busy)
            .build(app)
            .unwrap();
        let stop = tauri::menu::MenuItemBuilder::new(stop_label)
            .id("stop")
            .enabled(running && !is_busy)
            .build(app)
            .unwrap();
        let quit = tauri::menu::MenuItemBuilder::new("Quit")
            .id("quit")
            .enabled(!is_busy)
            .build(app)
            .unwrap();
        let menu = tauri::menu::MenuBuilder::new(app)
            .items(&[&open, &start, &stop, &quit])
            .build()
            .unwrap();
        let _ = tray.set_menu(Some(menu));
    }
}

fn sync_server_state(app: &AppHandle) -> bool {
    set_tray_busy_state(TrayBusyState::Idle);
    let running = is_server_running();
    let _ = app.emit("server-status", running);
    update_tray_menu(app);
    running
}

fn ensure_started(app: &AppHandle) -> Result<(), String> {
    do_install(app)?;

    if let Err(start_error) = do_start_server() {
        if is_installed(app) {
            emit_log(app, "==> Startup failed. Revalidating installation and retrying once...");
            clear_installed_marker(app);
            do_install(app)?;
            do_start_server()?;
            return Ok(());
        }
        return Err(start_error);
    }

    Ok(())
}

fn launch_start_server(app: AppHandle) {
    set_tray_busy_state(TrayBusyState::Starting);
    update_tray_menu(&app);
    std::thread::spawn(move || {
        if let Err(error) = ensure_started(&app) {
            eprintln!("Start error: {}", error);
            let _ = app.emit("install-error", error);
            sync_server_state(&app);
            return;
        }
        sync_server_state(&app);
    });
}

fn launch_stop_server(app: AppHandle) {
    set_tray_busy_state(TrayBusyState::Stopping);
    update_tray_menu(&app);
    std::thread::spawn(move || {
        do_stop_server();
        wait_for_server_stop();
        sync_server_state(&app);
    });
}

#[derive(Serialize)]
struct InitialState {
    installed: bool,
    server_running: bool,
}

#[tauri::command]
fn get_initial_state(app: AppHandle) -> InitialState {
    let installed = is_installed(&app) && is_install_valid(&app);
    if !installed && is_installed(&app) {
        clear_installed_marker(&app);
    }

    InitialState {
        installed,
        server_running: is_server_running(),
    }
}

#[tauri::command]
fn get_server_status() -> bool {
    is_server_running()
}

#[tauri::command]
fn open_url() {
    let url = format!("http://{}:{}", SERVER_HOST, SERVER_PORT);
    let _ = tauri_plugin_opener::open_url(url, None::<&str>);
}

#[tauri::command]
fn start_server(app: AppHandle) {
    launch_start_server(app);
}

#[tauri::command]
fn retry_install(app: AppHandle) {
    launch_start_server(app);
}

#[tauri::command]
fn stop_server(app: AppHandle) {
    launch_stop_server(app);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Show in Dock on macOS
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);

            let open = tauri::menu::MenuItemBuilder::new("Open in Browser").id("open").build(app)?;
            let start = tauri::menu::MenuItemBuilder::new("Start").id("start").build(app)?;
            let stop = tauri::menu::MenuItemBuilder::new("Stop").id("stop").enabled(false).build(app)?;
            let quit = tauri::menu::MenuItemBuilder::new("Quit").id("quit").build(app)?;
            let menu = tauri::menu::MenuBuilder::new(app).items(&[&open, &start, &stop, &quit]).build()?;

            if let Some(window) = app.get_webview_window("main") {
                let window_for_events = window.clone();
                let _ = window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        if !APP_IS_QUITTING.load(Ordering::SeqCst) {
                            api.prevent_close();
                            let _ = window_for_events.hide();
                        }
                    }
                });
            }

            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png")).unwrap();
            let _tray = tauri::tray::TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                        let _ = app.emit("show-dashboard", ());
                    }
                })
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => {
                        let url = format!("http://{}:{}", SERVER_HOST, SERVER_PORT);
                        let _ = tauri_plugin_opener::open_url(url, None::<&str>);
                    }
                    "start" => {
                        let app = app.clone();
                        launch_start_server(app);
                    }
                    "stop" => {
                        let app = app.clone();
                        launch_stop_server(app);
                    }
                    "quit" => {
                        APP_IS_QUITTING.store(true, Ordering::SeqCst);
                        app.exit(0)
                    },
                    _ => {}
                })
                .build(app)?;

            // Auto start on launch
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                let already_installed = is_installed(&app_handle) && is_install_valid(&app_handle);
                if !already_installed {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                if let Err(e) = ensure_started(&app_handle) {
                    if e.contains("REBOOT_REQUIRED") {
                        let _ = app_handle.emit("install-reboot", "WSL components installed. Please restart your computer, then reopen the app.");
                    } else {
                        let _ = app_handle.emit("install-error", e);
                    }
                    return;
                }
                let server_running = is_server_running();
                if already_installed {
                    let _ = app_handle.emit("show-dashboard", server_running);
                } else {
                    let _ = app_handle.emit("install-success", server_running);
                }
                sync_server_state(&app_handle);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![reboot_system, get_initial_state, get_server_status, open_url, start_server, retry_install, stop_server])
        .build(tauri::generate_context!())
        .expect("failed to start")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { has_visible_windows, .. } = event {
                if !has_visible_windows {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
        });
}
