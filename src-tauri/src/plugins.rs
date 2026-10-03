//! Plugins live in `<config>/arcade-look/plugins/<id>/plugin.json`.
//!
//! * **command** plugins run an external tool and preview its output
//!   (`html`, `text`, `markdown`, `image`, or `file` = re-preview whatever it produced).
//! * **script** plugins are ES modules rendered by the web UI.

use crate::util::{self, OrStr, Res};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_OUTPUT: u64 = 64 << 20;

fn default_type() -> String {
    "command".into()
}
fn default_output() -> String {
    "text".into()
}
fn default_mode() -> String {
    "override".into()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Plugin {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(rename = "type", default = "default_type")]
    pub kind: String,
    /// Lower-case extensions without dots ("doc", "tar.gz").
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Built-in kinds to take over ("video", "binary", …).
    #[serde(default)]
    pub kinds: Vec<String>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default = "default_output")]
    pub output: String,
    /// highlight.js language for `text` output.
    #[serde(default)]
    pub language: Option<String>,
    /// Script entry (relative to the plugin folder).
    #[serde(default)]
    pub entry: Option<String>,
    /// "override": used instead of the built-in viewer; "fallback": only when it fails.
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(skip_deserializing)]
    pub dir: String,
}

pub fn load_all(enabled: bool) -> Vec<Plugin> {
    if !enabled {
        return Vec::new();
    }
    let Ok(rd) = std::fs::read_dir(crate::config::plugins_dir()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let manifest = e.path().join("plugin.json");
        let Ok(s) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        match serde_json::from_str::<Plugin>(&s) {
            Ok(mut p) => {
                if p.id.is_empty() {
                    p.id = e.file_name().to_string_lossy().to_string();
                }
                p.dir = e.path().to_string_lossy().to_string();
                p.extensions = p
                    .extensions
                    .iter()
                    .map(|x| x.trim_start_matches('.').to_ascii_lowercase())
                    .collect();
                let valid = match p.kind.as_str() {
                    "command" => !p.command.is_empty(),
                    "script" => p.entry.is_some(),
                    _ => false,
                };
                if valid {
                    out.push(p);
                } else {
                    eprintln!(
                        "arcade-look: plugin {} is missing `command`/`entry`",
                        manifest.display()
                    );
                }
            }
            Err(err) => eprintln!(
                "arcade-look: invalid plugin manifest {}: {err}",
                manifest.display()
            ),
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Find the plugin for a file: (override plugin, fallback plugin).
pub fn matching<'a>(
    plugins: &'a [Plugin],
    ext: &str,
    kind: &str,
) -> (Option<&'a Plugin>, Option<&'a Plugin>) {
    let hit =
        |p: &&Plugin| p.extensions.iter().any(|e| e == ext) || p.kinds.iter().any(|k| k == kind);
    let over = plugins.iter().filter(hit).find(|p| p.mode != "fallback");
    let fall = plugins.iter().filter(hit).find(|p| p.mode == "fallback");
    (over, fall)
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PluginOutput {
    pub output: String,
    pub text: Option<String>,
    pub path: Option<String>,
    pub language: Option<String>,
}

pub fn run(p: &Plugin, file: &Path) -> Res<PluginOutput> {
    if p.kind != "command" {
        return Err("not a command plugin".into());
    }
    let key = util::hash_key(&[&p.id, &util::file_key(file), &p.command.join("\u{1}")]);
    let outdir = util::temp_root().join("p").join(key);
    let done = outdir.join(".done");
    let stdout_file = outdir.join(".stdout");

    if !done.exists() {
        let _ = std::fs::remove_dir_all(&outdir);
        std::fs::create_dir_all(&outdir).or_str()?;
        let stdout = execute(p, file, &outdir)?;
        std::fs::write(&stdout_file, &stdout).or_str()?;
        std::fs::write(&done, b"").or_str()?;
    }

    let mut out = PluginOutput {
        output: p.output.clone(),
        text: None,
        path: None,
        language: p.language.clone(),
    };
    match p.output.as_str() {
        "html" | "text" | "markdown" => {
            let bytes = std::fs::read(&stdout_file).or_str()?;
            out.text = Some(crate::text::decode(&bytes).0);
        }
        "image" => out.path = Some(stdout_file.to_string_lossy().to_string()),
        "file" => {
            let newest = std::fs::read_dir(&outdir)
                .or_str()?
                .flatten()
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
            let path: PathBuf = newest
                .map(|e| e.path())
                .ok_or("The plugin command produced no file")?;
            out.path = Some(path.to_string_lossy().to_string());
        }
        other => return Err(format!("unknown plugin output type: {other}")),
    }
    Ok(out)
}

fn execute(p: &Plugin, file: &Path, outdir: &Path) -> Res<Vec<u8>> {
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let stem = file
        .file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let subst = |s: &str| {
        s.replace("{path}", &file.to_string_lossy())
            .replace("{outdir}", &outdir.to_string_lossy())
            .replace("{name}", &name)
            .replace("{stem}", &stem)
            .replace("{ext}", &crate::detect::extension_of(&name))
            .replace("{plugin}", &p.dir)
    };
    let args: Vec<String> = p.command.iter().map(|a| subst(a)).collect();
    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..])
        .current_dir(&p.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not run `{}`: {e}", args[0]))?;
    let mut so = child.stdout.take().ok_or("no stdout")?;
    let mut se = child.stderr.take().ok_or("no stderr")?;
    let out_t = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = (&mut so).take(MAX_OUTPUT).read_to_end(&mut v);
        // Drain the rest so the child never blocks on a full pipe.
        let _ = std::io::copy(&mut so, &mut std::io::sink());
        v
    });
    let err_t = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = (&mut se).take(64 * 1024).read_to_end(&mut v);
        let _ = std::io::copy(&mut se, &mut std::io::sink());
        v
    });
    let timeout = Duration::from_millis(p.timeout_ms.unwrap_or(15_000));
    let started = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().or_str()? {
            break s;
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "`{}` timed out after {} s",
                args[0],
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(15));
    };
    let stdout = out_t.join().unwrap_or_default();
    let stderr = err_t.join().unwrap_or_default();
    if !status.success() {
        let msg = String::from_utf8_lossy(&stderr);
        return Err(format!("`{}` failed ({status}): {}", args[0], msg.trim()));
    }
    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn runs_command_plugin() {
        let d = std::env::temp_dir().join(format!("alook-plug-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("in.txt");
        std::fs::write(&f, "abc").unwrap();
        let p = Plugin {
            id: "t".into(),
            name: "t".into(),
            kind: "command".into(),
            extensions: vec!["txt".into()],
            kinds: vec![],
            command: vec![
                "sh".into(),
                "-c".into(),
                "printf '<b>%s</b>' \"$(cat \"$0\")\"".into(),
                "{path}".into(),
            ],
            output: "html".into(),
            language: None,
            entry: None,
            mode: "override".into(),
            timeout_ms: None,
            dir: d.to_string_lossy().to_string(),
        };
        let out = run(&p, &f).unwrap();
        assert_eq!(out.text.as_deref(), Some("<b>abc</b>"));
        let (o, fb) = matching(std::slice::from_ref(&p), "txt", "text");
        assert!(o.is_some() && fb.is_none());

        let slow = Plugin {
            command: vec!["sleep".into(), "5".into()],
            timeout_ms: Some(200),
            id: "slow".into(),
            ..p
        };
        let e = run(&slow, &f).unwrap_err();
        assert!(e.contains("timed out"), "{e}");
    }
}
