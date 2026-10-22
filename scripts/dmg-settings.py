# dmgbuild settings for the drag-to-Applications window. Values come from make-dmg.sh (-D).
import os.path

app = defines["app"]  # noqa: F821 (provided by dmgbuild)
format = "UDZO"
filesystem = "HFS+"
files = [app]
symlinks = {"Applications": "/Applications"}
badge_icon = None
icon_locations = {os.path.basename(app): (160, 205), "Applications": (480, 205)}
background = defines["background"]  # noqa: F821
window_rect = ((200, 160), (640, 400))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
icon_size = 112
text_size = 13
