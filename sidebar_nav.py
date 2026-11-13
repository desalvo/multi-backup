"""Accessible, high-contrast canvas navigation for macOS, Windows and Linux.

Tk/Aqua can ignore foreground colours on native-looking tk.Button controls.
Drawing both text and icons on a Canvas gives predictable contrast on macOS
in light and dark appearances, independent of the operating-system button theme.
Mouse, Tab focus, Enter and Space all activate the same navigation command.
"""
from __future__ import annotations

import tkinter as tk
from typing import Callable

SIDEBAR_BG = '#0F1D42'
SIDEBAR_PANEL = '#1C2C56'
SIDEBAR_SEPARATOR = '#354B82'
SIDEBAR_HEADING = '#B8C8F1'
SIDEBAR_MUTED = '#A5B8E2'
NAV_TEXT = '#FCFCFF'
NAV_ICON = '#EBEBFF'
NAV_NORMAL = SIDEBAR_BG
NAV_HOVER = '#273B74'
NAV_ACTIVE = '#574FB8'
NAV_ACTIVE_BORDER = '#978BFF'
NAV_ICON_PLATE = '#223663'
NAV_ICON_PLATE_ACTIVE = '#6962CF'
NAV_ACCENT = '#AA9EFF'


def contrast_ratio(foreground: str, background: str) -> float:
    """WCAG relative contrast ratio for opaque sRGB colours; no GUI required."""
    def luminance(value: str) -> float:
        components = [int(value[i:i+2], 16) / 255 for i in (1, 3, 5)]
        linear = [v / 12.92 if v <= .04045 else ((v + .055) / 1.055) ** 2.4
                  for v in components]
        return .2126 * linear[0] + .7152 * linear[1] + .0722 * linear[2]
    a, b = sorted((luminance(foreground), luminance(background)), reverse=True)
    return (a + .05) / (b + .05)


NAV_ITEMS = (
    ('Dashboard', 'dashboard'),
    ('Backup', 'backup'),
    ('Archives', 'archives'),
    ('Restore', 'restore'),
    ('Plugins', 'plugins'),
    ('Encryption', 'encryption'),
    ('Updates', 'updates'),
    ('Settings', 'settings'),
)


class SidebarNavItem(tk.Canvas):
    """A keyboard-operable navigation surface with explicit Canvas text colour."""

    def __init__(self, parent: tk.Widget, title: str, icon: str,
                 command: Callable[[], None]) -> None:
        super().__init__(
            parent, width=222, height=52, bg=SIDEBAR_BG, borderwidth=0,
            highlightthickness=0, relief='flat', cursor='hand2', takefocus=1,
        )
        self.title = title
        self.icon = icon
        self.command = command
        self.selected = False
        self.hovered = False
        self.focused = False
        self.bind('<Enter>', self._enter)
        self.bind('<Leave>', self._leave)
        self.bind('<FocusIn>', self._focus_in)
        self.bind('<FocusOut>', self._focus_out)
        self.bind('<Button-1>', self._click)
        self.bind('<Return>', self._activate)
        self.bind('<space>', self._activate)
        self.bind('<Configure>', lambda _event: self._redraw())
        self._redraw()

    def set_active(self, active: bool) -> None:
        if self.selected != active:
            self.selected = active
            self._redraw()

    def _enter(self, _event: tk.Event) -> None:
        self.hovered = True
        self._redraw()

    def _leave(self, _event: tk.Event) -> None:
        self.hovered = False
        self._redraw()

    def _focus_in(self, _event: tk.Event) -> None:
        self.focused = True
        self._redraw()

    def _focus_out(self, _event: tk.Event) -> None:
        self.focused = False
        self._redraw()

    def _click(self, _event: tk.Event) -> None:
        self.focus_set()
        self.command()

    def _activate(self, _event: tk.Event) -> str:
        self.command()
        return 'break'

    def _round_rect(self, x1: float, y1: float, x2: float, y2: float,
                    radius: int, *, fill: str, outline: str = '',
                    width: int = 1) -> None:
        r = radius
        self.create_polygon(
            x1+r, y1, x2-r, y1, x2, y1, x2, y1+r,
            x2, y2-r, x2, y2, x2-r, y2, x1+r, y2,
            x1, y2, x1, y2-r, x1, y1+r, x1, y1,
            smooth=True, splinesteps=24,
            fill=fill, outline=outline, width=width,
        )

    def _draw_icon(self, x: int, y: int, colour: str) -> None:
        def line(*points: int, **kwargs: object) -> None:
            self.create_line(*points, fill=colour, width=2, capstyle=tk.ROUND,
                             joinstyle=tk.ROUND, **kwargs)

        kind = self.icon
        if kind == 'dashboard':
            for dx, dy in ((-7, -7), (2, -7), (-7, 2), (2, 2)):
                self.create_rectangle(x+dx, y+dy, x+dx+5, y+dy+5,
                                      outline=colour, width=1.6)
        elif kind == 'backup':
            self.create_arc(x-9, y-9, x+9, y+9, start=40, extent=290,
                            style=tk.ARC, outline=colour, width=2)
            line(x+7, y-8, x+10, y-2, x+3, y-1)
        elif kind == 'archives':
            for offset in (-6, 0, 6):
                self.create_rectangle(x-9, y+offset-2, x+9, y+offset+2,
                                      outline=colour, width=1.5)
                self.create_oval(x+4, y+offset-0.8, x+5.6, y+offset+0.8,
                                 fill=colour, outline=colour)
        elif kind == 'restore':
            self.create_arc(x-9, y-9, x+9, y+9, start=190, extent=285,
                            style=tk.ARC, outline=colour, width=2)
            line(x-7, y+7, x-10, y+1, x-3, y)
            line(x, y-8, x, y+4)
            line(x-5, y, x, y+5, x+5, y)
        elif kind == 'plugins':
            line(x-7, y-5, x+5, y+1)
            line(x-7, y+6, x+5, y+1)
            for nx, ny in ((x-8, y-6), (x-8, y+7), (x+7, y+1)):
                self.create_oval(nx-3, ny-3, nx+3, ny+3, fill=NAV_ICON_PLATE_ACTIVE
                                 if self.selected else NAV_ICON_PLATE,
                                 outline=colour, width=1.5)
        elif kind == 'encryption':
            self.create_arc(x-6, y-10, x+6, y+4, start=0, extent=180,
                            style=tk.ARC, outline=colour, width=2)
            self.create_rectangle(x-9, y-2, x+9, y+10, outline=colour, width=2)
            self.create_oval(x-1, y+2, x+1, y+4, fill=colour, outline=colour)
            line(x, y+4, x, y+7)
        elif kind == 'updates':
            line(x, y-10, x, y+4)
            line(x-5, y-1, x, y+4, x+5, y-1)
            line(x-8, y+8, x+8, y+8)
        elif kind == 'settings':
            for yy, knob in ((y-7, x-2), (y, x+5), (y+7, x-4)):
                line(x-9, yy, x+9, yy)
                self.create_oval(knob-2.8, yy-2.8, knob+2.8, yy+2.8,
                                 fill=NAV_ICON_PLATE_ACTIVE if self.selected
                                 else NAV_ICON_PLATE, outline=colour, width=1.6)

    def _redraw(self) -> None:
        self.delete('all')
        w = max(self.winfo_width(), 222)
        background = (NAV_ACTIVE if self.selected else
                      NAV_HOVER if self.hovered else NAV_NORMAL)
        border = NAV_ACTIVE_BORDER if self.selected else background
        self._round_rect(3, 3, w-3, 49, 12, fill=background,
                         outline=border, width=1)
        if self.selected:
            self._round_rect(3, 13, 7, 39, 2, fill=NAV_ACCENT)
        elif self.hovered:
            self._round_rect(3, 18, 6, 34, 2, fill='#47799E')
        plate = NAV_ICON_PLATE_ACTIVE if self.selected else NAV_ICON_PLATE
        self._round_rect(18, 10, 46, 42, 9, fill=plate)
        self._draw_icon(32, 26, NAV_ICON)
        self.create_text(62, 26, text=self.title, anchor='w',
                         font=('Helvetica Neue', 12, 'bold' if self.selected
                               else 'normal'), fill=NAV_TEXT,
                         width=max(100, w-76))
        if self.focused:
            self._round_rect(2, 2, w-2, 50, 13,
                             fill='', outline=NAV_ACCENT, width=2)
