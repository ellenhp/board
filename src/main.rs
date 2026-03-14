use board::{Arrival, fetch_with_retry, format_time, recalculate_and_filter};
use slint::{Model, VecModel};
#[cfg(feature = "framebuffer")]
use slint_backend_linuxfb::LinuxFbPlatformBuilder;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use tokio::sync::Mutex;

slint::slint! {
export struct ArrivalData {
    destination: string,
    time: string,
    route_color: color,
    route_label: string,
}

export component MainUI inherits Window {
    in-out property <[ArrivalData]> arrivals;
    in-out property <string> current_time;
    in-out property <string> stop_name;
    background: black;

    VerticalLayout {
        alignment: start;
        padding-left: 30px;
        padding-right: 30px;
        padding-top: 10px;
        padding-bottom: 30px;
        spacing: 10px;

        // Header: time left-aligned, stop name centered
        Rectangle {
            height: 40px;

            // Time on the left
            Text {
                x: 0;
                text: current_time;
                font-size: 24pt;
                font-weight: 800;
                color: white;
                vertical-alignment: center;
            }

            // Stop name centered
            Text {
                text: stop_name;
                font-size: 24pt;
                font-weight: 800;
                color: white;
                horizontal-alignment: center;
                vertical-alignment: center;
                width: 100%;
            }
        }

        for arrival in arrivals: HorizontalLayout {
            spacing: 15px;

            // Route circle
            Rectangle {
                width: 60px;
                height: 60px;
                border-radius: 25px;
                background: arrival.route_color;

                Text {
                    text: arrival.route_label;
                    font-size: 28pt;
                    font-weight: 900;
                    color: black;
                    horizontal-alignment: center;
                    vertical-alignment: center;
                }
            }

            // Destination
            Text {
                text: arrival.destination;
                font-size: 40pt;
                font-weight: 600;
                color: white;
                vertical-alignment: center;
                overflow: clip;
                width: 500px;
            }

            // Time
            Text {
                text: arrival.time;
                font-size: 40pt;
                font-weight: 800;
                color: white;
                vertical-alignment: center;
                horizontal-alignment: right;
                width: 150px;
            }
        }
    }
}
}

struct State {
    main_ui: MainUI,
    stop_ids: Vec<String>,
    cache: Mutex<HashMap<String, Vec<Arrival>>>,
    raw_arrivals: Mutex<Vec<Arrival>>,
    clock_offset_ms: Cell<i64>,
    arrivals_model: Rc<VecModel<ArrivalData>>,
}

async fn fetch_arrivals(state: &State) {
    let mut all_arrivals: Vec<Arrival> = Vec::new();

    for stop_id in &state.stop_ids {
        match fetch_with_retry(stop_id).await {
            Ok((arrivals, clock_offset)) => {
                state.clock_offset_ms.set(clock_offset);
                state
                    .cache
                    .lock()
                    .await
                    .insert(stop_id.clone(), arrivals.clone());
                all_arrivals.extend(arrivals);
            }
            Err(_) => {
                if let Some(cached) = state.cache.lock().await.get(stop_id) {
                    all_arrivals.extend(cached.clone());
                }
            }
        }
    }

    *state.raw_arrivals.lock().await = all_arrivals;
    refresh_display(state).await;
}

async fn refresh_display(state: &State) {
    let mut arrivals = state.raw_arrivals.lock().await.clone();
    recalculate_and_filter(&mut arrivals, state.clock_offset_ms.get());

    let model = &state.arrivals_model;
    for (i, a) in arrivals.iter().enumerate() {
        let [r, g, b] = a.route_color;
        let data = ArrivalData {
            destination: a.destination.clone().into(),
            time: format_time(a.minutes).into(),
            route_color: slint::Color::from_rgb_u8(r, g, b),
            route_label: a.route_label.clone().into(),
        };
        if i < model.row_count() {
            model.set_row_data(i, data);
        } else {
            model.push(data);
        }
    }
    while model.row_count() > arrivals.len() {
        model.remove(model.row_count() - 1);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("Usage: board <stop_name> <stop_id> [stop_id...]");
        std::process::exit(1);
    }
    let stop_name = args[0].clone();
    let stop_ids: Vec<String> = args[1..].to_vec();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let _tokio = rt.enter();

    #[cfg(feature = "framebuffer")]
    {
        let platform = LinuxFbPlatformBuilder::new()
            .with_framebuffer("/dev/fb0")
            .with_input_autodiscovery(true)
            .build()
            .unwrap();
        slint::platform::set_platform(Box::new(platform)).unwrap();
    }

    let arrivals_model = Rc::new(VecModel::<ArrivalData>::default());
    let main_ui = MainUI::new().unwrap();
    main_ui.set_arrivals(arrivals_model.clone().into());

    let state = Rc::new(State {
        main_ui,
        stop_ids,
        cache: Mutex::new(HashMap::new()),
        raw_arrivals: Mutex::new(Vec::new()),
        clock_offset_ms: Cell::new(0),
        arrivals_model,
    });

    state.main_ui.set_stop_name(stop_name.into());

    // Update clock + refresh display every 10s
    {
        let state = state.clone();
        slint::spawn_local(async move {
            let mut tick = 0u32;
            loop {
                #[cfg(feature = "watchdog")]
                {
                    use subprocess::Exec;
                    tokio::spawn(async {
                        let _ = Exec::cmd("/usr/bin/NxExe")
                            .arg("watchdog")
                            .arg("10")
                            .start();
                    });
                }

                let now = chrono::Local::now();
                state
                    .main_ui
                    .set_current_time(now.format("%H:%M:%S").to_string().into());
                if tick % 10 == 0 {
                    refresh_display(&state).await;
                    #[cfg(feature = "reminder")]
                    for arrival in state.raw_arrivals.lock().await.clone() {
                        use std::time::{SystemTime, UNIX_EPOCH};
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .expect("duration since epoch failed")
                            .as_millis() as i64;
                        let req_path = format!("{}.req", arrival.route_label);
                        if std::fs::exists(&req_path).unwrap()
                            && arrival.arrival_time_ms > now + 300_000
                            && arrival.arrival_time_ms < now + 420_000
                        {
                            std::fs::remove_file(&req_path).unwrap();
                            use subprocess::Exec;
                            let filename = format!("{}.wav", &arrival.route_label);
                            tokio::spawn(async {
                                let _ = Exec::cmd("aplay").arg(filename).start();
                            });
                        }
                    }
                }
                tick = tick.wrapping_add(1);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        })
        .unwrap();
    }

    #[cfg(feature = "reminder")]
    slint::spawn_local(board::listener::listen_udp()).unwrap();

    // Sync system clock from HTTP every 5 minutes
    slint::spawn_local(board::clock::run_clock_sync()).unwrap();

    // Fetch from API every 60s
    {
        let state = state.clone();
        slint::spawn_local(async move {
            loop {
                fetch_arrivals(&state).await;
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        })
        .unwrap();
    }

    state.main_ui.run().unwrap();
}
