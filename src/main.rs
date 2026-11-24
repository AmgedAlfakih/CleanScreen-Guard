#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod blur;
mod capture;
mod config;
mod detector;
mod overlay;
mod tracker;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;
use wry::WebViewBuilder;

use crate::blur::{blur_region_bgra, BlurPatch};
use crate::capture::DesktopCapturer;
use crate::config::{
    get_models_dir, is_startup_enabled, set_startup_enabled, AppConfig, OFFICIAL_GITHUB_URL,
};
use crate::detector::{list_available_models, FastNSFWDetector};
use crate::overlay::BlurOverlay;
use crate::tracker::RegionTracker;

const SETTINGS_HTML: &str = include_str!("../assets/ui/settings.html");
const ACTIVE_ICON_BYTES: &[u8] = include_bytes!("../assets/app_icon.ico");
const PAUSED_ICON_BYTES: &[u8] = include_bytes!("../assets/app_icon_paused.ico");

#[derive(Debug)]
enum UserEvent {
    TrayClick,
    MenuSettings,
    MenuTogglePause,
    MenuQuit,
    StatusUpdated(String),
    IpcMessage(String),
}

#[derive(Deserialize)]
#[serde(tag = "cmd")]
enum IpcCmd {
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "update_config")]
    UpdateConfig { config: AppConfig, reload_model: bool },
    #[serde(rename = "set_autostart")]
    SetAutostart { enabled: bool },
    #[serde(rename = "open_models")]
    OpenModels,
    #[serde(rename = "open_github")]
    OpenGithub,
    #[serde(rename = "reset_defaults")]
    ResetDefaults,
    #[serde(rename = "hide_window")]
    HideWindow,
}

struct AIJob {
    frame_seq: u64,
    width: usize,
    height: usize,
    bgra: Vec<u8>,
    cfg: AppConfig,
}

struct AIResult {
    frame_seq: u64,
    padded_detections: Vec<((i32, i32, i32, i32), String)>,
}

struct SharedState {
    cfg: Mutex<AppConfig>,
    paused: AtomicBool,
    running: AtomicBool,
    reload_requested: AtomicBool,
    status_text: Mutex<String>,
    ai_mailbox: (Mutex<Option<AIJob>>, Condvar),
    ai_result: Mutex<Option<AIResult>>,
}

fn load_tray_icon(bytes: &[u8]) -> Icon {
    if let Ok(img) = image::load_from_memory(bytes) {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        if let Ok(icon) = Icon::from_rgba(rgba.into_raw(), w, h) {
            return icon;
        }
    }
    Icon::from_rgba(vec![220, 38, 38, 255].repeat(16 * 16), 16, 16).unwrap()
}

fn downscale_bgra_to_gray320(bgra: &[u8], w: usize, h: usize) -> (Vec<u8>, usize, usize) {
    let scale = 320.0f32 / (w.max(h) as f32);
    let gw = ((w as f32 * scale).round() as usize).max(16);
    let gh = ((h as f32 * scale).round() as usize).max(16);
    let mut gray = vec![0u8; gw * gh];
    let sx = w as f32 / gw as f32;
    let sy = h as f32 / gh as f32;
    for y in 0..gh {
        let src_y = ((y as f32 + 0.5) * sy) as usize;
        let row = src_y.min(h - 1) * w * 4;
        for x in 0..gw {
            let src_x = ((x as f32 + 0.5) * sx) as usize;
            let i = row + src_x.min(w - 1) * 4;
            let b = bgra[i] as u32;
            let g = bgra[i + 1] as u32;
            let r = bgra[i + 2] as u32;
            gray[y * gw + x] = ((r * 77 + g * 150 + b * 29) >> 8) as u8;
        }
    }
    (gray, gw, gh)
}

fn motion_change_ratio(prev: &[u8], curr: &[u8]) -> f32 {
    if prev.len() != curr.len() || curr.is_empty() {
        return 1.0;
    }
    let mut changed = 0usize;
    for (&a, &b) in prev.iter().zip(curr.iter()) {
        if a.abs_diff(b) > 18 {
            changed += 1;
        }
    }
    changed as f32 / curr.len() as f32
}

fn main() {
    // Single-instance guard via Win32 Named Mutex
    unsafe {
        let name: Vec<u16> = "Global\\CleanScreenGuard_Rust_v2\0".encode_utf16().collect();
        let _mutex = CreateMutexW(None, true, PCWSTR(name.as_ptr()));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
    }

    let cfg = AppConfig::load();
    let shared = Arc::new(SharedState {
        cfg: Mutex::new(cfg.clone()),
        paused: AtomicBool::new(false),
        running: AtomicBool::new(true),
        reload_requested: AtomicBool::new(false),
        status_text: Mutex::new("Loading DirectML model...".to_string()),
        ai_mailbox: (Mutex::new(None), Condvar::new()),
        ai_result: Mutex::new(None),
    });

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // Dedicated Async AI Thread (DirectML / CPU)
    {
        let st = Arc::clone(&shared);
        let p_status = proxy.clone();
        thread::Builder::new()
            .name("CleanScreen-AI".into())
            .spawn(move || {
                let init_cfg = st.cfg.lock().unwrap().clone();
                let mut detector = match FastNSFWDetector::new(
                    &init_cfg.primary_model,
                    &init_cfg.secondary_model,
                    &init_cfg.compute_device,
                    init_cfg.use_fp16,
                ) {
                    Ok(d) => {
                        let s = d.status_text();
                        *st.status_text.lock().unwrap() = s.clone();
                        let _ = p_status.send_event(UserEvent::StatusUpdated(s));
                        d
                    }
                    Err(e) => {
                        let msg = format!("Model init error: {e}");
                        *st.status_text.lock().unwrap() = msg.clone();
                        let _ = p_status.send_event(UserEvent::StatusUpdated(msg));
                        return;
                    }
                };

                let (lock, cvar) = &st.ai_mailbox;
                while st.running.load(Ordering::Relaxed) {
                    if st.reload_requested.swap(false, Ordering::SeqCst) {
                        let c = st.cfg.lock().unwrap().clone();
                        let _ = detector.reload_models(
                            &c.primary_model,
                            &c.secondary_model,
                            &c.compute_device,
                            c.use_fp16,
                        );
                        let s = detector.status_text();
                        *st.status_text.lock().unwrap() = s.clone();
                        let _ = p_status.send_event(UserEvent::StatusUpdated(s));
                    }

                    let job = {
                        let mut guard = lock.lock().unwrap();
                        if guard.is_none() {
                            let (g, _) = cvar.wait_timeout(guard, Duration::from_millis(120)).unwrap();
                            guard = g;
                        }
                        guard.take()
                    };

                    let Some(job) = job else { continue };
                    let labels = job.cfg.active_labels();
                    let dets = detector.detect(
                        &job.bgra,
                        job.width,
                        job.height,
                        &labels,
                        job.cfg.confidence_threshold,
                    );
                    let pad = job.cfg.box_padding_px;
                    let padded = dets
                        .into_iter()
                        .map(|d| {
                            (
                                (
                                    (d.x1 - pad).max(0),
                                    (d.y1 - pad).max(0),
                                    (d.x2 + pad).min(job.width as i32),
                                    (d.y2 + pad).min(job.height as i32),
                                ),
                                format!("{} ({:.2})", d.label, d.score),
                            )
                        })
                        .collect();
                    *st.ai_result.lock().unwrap() = Some(AIResult {
                        frame_seq: job.frame_seq,
                        padded_detections: padded,
                    });
                }
            })
            .ok();
    }

    // Dedicated Non-Blocking Capture + Pure-Rust Motion Tracking + Native Overlay Thread
    {
        let st = Arc::clone(&shared);
        thread::Builder::new()
            .name("CleanScreen-Capture".into())
            .spawn(move || {
                let mut overlay = match BlurOverlay::new() {
                    Ok(o) => o,
                    Err(_) => return,
                };
                let mut capturer = match DesktopCapturer::new(0) {
                    Ok(c) => c,
                    Err(_) => return,
                };

                let mut tracker = RegionTracker::new();
                let mut prev_gray: Option<(Vec<u8>, usize, usize)> = None;
                let mut last_frame: Option<(Vec<u8>, usize, usize)> = None;
                let mut frame_seq: u64 = 0;
                let start = Instant::now();
                let mut last_ai_submit = 0.0f64;
                let mut last_hit_time = 0.0f64;
                let mut last_scene_cut_seq: u64 = 0;
                let mut last_scan_confirmed_tracks = false;
                let mut ai_in_flight = false;
                let mut needs_post_motion_scan = false;

                while st.running.load(Ordering::Relaxed) {
                    let t0 = Instant::now();
                    if st.paused.load(Ordering::Relaxed) {
                        tracker.clear();
                        overlay.clear();
                        last_scan_confirmed_tracks = false;
                        ai_in_flight = false;
                        needs_post_motion_scan = false;
                        last_scene_cut_seq = 0;
                        thread::sleep(Duration::from_millis(80));
                        continue;
                    }

                    let cfg = st.cfg.lock().unwrap().clone();
                    let now = start.elapsed().as_secs_f64();

                    if ai_in_flight && (now - last_ai_submit) > 1.5 {
                        ai_in_flight = false;
                    }

                    // 1. Drain async AI result without blocking
                    let mut reconciled = false;
                    if let Some(res) = st.ai_result.lock().unwrap().take() {
                        ai_in_flight = false;
                        if !res.padded_detections.is_empty() {
                            last_hit_time = now;
                            last_scan_confirmed_tracks = true;
                        } else {
                            last_scan_confirmed_tracks = false;
                        }

                        let cleared_by_scene_cut = res.padded_detections.is_empty()
                            && last_scene_cut_seq > 0
                            && res.frame_seq >= last_scene_cut_seq;
                        if last_scene_cut_seq > 0 && res.frame_seq >= last_scene_cut_seq {
                            last_scene_cut_seq = 0;
                        }

                        if cleared_by_scene_cut {
                            tracker.clear();
                            overlay.clear();
                            reconciled = true;
                        } else if let Some((ref bgra, w, h)) = last_frame {
                            let boxes = tracker.update(
                                &res.padded_detections,
                                now,
                                cfg.box_hold_seconds,
                                res.frame_seq,
                                w,
                                h,
                            );
                            let patches: Vec<BlurPatch> = boxes
                                .into_iter()
                                .filter_map(|(b, l)| blur_region_bgra(bgra, w, h, b, cfg.blur_strength, l))
                                .collect();
                            overlay.update_patches(&patches, cfg.debug_overlay);
                            reconciled = true;
                        }
                    }

                    // 2. Grab native BGRA frame via DXGI Desktop Duplication
                    let frame = capturer.grab(16);
                    if let Some(f) = frame {
                        frame_seq += 1;
                        let (curr_gray, gw, gh) = downscale_bgra_to_gray320(&f.bgra, f.width, f.height);
                        let change = prev_gray
                            .as_ref()
                            .map(|(pg, _, _)| motion_change_ratio(pg, &curr_gray))
                            .unwrap_or(1.0);

                        let is_still = prev_gray.is_some() && change < 0.0015;
                        if is_still {
                            if tracker.has_tracks() {
                                if last_scan_confirmed_tracks && !ai_in_flight {
                                    tracker.keep_alive(now);
                                } else if tracker.prune_expired(now, cfg.box_hold_seconds) {
                                    let patches: Vec<BlurPatch> = tracker
                                        .get_boxes()
                                        .into_iter()
                                        .filter_map(|(b, l)| {
                                            blur_region_bgra(&f.bgra, f.width, f.height, b, cfg.blur_strength, l)
                                        })
                                        .collect();
                                    overlay.update_patches(&patches, cfg.debug_overlay);
                                }
                            }
                            if !needs_post_motion_scan && (now - last_ai_submit) < 2.5 {
                                prev_gray = Some((curr_gray, gw, gh));
                                last_frame = Some((f.bgra, f.width, f.height));
                                thread::sleep(Duration::from_millis(80));
                                continue;
                            }
                        } else {
                            needs_post_motion_scan = true;
                        }

                        // Adaptive 3-Tier Scan Rate
                        let base_hz = cfg.ai_scan_fps.max(1) as f64;
                        let eff_hz = if !cfg.adaptive_scan_rate
                            || tracker.has_tracks()
                            || (now - last_hit_time) < 2.5
                        {
                            base_hz
                        } else {
                            (base_hz * 0.5).clamp(2.0, 4.0)
                        };
                        let scene_cut = change >= 0.14 && (now - last_ai_submit) >= 0.05;
                        if scene_cut {
                            last_hit_time = now;
                            last_scene_cut_seq = frame_seq;
                            last_scan_confirmed_tracks = false;
                            tracker.reset_velocities();
                        }

                        if prev_gray.is_none() || scene_cut || (now - last_ai_submit) >= (1.0 / eff_hz) {
                            last_ai_submit = now;
                            needs_post_motion_scan = false;
                            ai_in_flight = true;
                            let (lock, cvar) = &st.ai_mailbox;
                            *lock.lock().unwrap() = Some(AIJob {
                                frame_seq,
                                width: f.width,
                                height: f.height,
                                bgra: f.bgra.clone(),
                                cfg: cfg.clone(),
                            });
                            cvar.notify_one();
                        }

                        if cfg.optical_flow_tracking && !reconciled && tracker.has_tracks() {
                            if let Some((ref pg, pgw, pgh)) = prev_gray {
                                if pgw == gw && pgh == gh {
                                    let boxes = tracker.track_motion(
                                        pg,
                                        &curr_gray,
                                        gw,
                                        gh,
                                        f.width,
                                        f.height,
                                        now,
                                        cfg.box_hold_seconds,
                                        frame_seq,
                                    );
                                    let patches: Vec<BlurPatch> = boxes
                                        .into_iter()
                                        .filter_map(|(b, l)| {
                                            blur_region_bgra(&f.bgra, f.width, f.height, b, cfg.blur_strength, l)
                                        })
                                        .collect();
                                    overlay.update_patches(&patches, cfg.debug_overlay);
                                }
                            }
                        }

                        prev_gray = Some((curr_gray, gw, gh));
                        last_frame = Some((f.bgra, f.width, f.height));
                    } else {
                        let ai_interval = 1.0 / cfg.ai_scan_fps.max(1) as f64;
                        if needs_post_motion_scan && !ai_in_flight && (now - last_ai_submit) >= ai_interval {
                            if let Some((ref bgra, w, h)) = last_frame {
                                last_ai_submit = now;
                                needs_post_motion_scan = false;
                                last_scan_confirmed_tracks = false;
                                ai_in_flight = true;
                                let (lock, cvar) = &st.ai_mailbox;
                                *lock.lock().unwrap() = Some(AIJob {
                                    frame_seq,
                                    width: w,
                                    height: h,
                                    bgra: bgra.clone(),
                                    cfg: cfg.clone(),
                                });
                                cvar.notify_one();
                            }
                        } else if tracker.has_tracks() {
                            if last_scan_confirmed_tracks && !ai_in_flight {
                                tracker.keep_alive(now);
                            } else if tracker.prune_expired(now, cfg.box_hold_seconds) {
                                if let Some((ref bgra, w, h)) = last_frame {
                                    let patches: Vec<BlurPatch> = tracker
                                        .get_boxes()
                                        .into_iter()
                                        .filter_map(|(b, l)| {
                                            blur_region_bgra(bgra, w, h, b, cfg.blur_strength, l)
                                        })
                                        .collect();
                                    overlay.update_patches(&patches, cfg.debug_overlay);
                                } else {
                                    overlay.clear();
                                }
                            }
                        }
                    }

                    let target_ms = if tracker.has_tracks() {
                        1000 / cfg.target_fps.clamp(10, 30) as u64
                    } else {
                        80
                    };
                    let elapsed = t0.elapsed().as_millis() as u64;
                    if elapsed < target_ms {
                        thread::sleep(Duration::from_millis(target_ms - elapsed));
                    }
                }
                overlay.clear();
            })
            .ok();
    }

    // System Tray Icon & Menu
    let active_icon = load_tray_icon(ACTIVE_ICON_BYTES);
    let paused_icon = load_tray_icon(PAUSED_ICON_BYTES);

    let tray_menu = Menu::new();
    let item_settings = MenuItem::new("Settings...", true, None);
    let item_pause = MenuItem::new("Pause Protection", true, None);
    let item_quit = MenuItem::new("Quit", true, None);
    let _ = tray_menu.append(&item_settings);
    let _ = tray_menu.append(&item_pause);
    let _ = tray_menu.append(&PredefinedMenuItem::separator());
    let _ = tray_menu.append(&item_quit);

    let id_settings = item_settings.id().clone();
    let id_pause = item_pause.id().clone();
    let id_quit = item_quit.id().clone();

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_menu_on_left_click(false)
        .with_tooltip("CleanScreen Guard v2.0 (Rust) — Click for Settings")
        .with_icon(active_icon.clone())
        .build()
        .expect("Failed to create tray icon");

    let p_menu = proxy.clone();
    MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
        if ev.id == id_settings {
            let _ = p_menu.send_event(UserEvent::MenuSettings);
        } else if ev.id == id_pause {
            let _ = p_menu.send_event(UserEvent::MenuTogglePause);
        } else if ev.id == id_quit {
            let _ = p_menu.send_event(UserEvent::MenuQuit);
        }
    }));

    let p_tray = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = ev
        {
            let _ = p_tray.send_event(UserEvent::TrayClick);
        }
    }));

    // Hidden Settings Window (Windows 11 Edge WebView2 — 0 MB extra installer bloat)
    let window = WindowBuilder::new()
        .with_title("CleanScreen Guard — Settings")
        .with_inner_size(tao::dpi::LogicalSize::new(560.0, 680.0))
        .with_visible(false)
        .build(&event_loop)
        .expect("Failed to create settings window");

    let p_ipc = proxy.clone();
    let webview = WebViewBuilder::new()
        .with_html(SETTINGS_HTML)
        .with_ipc_handler(move |req| {
            let _ = p_ipc.send_event(UserEvent::IpcMessage(req.body().to_string()));
        })
        .build(&window)
        .expect("Failed to initialize Edge WebView2");

    let push_state = |wv: &wry::WebView, st: &SharedState| {
        let payload = serde_json::json!({
            "config": *st.cfg.lock().unwrap(),
            "models": list_available_models(),
            "autostart": is_startup_enabled(),
            "status": *st.status_text.lock().unwrap(),
        });
        let js = format!("window.__loadState && window.__loadState({payload});");
        let _ = wv.evaluate_script(&js);
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                window.set_visible(false);
            }
            Event::UserEvent(UserEvent::TrayClick) | Event::UserEvent(UserEvent::MenuSettings) => {
                push_state(&webview, &shared);
                window.set_visible(true);
                window.set_focus();
            }
            Event::UserEvent(UserEvent::MenuTogglePause) => {
                let now_paused = !shared.paused.load(Ordering::Relaxed);
                shared.paused.store(now_paused, Ordering::Relaxed);
                item_pause.set_text(if now_paused {
                    "Resume Protection"
                } else {
                    "Pause Protection"
                });
                let _ = tray.set_icon(Some(if now_paused {
                    paused_icon.clone()
                } else {
                    active_icon.clone()
                }));
            }
            Event::UserEvent(UserEvent::MenuQuit) => {
                shared.running.store(false, Ordering::Relaxed);
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(UserEvent::StatusUpdated(_)) => {
                push_state(&webview, &shared);
            }
            Event::UserEvent(UserEvent::IpcMessage(body)) => {
                if let Ok(cmd) = serde_json::from_str::<IpcCmd>(&body) {
                    match cmd {
                        IpcCmd::Ready => push_state(&webview, &shared),
                        IpcCmd::UpdateConfig { config, reload_model } => {
                            config.save();
                            *shared.cfg.lock().unwrap() = config;
                            if reload_model {
                                shared.reload_requested.store(true, Ordering::SeqCst);
                                shared.ai_mailbox.1.notify_one();
                            }
                        }
                        IpcCmd::SetAutostart { enabled } => set_startup_enabled(enabled),
                        IpcCmd::OpenModels => {
                            let _ = std::process::Command::new("explorer")
                                .arg(get_models_dir())
                                .spawn();
                        }
                        IpcCmd::OpenGithub => {
                            let _ = std::process::Command::new("cmd")
                                .args(["/C", "start", "", OFFICIAL_GITHUB_URL])
                                .spawn();
                        }
                        IpcCmd::ResetDefaults => {
                            let lang = shared.cfg.lock().unwrap().language.clone();
                            let mut def = AppConfig::default();
                            def.language = lang;
                            def.save();
                            *shared.cfg.lock().unwrap() = def;
                            shared.reload_requested.store(true, Ordering::SeqCst);
                            shared.ai_mailbox.1.notify_one();
                            push_state(&webview, &shared);
                        }
                        IpcCmd::HideWindow => window.set_visible(false),
                    }
                }
            }
            _ => {}
        }
    });
}
