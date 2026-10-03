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
pub mod util;
pub mod xml;

use tauri::{Manager, RunEvent, WindowEvent};

pub fn run() {
    let argv: Vec<String> = std::env::args().collect();
    let args = cli::parse(&argv);

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
            commands::reload_plugins,
        ])
        .setup(move |app| {
            dbg_log!("setup, args: {start_args:?}");
            let handle = app.handle().clone();
            // WebKitGTK streams media reliably only over HTTP (see mediaserver.rs).
            #[cfg(target_os = "linux")]
            mediaserver::start();
            integration::start(&handle);
            app::start_idle_watcher(handle.clone());
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
