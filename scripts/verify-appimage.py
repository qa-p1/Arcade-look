#!/usr/bin/env python3
"""Smoke-test a real AppImage outside the checkout with a disposable profile."""
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile


def unpacked_copies(root):
    """Processes running Look from an unpacked copy under `root` (Link U1)."""
    found = []
    for proc in Path("/proc").iterdir():
        try:
            exe = os.readlink(proc / "exe")
        except OSError:
            continue
        if exe.startswith(str(root) + "/") and "/arcade/runtime/look-" in exe and exe.endswith("/arcade-look"):
            found.append(int(proc.name))
    return found


def webkit_helpers(pid):
    """WebKit's helper processes (web and network) started by `pid`, at any depth."""
    parents = {}
    for proc in Path("/proc").iterdir():
        if not proc.name.isdigit():
            continue
        try:
            stat = (proc / "stat").read_text()
        except (OSError, ValueError):
            continue
        parents[int(proc.name)] = int(stat[stat.rindex(")") + 2:].split()[1])
    family, found = {pid}, []
    while True:
        children = {child for child, parent in parents.items() if parent in family} - family
        if not children:
            break
        family |= children
    for helper in family - {pid}:
        try:
            name = Path(f"/proc/{helper}/comm").read_text().strip()
        except OSError:
            continue
        if name.startswith(("WebKitWebProces", "WebKitNetworkPr")):
            found.append(helper)
    return found


def preloads_trimmer(pid):
    """Whether WebKit's helpers under `pid` run with libarcade-trim.so loaded (OPEN1). Look adds it to LD_PRELOAD
    at run time, which /proc/<pid>/environ (the environment at exec) never shows, so the helpers' mappings are
    what counts."""
    helpers = webkit_helpers(pid)
    def loaded(helper):
        try:
            return "/usr/lib/libarcade-trim.so" in Path(f"/proc/{helper}/maps").read_text()
        except OSError:
            return False
    return bool(helpers) and all(loaded(helper) for helper in helpers)


def survives(executable, env, work, root, *args, seconds=15):
    """Starts `executable` from the user's directory `work` and fails unless Look
    is still running `seconds` later from its unpacked copy under `root`, with the
    WebKit trimmer loaded in WebKit's helpers; then stops it and everything it started. The launcher
    copies the image once to <data>/arcade/runtime/, starts Look from there and
    exits 0 (U1, D12), so a shell holds the session open after it."""
    work.mkdir(exist_ok=True)
    with tempfile.TemporaryFile() as log:
        child = subprocess.Popen(
            ["sh", "-c", '"$0" "$@" || exit $?; exec sleep 600', str(executable), *args],
            cwd=work, env=dict(env, OWD=str(work)),
            stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, start_new_session=True,
        )
        unpacked = []
        trimmer = False
        try:
            code = child.wait(timeout=seconds)
        except subprocess.TimeoutExpired:
            code = None
            unpacked = unpacked_copies(root)
            trimmer = bool(unpacked) and all(preloads_trimmer(pid) for pid in unpacked)
        finally:
            for pid in unpacked_copies(root):
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait()
        if code is not None or not unpacked or not trimmer:
            log.seek(0)
            output = log.read().decode(errors="replace")[-4000:]
            if code is not None:
                raise RuntimeError(f"{executable} {' '.join(args)} exited {code} within {seconds}s:\n{output}")
            if not unpacked:
                raise RuntimeError(f"{executable} {' '.join(args)} is not running from its unpacked copy (U1):\n{output}")
            raise RuntimeError(f"{executable} {' '.join(args)} does not preload libarcade-trim.so (OPEN1):\n{output}")


def main():
    image, app_id, filename, cli_name = sys.argv[1:]
    image = Path(image).resolve(strict=True)
    # Link treats executables anywhere under a .git tree as development builds.
    with tempfile.TemporaryDirectory(prefix=app_id + "-", dir=os.environ.get("RUNNER_TEMP")) as temporary:
        root = Path(temporary).resolve()
        for parent in (root, *root.parents):
            if (parent / ".git").exists():
                raise RuntimeError("AppImage verification must run outside the checkout")
        env = os.environ.copy()
        for key in ("APPIMAGE", "APPDIR", "ARCADE_DEV_BUILD", "WAYLAND_DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE", "SWAYSOCK"):
            env.pop(key, None)
        env["GDK_BACKEND"] = "x11"  # Use xvfb-run, never an inherited real desktop.
        for key, directory in (
            ("HOME", "home"), ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"),
            ("XDG_CACHE_HOME", "cache"), ("XDG_RUNTIME_DIR", "run"), ("ARCADE_HOME", "arcade"),
            ("ARCADE_LENS_HOME", "lens"), ("ARCADE_LOOK_HOME", "look"),
        ):
            path = root / directory
            path.mkdir()
            env[key] = str(path)
        (root / "run").chmod(0o700)
        env["APPIMAGE_EXTRACT_AND_RUN"] = "1"
        env["ARCADE_NO_INSTALL_PROMPT"] = "1"
        # ARCADE_HOME redirects Link's Applications, integration and CLI roots.
        install_home = Path(env["ARCADE_HOME"])
        # Mirror a real profile: application integration and the installer
        # must resolve the same XDG roots even with ARCADE_HOME isolation.
        env["XDG_DATA_HOME"] = str(install_home / ".local/share")
        env["XDG_CONFIG_HOME"] = str(install_home / ".config")
        env["PATH"] = str(install_home / ".local/bin") + os.pathsep + env.get("PATH", "")
        download = root / "Downloaded.AppImage"
        shutil.copy2(image, download)
        download.chmod(0o755)

        def run(executable, *args):
            result = subprocess.run(
                [str(executable), *args], env=env, text=True, encoding="utf-8",
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=180,
            )
            if result.returncode:
                raise RuntimeError(f"{executable} {' '.join(args)} exited {result.returncode}\n{result.stdout}\n{result.stderr}")
            return result.stdout

        # OPEN1 (D12): Look preloads the trimmer into WebKit's helper processes (memory.rs).
        extracted = root / "extracted"
        extracted.mkdir()
        subprocess.run([str(download), "--appimage-extract"], cwd=extracted, env=env, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=180)
        assert (extracted / "squashfs-root/usr/lib/libarcade-trim.so").is_file(), "AppImage lacks libarcade-trim.so"
        # Third-party notices and the licence ship inside the image (Tauri resources), so Settings can open them offline.
        for name in ("THIRD_PARTY_NOTICES.md", "LICENSE"):
            found = [p for p in (extracted / "squashfs-root").rglob(name) if p.is_file() and p.stat().st_size > 0]
            assert found, f"AppImage lacks {name}"
        shutil.rmtree(extracted)

        version = run(download, "--version").strip().split()[-1]
        run(download, "--install", "--silent", "--no-launch")
        receipt_path = root / "arcade/installs" / (app_id + ".json")
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        assert receipt["schema"] == 1 and receipt["id"] == app_id, receipt
        assert receipt["method"] == "appimage" and receipt["managedBy"] == "self", receipt
        assert receipt["version"] == version and receipt["channel"] == "stable", receipt
        installed = install_home / "Applications/Arcade" / filename
        assert Path(receipt["path"]) == installed and os.access(installed, os.X_OK), receipt
        assert installed.read_bytes() == image.read_bytes(), "Installed package differs from the download"
        integration = receipt["integration"]
        desktop = Path(integration["desktopEntry"])
        cli = Path(integration["cli"])
        autostart = Path(integration["autostart"])
        icons = [Path(path) for path in integration["icons"]]
        entries = [installed, desktop, cli, autostart, *icons]
        assert icons, "No installed icons"
        for path in entries:
            assert path.is_relative_to(root) and path.exists(), path
        assert cli == install_home / ".local/bin" / cli_name and cli.is_symlink(), cli
        assert cli.resolve() == installed, cli
        assert f"X-Arcade-Id={app_id}" in desktop.read_text(encoding="utf-8"), desktop
        assert f"X-Arcade-Id={app_id}" in autostart.read_text(encoding="utf-8"), autostart
        assert run(cli_name, "--version").strip().split()[-1] == version
        status = json.loads(run(cli_name, "--integration-status"))
        assert status["receipt"]["path"] == str(installed), status
        assert status["entries"] and all(entry["exists"] for entry in status["entries"]), status

        # The app itself must survive its first webview: a preview of a file
        # named relative to the user's directory. AppRun changes into
        # $APPDIR/usr and the mounting runtime sets $OWD (extract-and-run
        # doesn't, so it is set here); WebKit finds its helpers relative to
        # $APPDIR/usr and aborts the whole app when they aren't there.
        # With U1 (D12) the launcher hands over to Look's unpacked copy and exits 0, so the copy is what is checked.
        work = root / "work"
        work.mkdir()
        (work / "note.md").write_text("# Arcade Look\n", encoding="utf-8")
        survives(installed, env, work, root, "note.md")

        # Reproduce an in-place upgrade from pre-overhaul ids. Tools creates
        # the new receipt-owned entries, but an older customized launcher and
        # login entry can remain. Fresh-profile checks alone missed 0.4.0's
        # fatal conflict: both Settings and real previews must still start.
        legacy_entries = {}
        for folder, args in ((desktop.parent, "%U"), (autostart.parent, "--service")):
            folder.mkdir(parents=True, exist_ok=True)
            path = folder / "arcade-look.desktop"
            contents = f'[Desktop Entry]\nType=Application\nName=Arcade Look\nComment=Legacy custom entry\nExec="{installed}" {args}\n'
            path.write_text(contents, encoding="utf-8")
            legacy_entries[path] = contents
        owned_entries = {path: path.read_bytes() for path in (desktop, autostart)}
        survives(installed, env, work, root, "--settings")
        for path, contents in legacy_entries.items():
            assert path.read_text(encoding="utf-8") == contents, f"Upgrade changed conflicting entry: {path}"
        # Exercise a login-entry conflict independently of the launcher conflict.
        legacy_launcher = desktop.parent / "arcade-look.desktop"
        legacy_launcher.unlink()
        del legacy_entries[legacy_launcher]
        survives(installed, env, work, root, "note.md")
        for path, contents in legacy_entries.items():
            assert path.read_text(encoding="utf-8") == contents, f"Upgrade changed conflicting entry: {path}"
        for path, contents in owned_entries.items():
            assert path.read_bytes() == contents, f"Upgrade replaced installer-owned entry: {path}"
        # The uninstall check below remains about installer-owned files.
        for path in legacy_entries:
            path.unlink()

        # A missing launcher must be repaired from the installed package.
        desktop.unlink()
        run(installed, "--repair")
        assert desktop.is_file(), "Repair did not restore the launcher"
        status = json.loads(run(installed, "--integration-status"))
        assert all(entry["exists"] for entry in status["entries"]), status
        app_home = Path(env["ARCADE_" + app_id.split(".")[-1].upper() + "_HOME"])
        sentinel = app_home / "keep-user-data.txt"
        sentinel.write_text("preserve", encoding="utf-8")
        run(installed, "--uninstall", "--silent")
        for path in [receipt_path, *entries]:
            assert not path.exists() and not path.is_symlink(), f"Uninstall retained {path}"
        assert sentinel.read_text(encoding="utf-8") == "preserve", "Uninstall removed user data"
        for name in (".profile", ".bash_profile", ".bashrc", ".zprofile", ".zshrc"):
            assert not (root / "home" / name).exists(), f"Installer created {name}"
        print(f"{app_id}: AppImage install, receipt, launcher, icons, CLI, autostart, repair and uninstall passed")


if __name__ == "__main__":
    main()
