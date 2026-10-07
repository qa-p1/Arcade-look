//! Arcade Look: a universal, cross-platform Quick Look.

pub mod app;
pub mod archive;
pub mod cli;
pub mod commands;
pub mod config;
pub mod detect;
pub mod font;
pub mod fsx;
pub mod imaging;
pub mod integration;
pub mod link;
pub mod link_consumer;
pub mod markdown;
pub mod media;
pub mod mediaserver;
pub mod office;
pub mod plugins;
pub mod protocol;
pub mod rtf;
pub mod slides;
pub mod table;
pub mod text;
pub mod tray;
pub mod util;
pub mod xml;

#[cfg(all(feature = "e2e", unix))]
mod e2e;

use tauri::{Manager, RunEvent, WindowEvent};

pub fn run() {
    let argv: Vec<String> = std::env::args().collect();
    let args = cli::parse(&argv);

    if args.invoke {
        std::process::exit(link::serve_oneshot());
    }

    if args.manifest {
        integration::attach_console();
        println!(
            "{}",
            link::manifest(&config::try_load().unwrap_or_default()).to_json()
        );
        return;
    }

    if args.help || args.version || args.install || args.uninstall {
        integration::attach_console();
        if args.version {
            println!("arcade-look {}", env!("CARGO_PKG_VERSION"));
        } else if args.help {
            print!("{}", cli::HELP);
        } else {
            let r = if args.install {
                integration::install()
            } else {
                integration::uninstall()
            };
            match r {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        }
        return;
    }

    tray::wait_for_predecessor();
    std::thread::spawn(util::clean_temp);
    let config = config::load();
    let plugins = plugins::load_all(config.plugins);
    let start_args = args.clone();
    // The shortcut plugin grabs keys through X11/Win32/Carbon at startup; only load it when a
    // shortcut is configured so a pure-Wayland session can never fail to launch because of it.
    let want_shortcut = !config.global_shortcut.trim().is_empty();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            let args = cli::parse(&argv);
            app::handle_args(app, &args, Some(std::path::Path::new(&cwd)));
        }))
        .plugin(tauri_plugin_opener::init())
        .manage(app::AppState::new(config, plugins, args.service))
        .register_asynchronous_uri_scheme_protocol("alook", |_ctx, request, responder| {
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(protocol::handle(&request));
            });
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                app::hide(window.app_handle());
            }
            // The window may get focus after `show` returned (it maps asynchronously): hand
            // keyboard focus to the web view so Space/Esc/arrows work without a click.
            #[cfg(target_os = "linux")]
            if let WindowEvent::Focused(true) = event {
                if let Some(w) = window.app_handle().get_webview_window(app::MAIN) {
                    integration::linux::unpin_size(&w);
                    app::focus_webview(&w);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::inspect,
            commands::read_text,
            commands::render_markdown,
            commands::markdown_batch,
            commands::read_bytes,
            commands::list_archive,
            commands::extract_entry,
            commands::read_table,
            commands::read_document,
            commands::read_slides,
            commands::font_info,
            commands::audio_info,
            commands::image_info,
            commands::list_dir,
            commands::dir_size,
            commands::neighbor,
            commands::run_plugin,
            commands::open_default,
            commands::open_url,
            commands::reveal,
            commands::log,
            commands::show_window,
            commands::hide_window,
            commands::quit,
            commands::set_info_panel,
            commands::navigate_external,
            commands::install_integration,
            commands::get_autostart,
            commands::set_autostart,
            commands::integration_status,
            commands::set_config,
            commands::reload_plugins,
            link_consumer::link_available,
            link_consumer::link_actions,
            link_consumer::link_invoke,
            link_consumer::link_cancel,
            link_consumer::link_connected,
            link_consumer::link_get,
            link_consumer::link_shortcut_owner,
            link_consumer::link_shortcut_recording,
            link_consumer::link_save_shortcut,
        ])
        .setup(move |app| {
            dbg_log!("setup, args: {start_args:?}");
            let handle = app.handle().clone();
            // `--quit` with nothing to quit (installers run it unconditionally): exit without
            // starting listeners or flashing a tray icon.
            if start_args.quit {
                handle.exit(0);
                return Ok(());
            }
            // A background utility: no Dock icon, just the menu bar item and the preview.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            // WebKitGTK streams media reliably only over HTTP (see mediaserver.rs).
            #[cfg(target_os = "linux")]
            mediaserver::start();
            integration::start(&handle);
            integration::refresh_autostart();
            integration::first_run_setup(&handle);
            tray::start(&handle);
            app::start_idle_watcher(handle.clone());
            link::start(&handle, &handle.state::<app::AppState>().config());
            link_consumer::start(&handle);
            #[cfg(all(feature = "e2e", unix))]
            e2e::start(&handle);
            app::handle_args(&handle, &start_args, None);
            Ok(())
        });

    let builder = if want_shortcut {
        builder.plugin(integration::shortcut_plugin())
    } else {
        builder
    };

    let app = match builder.build(tauri::generate_context!()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("arcade-look: failed to start: {e}");
            std::process::exit(1);
        }
    };
    app.run(|app, event| {
        if let RunEvent::Exit = event {
            link::stop();
        }
        if let RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            // The last window was destroyed (idle release): keep listening if needed.
            if app::keep_alive(app) {
                api.prevent_exit();
            }
        }
    });
}
