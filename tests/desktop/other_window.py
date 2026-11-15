#!/usr/bin/python3.12
"""Another application's window for the desktop harness (the popup rule): GTK 3, app id "arcade-other-window".
Needs python3-gi and gir1.2-gtk-3.0; runs until the harness ends it."""
import gi

gi.require_version("Gtk", "3.0")
from gi.repository import GLib, Gtk  # noqa: E402

GLib.set_prgname("arcade-other-window")
window = Gtk.Window(title="Another window")
window.set_default_size(700, 500)
window.add(Gtk.Label(label="Another application"))
window.connect("destroy", Gtk.main_quit)
window.show_all()
Gtk.main()
