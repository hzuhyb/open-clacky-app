use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter};

fn emit_log(app: &AppHandle, line: &str) {
    let _ = app.emit("install-log", line.to_string());
}

fn run_streaming(app: &AppHandle, program: &str, args: &[&str]) -> Result<(), String> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
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

fn is_installed() -> bool {
    #[cfg(target_os = "windows")]
    {
        Command::new("wsl")
            .args(["--", "bash", "-c", "command -v openclacky"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Command::new("bash")
            .args(["-c", "command -v openclacky"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

#[cfg(target_os = "windows")]
fn wsl_kernel_exists() -> bool {
    std::path::Path::new(r"C:\Windows\System32\lxss\tools\init").exists()
}

#[cfg(target_os = "windows")]
fn ubuntu_installed() -> bool {
    Command::new("wsl")
        .args(["--list", "--quiet"])
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
    let install_dir = r"C:\WSL\Ubuntu";
    let url = "https://mirrors.tuna.tsinghua.edu.cn/ubuntu-cloud-images/wsl/jammy/20250318/ubuntu-jammy-wsl-amd64-ubuntu22.04lts.rootfs.tar.gz";

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
    if is_installed() {
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
        run_streaming(&app, "wsl", &["--", "bash", "-c", "curl -fsSL https://raw.githubusercontent.com/clacky-ai/open-clacky/main/scripts/install.sh | bash"])?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        emit_log(&app, "==> Installing OpenClacky...");
        run_streaming(&app, "bash", &["-c", "curl -fsSL https://raw.githubusercontent.com/clacky-ai/open-clacky/main/scripts/install.sh | bash"])?;
    }

    emit_log(&app, "==> Installation complete!");
    Ok(())
}

#[tauri::command]
async fn start_server(app: AppHandle) -> Result<(), String> {
    if std::net::TcpStream::connect("127.0.0.1:7070").is_ok() {
        emit_log(&app, "==> Server already running.");
        return Ok(());
    }

    emit_log(&app, "==> Starting OpenClacky server...");

    #[cfg(target_os = "windows")]
    {
        Command::new("wsl")
            .args(["--", "bash", "-lc", "nohup openclacky server > /tmp/openclacky.log 2>&1 &"])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        Command::new("bash")
            .args(["-lc", "nohup openclacky server > /tmp/openclacky.log 2>&1 &"])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    // Wait up to 60s for server to be ready
    for i in 0..60 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if std::net::TcpStream::connect("127.0.0.1:7070").is_ok() {
            emit_log(&app, "==> Server is ready.");
            return Ok(());
        }
        if i % 10 == 9 {
            emit_log(&app, &format!("==> Waiting for server... ({}s)", i + 1));
        }
    }

    Err("Server did not start within 60 seconds.".to_string())
}

#[tauri::command]
async fn check_server() -> bool {
    std::net::TcpStream::connect("127.0.0.1:7070").is_ok()
}

#[tauri::command]
async fn open_app(window: tauri::WebviewWindow) -> Result<(), String> {
    let _ = window.navigate("http://127.0.0.1:7070".parse().unwrap());
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![install, start_server, check_server, open_app])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
