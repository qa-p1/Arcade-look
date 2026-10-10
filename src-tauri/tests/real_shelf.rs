//! Look's "Add to Shelf" against the real Arcade Shelf binary (not a mock),
//! in a private ARCADE_HOME. Ignored by default: set ARCADE_SHELF_BIN.
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use arcade_link::{ids, Locations};
use arcade_look_lib::config::Config;
use arcade_look_lib::link_consumer::{file_content, Consumer};

struct Shelf(Child);
impl Drop for Shelf {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "real app: set ARCADE_SHELF_BIN to an Arcade Shelf binary"]
fn real_shelf_keeps_the_previewed_file() {
    let bin = std::env::var_os("ARCADE_SHELF_BIN").expect("set ARCADE_SHELF_BIN");
    let root = std::env::temp_dir().join(format!("look-real-shelf-{}", std::process::id()));
    let run = root.join("xdg-run");
    for dir in [&run, &root.join("shelf")] {
        std::fs::create_dir_all(dir).unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::env::set_var("ARCADE_HOME", &root);
    let _shelf = Shelf(
        Command::new(&bin)
            .arg("--background")
            .env("ARCADE_HOME", &root)
            .env("ARCADE_SHELF_HOME", root.join("shelf"))
            .env("XDG_RUNTIME_DIR", &run)
            .env("QT_QPA_PLATFORM", "offscreen")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let locations = Locations::under(&root);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !locations.endpoint(ids::SHELF).exists() {
        assert!(
            Instant::now() < deadline,
            "Shelf never published its endpoint"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let path = root.join("preview.png");
    image::RgbImage::new(8, 6).save(&path).unwrap();
    let content = file_content(path.to_str().unwrap()).unwrap();
    let consumer = Consumer::new(locations);
    let config = Config::default();
    let offers: Vec<_> = consumer
        .offers(&config, &content, false)
        .into_iter()
        .filter(|offer| offer.app == ids::SHELF)
        .map(|offer| (offer.action, offer.title))
        .collect();
    assert_eq!(
        offers,
        [("shelf.add".to_string(), "Add to Shelf".to_string())]
    );

    let result = consumer
        .invoke(
            &config,
            (ids::SHELF, "shelf.add"),
            content,
            &mut |_| {},
            &AtomicBool::new(false),
            Duration::from_secs(10),
        )
        .unwrap();
    let message = result.message.unwrap_or_default();
    assert!(message.starts_with("Added 1 item to"), "{message}");
    // Shelf lists what it added: the file itself, by reference.
    let data = result.data.unwrap_or_default();
    let added = data["added"].as_array().cloned().unwrap_or_default();
    let canonical = path.canonicalize().unwrap();
    assert!(
        added.iter().any(
            |item| item["path"].as_str().map(Path::new) == Some(path.as_path())
                || item["path"].as_str().map(Path::new) == Some(canonical.as_path())
        ),
        "{data}"
    );
    assert!(path.is_file(), "the original stays where it is");
    println!("real Shelf: \"{message}\"; added {}", data["added"]);
    let _ = std::fs::remove_dir_all(&root);
}
