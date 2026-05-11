use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

fn create_hidden_command(program: &str) -> Command {
    let cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = cmd;
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }
    #[cfg(not(windows))]
    cmd
}

// ─── Data Structures ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub serial: String,
    pub model: String,
    pub pda: String,
    pub android_version: String,
    pub status: String, // "device", "offline", "unauthorized"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestItem {
    pub id: String,
    pub name: String,
    pub jar: String,
    pub main_class: String,
    pub test_type: String, // "auto", "optional", "manual"
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStatus {
    pub device_serial: String,
    pub test_id: String,
    pub status: String, // "idle", "running", "pass", "fail", "skipped", "error"
    pub progress: f32,  // 0.0 to 1.0
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub device_serial: String,
    pub test_id: String,
    pub timestamp: String,
    pub level: String, // "info", "warn", "error", "success"
    pub message: String,
}

// ─── App State ────────────────────────────────────────────────────────

pub struct AppState {
    pub atm_path: Mutex<String>,
    pub running: Mutex<bool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            atm_path: Mutex::new(String::new()),
            running: Mutex::new(false),
        }
    }
}

// ─── Commands ─────────────────────────────────────────────────────────

#[tauri::command]
async fn get_devices() -> Result<Vec<DeviceInfo>, String> {
    let output = create_hidden_command("adb")
        .args(["devices", "-l"])
        .output()
        .await
        .map_err(|e| format!("Failed to run adb: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();

    for line in stdout.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('*') {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let serial = parts[0].to_string();
            let status = parts[1].to_string();

            let model = extract_prop(&parts, "model:");
            let device = extract_prop(&parts, "device:");

            // Get Android version and PDA via getprop
            let (android_ver, pda) = if status == "device" {
                let v = get_device_prop(&serial, "ro.build.version.release").await;
                let mut p = get_device_prop(&serial, "ro.build.PDA").await;
                if p == "N/A" || p.is_empty() {
                    p = get_device_prop(&serial, "ro.build.display.id").await;
                }
                (v, p)
            } else {
                ("N/A".to_string(), "N/A".to_string())
            };

            devices.push(DeviceInfo {
                serial,
                model: if model.is_empty() {
                    device
                } else {
                    model
                },
                pda,
                android_version: android_ver,
                status,
            });
        }
    }

    Ok(devices)
}

fn extract_prop(parts: &[&str], prefix: &str) -> String {
    for part in parts {
        if let Some(val) = part.strip_prefix(prefix) {
            return val.to_string();
        }
    }
    String::new()
}

async fn get_device_prop(serial: &str, prop: &str) -> String {
    let output = create_hidden_command("adb")
        .args(["-s", serial, "shell", "getprop", prop])
        .output()
        .await;

    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => "N/A".to_string(),
    }
}

#[tauri::command]
fn get_tools_version(state: State<AppState>) -> String {
    let atm_path = state.atm_path.lock().unwrap().clone();
    if atm_path.is_empty() {
        return "N/A".to_string();
    }
    let test_info_path = PathBuf::from(&atm_path).join("TestInfo.xml");
    if !test_info_path.exists() {
        return "N/A".to_string();
    }
    match std::fs::read_to_string(&test_info_path) {
        Ok(content) => {
            let re = regex::Regex::new(r#"TestList\s+version="([^"]+)""#).unwrap();
            re.captures(&content)
                .map(|cap| cap.get(1).map_or("N/A", |m| m.as_str()).to_string())
                .unwrap_or_else(|| "N/A".to_string())
        }
        Err(_) => "N/A".to_string(),
    }
}

#[tauri::command]
fn get_available_tests(state: State<AppState>) -> Result<Vec<TestItem>, String> {
    let atm_path = state.atm_path.lock().unwrap().clone();
    if atm_path.is_empty() {
        return Ok(get_default_tests());
    }

    let test_info_path = PathBuf::from(&atm_path).join("TestInfo.xml");
    if !test_info_path.exists() {
        return Ok(get_default_tests());
    }

    // Parse TestInfo.xml for available tests
    let content = std::fs::read_to_string(&test_info_path)
        .map_err(|e| format!("Failed to read TestInfo.xml: {}", e))?;

    let mut tests = Vec::new();
    let re_test = regex::Regex::new(
        r#"<(?:Test|Optional)\s+[^>]*name="([^"]+)"[^>]*exefile="([^"]*)"[^>]*summary="([^"]*)"[^>]*type="([^"]*)"[^>]*/?\s*>"#
    ).unwrap();

    for cap in re_test.captures_iter(&content) {
        let name = cap.get(1).map_or("", |m| m.as_str()).to_string();
        let jar = cap.get(2).map_or("", |m| m.as_str()).to_string();
        let summary = cap.get(3).map_or("", |m| m.as_str())
            .replace("&#13;", "")
            .replace("&#10;", " ")
            .replace("&amp;", "&");
        let test_type = cap.get(4).map_or("", |m| m.as_str()).to_string();

        let main_class = match name.as_str() {
            "BVT" => "com.bi.BVT.MainForm",
            "SVT" => "com.ast.svt.MainKt",
            "SDT" => "com.sec.atm.Main",
            "Getprop" => "com.sec.ui.Main",
            "CSCChecker" => "MyApplcaition",
            _ => "com.sec.atm.Main",
        }.to_string();

        tests.push(TestItem {
            id: name.clone().to_lowercase(),
            name: name.clone(),
            jar: if jar.is_empty() {
                format!("{}.jar", name)
            } else {
                jar
            },
            main_class,
            test_type,
            description: summary,
        });
    }

    if tests.is_empty() {
        return Ok(get_default_tests());
    }

    Ok(tests)
}

fn get_default_tests() -> Vec<TestItem> {
    vec![
        TestItem {
            id: "bvt".to_string(),
            name: "BVT".to_string(),
            jar: "BVT.jar".to_string(),
            main_class: "com.bi.BVT.MainForm".to_string(),
            test_type: "auto".to_string(),
            description: "Build Verification Test — Verify permissions, build parameters, CDD compliance".to_string(),
        },
        TestItem {
            id: "svt".to_string(),
            name: "SVT".to_string(),
            jar: "SVT.jar".to_string(),
            main_class: "com.ast.svt.MainKt".to_string(),
            test_type: "auto".to_string(),
            description: "Screenshot Verification Tool — Verify GMS placement rules".to_string(),
        },
        TestItem {
            id: "sdt".to_string(),
            name: "SDT".to_string(),
            jar: "SDT.jar".to_string(),
            main_class: "com.sec.atm.Main".to_string(),
            test_type: "auto".to_string(),
            description: "Samsung Device Test — Verify device properties, security, apps".to_string(),
        },
        TestItem {
            id: "getprop".to_string(),
            name: "Getprop".to_string(),
            jar: "Getprop.jar".to_string(),
            main_class: "com.sec.ui.Main".to_string(),
            test_type: "auto".to_string(),
            description: "Getprop — Verify build properties, security patch level".to_string(),
        },
        TestItem {
            id: "cscchecker".to_string(),
            name: "CSCChecker".to_string(),
            jar: "CSCChecker.jar".to_string(),
            main_class: "MyApplcaition".to_string(),
            test_type: "optional".to_string(),
            description: "CSC Checker — Verify CSC specifications".to_string(),
        },
    ]
}

#[tauri::command]
fn set_atm_path(path: String, state: State<AppState>) -> Result<bool, String> {
    let p = PathBuf::from(&path);
    // Look for ATM_v5.jar or TestInfo.xml to validate
    let valid = p.join("ATM_v5.jar").exists() || p.join("TestInfo.xml").exists();
    if valid {
        *state.atm_path.lock().unwrap() = path;
    }
    Ok(valid)
}

#[tauri::command]
fn get_atm_path(state: State<AppState>) -> String {
    state.atm_path.lock().unwrap().clone()
}

#[tauri::command]
async fn run_test_sequence(
    app: tauri::AppHandle,
    devices: Vec<String>,
    tests: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let atm_path = state.atm_path.lock().unwrap().clone();
    if atm_path.is_empty() {
        return Err("ATM path not set".to_string());
    }

    {
        let mut running = state.running.lock().unwrap();
        if *running {
            return Err("Tests already running".to_string());
        }
        *running = true;
    }

    let tools_path = PathBuf::from(&atm_path).join("tools");

    // Spawn tasks for each device
    let mut handles = Vec::new();
    for device_serial in devices {
        let tests_clone = tests.clone();
        let tools_path_clone = tools_path.clone();
        let atm_path_clone = atm_path.clone();
        let app_clone = app.clone();

        let handle = tokio::spawn(async move {
            for test_id in &tests_clone {
                let jar_name = format!("{}.jar", test_id);
                let jar_path = tools_path_clone.join(&jar_name);

                // Emit test starting
                let _ = app_clone.emit("test-status", TestStatus {
                    device_serial: device_serial.clone(),
                    test_id: test_id.clone(),
                    status: "running".to_string(),
                    progress: 0.0,
                    message: format!("Starting {}...", test_id),
                });

                let _ = app_clone.emit("log-entry", LogEntry {
                    device_serial: device_serial.clone(),
                    test_id: test_id.clone(),
                    timestamp: chrono_now(),
                    level: "info".to_string(),
                    message: format!("▶ Starting {} on {}", test_id, device_serial),
                });

                if !jar_path.exists() && test_id != "getprop" {
                    let _ = app_clone.emit("test-status", TestStatus {
                        device_serial: device_serial.clone(),
                        test_id: test_id.clone(),
                        status: "error".to_string(),
                        progress: 0.0,
                        message: format!("JAR not found: {}", jar_name),
                    });
                    let _ = app_clone.emit("log-entry", LogEntry {
                        device_serial: device_serial.clone(),
                        test_id: test_id.clone(),
                        timestamp: chrono_now(),
                        level: "error".to_string(),
                        message: format!("JAR not found: {}", jar_path.display()),
                    });
                    continue;
                }

                // ─── Execute using specific tool logic ───────────────────────────
                let mut cmd = match test_id.as_str() {
                    "svt" => {
                        // SVT has a confirmed --silent flag
                        let out_dir = atm_path_clone.clone() + "/results/SVT/" + &device_serial;
                        let _ = std::fs::create_dir_all(&out_dir);
                        let mut c = create_hidden_command("java");
                        c.args([
                            "-jar", &jar_path.to_string_lossy(),
                            "--silent",
                            "-s", &device_serial,
                            "-o", &out_dir,
                        ]).current_dir(&atm_path_clone);
                        c
                    }
                    "sdt" | "getprop" | "bvt" => {
                        // Use JAR with auto flags but NO silent flag to show the UI
                        let main_class = match test_id.as_str() {
                            "sdt" => "com.sec.atm.Main",
                            "getprop" => "com.sec.ui.Main",
                            "bvt" => "com.bi.BVT.MainForm",
                            _ => "com.sec.atm.Main",
                        };

                        // Build classpath for these tools
                        let lib_dir = tools_path_clone.join("lib");
                        let mut cp_parts = vec![jar_path.to_string_lossy().to_string()];
                        if lib_dir.exists() {
                            if let Ok(entries) = std::fs::read_dir(&lib_dir) {
                                for entry in entries.flatten() {
                                    let p = entry.path();
                                    if p.extension().map_or(false, |e| e == "jar") {
                                        cp_parts.push(p.to_string_lossy().to_string());
                                    }
                                }
                            }
                        }
                        #[cfg(windows)]
                        let sep = ";";
                        #[cfg(not(windows))]
                        let sep = ":";
                        let cp = cp_parts.join(sep);

                        let mut c = create_hidden_command("java");
                        c.args([
                            "-cp", &cp,
                            main_class,
                            "-s", &device_serial,
                            "-auto",
                            "-e", "auto-start",
                        ]).current_dir(&atm_path_clone);
                        c
                    }
                    _ => {
                        let mut c = create_hidden_command("java");
                        c.args(["-jar", &jar_path.to_string_lossy(), "-s", &device_serial])
                            .current_dir(&atm_path_clone);
                        c
                    }
                };

                let mut child = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = app_clone.emit("test-status", TestStatus {
                            device_serial: device_serial.clone(),
                            test_id: test_id.clone(),
                            status: "error".to_string(),
                            progress: 0.0,
                            message: format!("Failed to start: {}", e),
                        });
                        continue;
                    }
                };

                // Stream stdout
                if let Some(stdout) = child.stdout.take() {
                    let reader = BufReader::new(stdout);
                    let mut lines = reader.lines();
                    let mut line_count = 0;

                    while let Ok(Some(line)) = lines.next_line().await {
                        line_count += 1;
                        let level = if line.contains("FAIL") || line.contains("ERROR") || line.contains("fail") {
                            "error"
                        } else if line.contains("PASS") || line.contains("pass") || line.contains("OK") {
                            "success"
                        } else if line.contains("WARN") || line.contains("warn") {
                            "warn"
                        } else {
                            "info"
                        };

                        let _ = app_clone.emit("log-entry", LogEntry {
                            device_serial: device_serial.clone(),
                            test_id: test_id.clone(),
                            timestamp: chrono_now(),
                            level: level.to_string(),
                            message: line,
                        });

                        // Approximate progress
                        let progress = (line_count as f32 / 100.0).min(0.95);
                        let _ = app_clone.emit("test-status", TestStatus {
                            device_serial: device_serial.clone(),
                            test_id: test_id.clone(),
                            status: "running".to_string(),
                            progress,
                            message: format!("Processing... ({} lines)", line_count),
                        });
                    }
                }

                // Wait for process to complete
                let exit_status = child.wait().await;
                let (status, level) = match exit_status {
                    Ok(s) if s.success() => ("pass", "success"),
                    Ok(_) => ("fail", "error"),
                    Err(_) => ("error", "error"),
                };

                let _ = app_clone.emit("test-status", TestStatus {
                    device_serial: device_serial.clone(),
                    test_id: test_id.clone(),
                    status: status.to_string(),
                    progress: 1.0,
                    message: format!("{} completed: {}", test_id, status.to_uppercase()),
                });

                let _ = app_clone.emit("log-entry", LogEntry {
                    device_serial: device_serial.clone(),
                    test_id: test_id.clone(),
                    timestamp: chrono_now(),
                    level: level.to_string(),
                    message: format!(
                        "{} {} on {} - {}",
                        if status == "pass" { "PASS" } else { "FAIL" },
                        test_id,
                        device_serial,
                        status.to_uppercase()
                    ),
                });
            }
        });

        handles.push(handle);
    }

    // Wait for all devices to complete
    for handle in handles {
        let _ = handle.await;
    }

    // Mark as not running
    let app_state = app.state::<AppState>();
    *app_state.running.lock().unwrap() = false;

    let _ = app.emit("execution-complete", true);

    Ok(())
}

#[tauri::command]
async fn stop_tests(state: State<'_, AppState>) -> Result<(), String> {
    *state.running.lock().unwrap() = false;
    // In a production app, we'd track child PIDs and kill them
    Ok(())
}

#[tauri::command]
fn is_running(state: State<AppState>) -> bool {
    *state.running.lock().unwrap()
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

// ─── Entry Point ──────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_devices,
            get_available_tests,
            set_atm_path,
            get_atm_path,
            run_test_sequence,
            stop_tests,
            is_running,
            get_tools_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
