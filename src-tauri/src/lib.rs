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
pub mod installer;
pub mod installer_ui;
pub mod integration;
pub mod link;
pub mod link_consumer;
pub mod markdown;
pub mod media;
pub mod mediaserver;
pub mod office;
pub mod plugins;
pub mod portable_settings;
pub mod protocol;
pub mod rtf;
pub mod slides;
pub mod table;
pub mod text;
pub mod tray;
pub mod util;
pub mod window_geometry;
pub mod xml;

#[cfg(all(feature = "e2e", unix))]
mod e2e;

use tauri::{Manager, RunEvent, WindowEvent};

pub fn run() {
    let argv: Vec<String> = std::env::args().collect();
    let args = cli::parse(&argv);

    if args.status {
        integration::attach_console();
        cli::print_status();
        return;
    }

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

    if args.help
        || args.version
        || args.install_integration
        || args.uninstall_integration
        || args.install
        || args.uninstall
        || args.repair
        || args.integration_status
        || args.restart
    {
        integration::attach_console();
        if args.version {
            println!("arcade-look {}", env!("CARGO_PKG_VERSION"));
        } else if args.help {
            print!("{}", cli::HELP);
        } else if args.install_integration {
            cli::run_integration(true);
        } else if args.uninstall_integration {
            cli::run_integration(false);
        } else if args.restart {
            cli::restart_running();
        } else {
            cli::run_maintenance(&args);
        }
        return;
    }

    #[cfg(target_os = "linux")]
    if integration::linux::forward_to_running(&argv) {
        return;
    }

    if let Err(error) = config::migrate() {
        eprintln!("arcade-look: could not migrate settings; original files were kept: {error}");
        std::process::exit(1);
    }

    if installer::first_run_prompt(&argv) {
        return;
    }

    // Files on the command line are relative to the user's directory; the
    // previews are webviews, and WebKit needs the AppImage's own directory.
    let user_dir = std::env::current_dir().ok();
    arcade_link::childenv::enter_image_dir();

    #[cfg(target_os = "linux")]
    gtk::glib::set_prgname(Some("dev.arcade.Look"));
    tray::wait_for_predecessor();
    std::thread::spawn(util::clean_temp);
    if let Err(error) = integration::migrate_entries() {
        eprintln!("arcade-look: old integration entries were kept: {error}");
        std::process::exit(1);
    }
    let config = config::load();
    let plugins = plugins::load_all(config.plugins);
    let start_args = args.clone();
    // Native shortcut registration is also needed when a shortcut is set later from Settings.
    // Pure Wayland has no X11 display, so leave the plugin unloaded there.
    let want_shortcut = integration::shortcut_supported();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            let args = cli::parse(&argv);
            app::handle_args(app, &args, Some(std::path::Path::new(&cwd)));
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(app::AppState::new(config, plugins, args.service))
        .register_asynchronous_uri_scheme_protocol("alook", |_ctx, request, responder| {
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(protocol::handle(&request));
            });
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if window.label() == app::SETTINGS {
                    app::close_settings(window.app_handle());
                } else {
                    app::hide(window.app_handle());
                }
            }
            if matches!(event, WindowEvent::Focused(false))
                && window.label() == app::MAIN
                && window.app_handle().state::<app::AppState>().is_visible()
                && !window.app_handle().state::<app::AppState>().just_shown()
                && !window
                    .app_handle()
                    .get_webview_window(app::SETTINGS)
                    .is_some_and(|settings| settings.is_visible().unwrap_or(false))
            {
                app::hide(window.app_handle());
            }
            // The window may get focus after `show` returned (it maps asynchronously): hand
            // keyboard focus to the web view so Space/Esc/arrows work without a click.
            #[cfg(target_os = "linux")]
            if let WindowEvent::Focused(true) = event {
                if window.label() != app::MAIN {
                    return;
                }
                if let Some(w) = window.app_handle().get_webview_window(app::MAIN) {
                    integration::linux::unpin_size(&w);
                    app::focus_webview(&w);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::open_settings,
            commands::open_welcome,
            commands::list_plugins,
            commands::about_info,
            commands::diagnostics,
            link_consumer::link_open,
            commands::frontend_ready,
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
            // Started from the menu, at login or by Tools, stderr goes nowhere:
            // keep it next to the settings instead.
            if let Err(error) =
                arcade_link::logs::capture_stderr(&commands::logs_dir(), "arcade-look")
            {
                eprintln!("arcade-look: log unavailable: {error}");
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
            link::start(&handle, &handle.state::<app::AppState>().config());
            link_consumer::start(&handle);
            #[cfg(all(feature = "e2e", unix))]
            e2e::start(&handle);
            app::handle_args(&handle, &start_args, user_dir.as_deref());
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
        if let RunEvent::WindowEvent {
            label,
            event: WindowEvent::Destroyed,
            ..
        } = &event
        {
            if label == app::MAIN {
                app::window_destroyed(app);
            } else if label == app::SETTINGS {
                app::settings_destroyed(app);
            }
        }
        if let RunEvent::Exit = event {
            link::stop();
        }
        if let RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            // Releasing the last webview must not stop Link, tray or shortcuts.
            // Explicit Quit and the optional non-resident idle exit carry a code.
            api.prevent_exit();
        }
    });
}
