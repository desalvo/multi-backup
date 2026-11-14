"""Cross-platform, accessible canvas controls with consistent Aqua/Windows/Linux rendering.

The macOS native button theme can suppress Tk/ttk foreground colours. These widgets
paint their own labelled surfaces; no platform-specific button styling is required.
"""
from __future__ import annotations

import colorsys
import time
import tkinter as tk
from tkinter import font as tkfont, ttk

from rust_bridge import native_progress_color

SURFACE = '#F5F8FC'
NAVY = '#163451'
PALE = '#EDF5FF'


def _parent_background(parent):
    """Match rounded-button corner pixels to the real enclosing native surface."""
    try:
        color = parent.cget('background')
        if isinstance(color, str) and color.startswith('#'):
            return color
    except (tk.TclError, KeyError):
        pass
    try:
        style = parent.cget('style') or parent.winfo_class()
        value = ttk.Style(parent).lookup(style, 'background')
        if value and value.startswith('#'):
            return value
    except (tk.TclError, KeyError, AttributeError):
        pass
    return SURFACE


def _rgb(value: str) -> tuple[int, int, int]:
    return tuple(int(value[index:index + 2], 16) for index in (1, 3, 5))


def _blend(left: tuple[int, int, int], right: tuple[int, int, int],
           fraction: float) -> tuple[int, int, int]:
    t = max(0., min(1., fraction))
    return tuple(round(a * (1 - t) + b * t) for a, b in zip(left, right))


def progress_color(percent: float) -> str:
    """Red at 0%, yellow at midpoint, and green at 100%, inclusive."""
    # Prefer the tested Rust-native colour kernel when it is bundled; the
    # mathematical Python fallback keeps old/source builds fully compatible.
    native = native_progress_color(percent)
    if native is not None:
        return native
    value = max(0., min(100., float(percent)))
    red, green, blue = colorsys.hsv_to_rgb(value / 300., 0.86, 0.91)
    return '#%02X%02X%02X' % (round(red * 255), round(green * 255),
                                round(blue * 255))


class PremiumButton(tk.Canvas):
    """Rounded glossy button with explicit text, hover, disabled and key states."""

    def __init__(self, parent, *, text='', command=None, style='', state='normal',
                 width=None, height=39):
        self.caption = str(text)
        self.command = command
        self.accent = str(style).startswith('Accent')
        self.dark = str(style).startswith('Dark')
        self.state = state
        self.hover = False
        self.pressed = False
        self.focused = False
        self._font = tkfont.Font(root=parent, family='Helvetica Neue', size=11, weight='bold')
        self._height = int(height)
        self._surface = _parent_background(parent)
        self._width = max(96, int(width) if width else self._font.measure(self.caption) + 38)
        super().__init__(parent, width=self._width, height=self._height,
                         bg=self._surface,
                         borderwidth=0, highlightthickness=0,
                         takefocus=1, cursor='hand2')
        self._photo = None
        for name, handler in (
            ('<Enter>', self._enter), ('<Leave>', self._leave),
            ('<ButtonPress-1>', self._mouse_down), ('<ButtonRelease-1>', self._mouse_up),
            ('<FocusIn>', self._focus_in), ('<FocusOut>', self._focus_out),
            ('<Return>', self._keyboard), ('<space>', self._keyboard),
        ):
            self.bind(name, handler)
        self.bind('<Configure>', lambda _event: self._draw())
        self._draw()

    def __setitem__(self, key, value):
        if key == 'state':
            self.set_state(str(value))
        else:
            super().__setitem__(key, value)

    def __getitem__(self, key):
        if key == 'state':
            return self.state
        return super().__getitem__(key)

    def set_text(self, caption: str) -> None:
        self.caption = str(caption)
        self._width = max(96, self._font.measure(self.caption) + 38)
        self.configure(width=self._width)
        self._draw()

    def set_state(self, value: str) -> None:
        if value not in ('normal', 'disabled'):
            raise ValueError('Button state must be normal or disabled')
        self.state = value
        self.configure(cursor='hand2' if value == 'normal' else 'arrow')
        self._draw()

    def _enter(self, _event):
        self.hover = True
        self._draw()

    def _leave(self, _event):
        self.hover = False
        self.pressed = False
        self._draw()

    def _focus_in(self, _event):
        self.focused = True
        self._draw()

    def _focus_out(self, _event):
        self.focused = False
        self._draw()

    def _mouse_down(self, _event):
        if self.state == 'normal':
            self.focus_set()
            self.pressed = True
            self._draw()

    def _mouse_up(self, event):
        activated = self.pressed and self.state == 'normal' and (
            0 <= event.x <= self.winfo_width() and 0 <= event.y <= self.winfo_height())
        self.pressed = False
        self._draw()
        if activated and self.command:
            self.command()

    def _keyboard(self, _event):
        if self.state == 'normal' and self.command:
            self.command()
        return 'break'

    def _draw(self) -> None:
        """Render at four times widget resolution, then Lanczos-downsample."""
        from PIL import Image, ImageTk
        from render_quality import glossy_patch

        width = max(5, self.winfo_width() if self.winfo_width() > 1 else self._width)
        height = max(5, self.winfo_height() if self.winfo_height() > 1 else self._height)
        bg = self._surface
        if self.state == 'disabled':
            top, bottom, border, foreground = (
                '#E1E8F0', '#CED7E3', '#C6D2DE', '#8292A6')
        elif self.accent:
            top, bottom, border, foreground = (
                '#B18BFF' if self.hover else '#A579F6',
                '#5568E8' if self.hover else '#4658D1',
                '#C8BBFF', '#FFFFFF')
        elif self.dark:
            top, bottom, border, foreground = (
                '#6471C9' if self.hover else '#4559A5',
                '#30478D' if self.hover else '#263B77',
                '#8A95DB', '#FFFFFF')
        else:
            top, bottom, border, foreground = (
                '#FFFFFF' if self.hover else '#F7FAFF',
                '#DFEDFB' if self.hover else '#E8F1FC',
                '#B2C8E1', NAVY)
        if self.pressed:
            top, bottom = bottom, top
        img = Image.new('RGBA', (width, height), (*_rgb(bg), 255))
        img.alpha_composite(glossy_patch(
            width, height, top=top, bottom=bottom, border=border,
            radius=min(13, height / 3),
            focused=self.focused and self.state == 'normal'))
        self._photo = ImageTk.PhotoImage(img)
        self.delete('all')
        self.create_image(0, 0, image=self._photo, anchor='nw')
        # Native Tk text remains vector/sharp; only the rounded surfaces are
        # rasterized and downsampled.
        self.create_text(width // 2, (height - 3) // 2, text=self.caption,
                         font=self._font, fill=foreground, anchor='center')


class HamburgerButton(PremiumButton):
    """Compact, accessible hamburger with 4x-antialiased rounded vector bars."""

    def __init__(self, parent, *, command=None, expanded=True):
        # Reuse native keyboard, hover, click and focus handling, but draw only
        # the familiar icon (not an English 'Hide navigation' label).
        super().__init__(parent, text='', command=command,
                         style='Dark.TButton', width=56, height=42)
        self._width = 56
        self.configure(width=56)
        self.expanded = bool(expanded)
        self.caption = 'Nascondi navigazione' if expanded else 'Mostra navigazione'
        self._draw()

    def set_expanded(self, value):
        self.expanded = bool(value)
        self.caption = ('Nascondi navigazione' if self.expanded
                        else 'Mostra navigazione')
        self._draw()

    def _draw(self):
        from PIL import Image, ImageDraw, ImageTk
        from render_quality import SCALE, glossy_patch
        w = max(12, self.winfo_width() if self.winfo_width() > 1 else
                getattr(self, '_width', 56))
        h = max(12, self.winfo_height() if self.winfo_height() > 1 else 42)
        base = Image.new('RGBA', (w, h), (*_rgb(self._surface), 255))
        base.alpha_composite(glossy_patch(
            w, h, top='#6674BD' if self.hover else '#4C5DA8',
            bottom='#354783' if self.hover else '#263976',
            border='#A3B4EA', radius=14, focused=self.focused))
        big = Image.new('RGBA', (w * SCALE, h * SCALE))
        pen = ImageDraw.Draw(big)
        ink = '#F7FAFF' if self.expanded else '#BEEBFF'
        bar_left, bar_right = 17 * SCALE, (w - 17) * SCALE
        for y in (12, 20, 28):
            y0 = y * SCALE - 1.5 * SCALE
            pen.rounded_rectangle((bar_left, round(y0), bar_right,
                                   round(y0 + 3 * SCALE)),
                                  radius=round(1.5 * SCALE), fill=ink)
        base.alpha_composite(big.resize((w, h), Image.Resampling.LANCZOS))
        self._photo = ImageTk.PhotoImage(base)
        self.delete('all')
        self.create_image(0, 0, image=self._photo, anchor='nw')


class ActivityPulse:
    """Pure state machine: animate on fresh real events, freeze on a stall."""

    def __init__(self, idle_after=8.0):
        self.idle_after = float(idle_after)
        self.running = False
        self.phase = 0.0
        self.last_activity = 0.0

    def set_running(self, value, *, now=None):
        self.running = bool(value)
        if self.running:
            self.last_activity = time.monotonic() if now is None else float(now)
            self.phase = 0.0

    def note_activity(self, *, now=None):
        if self.running:
            self.last_activity = time.monotonic() if now is None else float(now)

    def advance(self, *, now=None):
        current = time.monotonic() if now is None else float(now)
        if not self.running or current - self.last_activity > self.idle_after:
            return False
        self.phase = (self.phase + 0.0225) % 1.0
        return True


class GradientProgress(tk.Canvas):
    """Smooth red→yellow→green fill plus an event-driven animated busy bead.

    Bar colours are mapped to absolute progress: red at 0%, green at 100%.
    A tiny cached 4x-antialiased activity image moves only while the backup
    process is active and the engine has reported an event recently.
    """

    def __init__(self, parent, *, width=440):
        from PIL import ImageTk
        from render_quality import activity_patch
        self._surface = _parent_background(parent)
        super().__init__(parent, height=43, width=width, bg=self._surface,
                         highlightthickness=0, borderwidth=0)
        self.percent = 0.
        self.pulse = ActivityPulse()
        self._photo = None
        self._activity_photo = ImageTk.PhotoImage(activity_patch())
        self._activity_id = None
        self._timer = None
        self.bind('<Configure>', lambda _event: self._draw())
        self.bind('<Destroy>', self._on_destroy)
        self._draw()

    def _on_destroy(self, event):
        if event.widget is self and self._timer is not None:
            try:
                self.after_cancel(self._timer)
            except tk.TclError:
                pass
            self._timer = None

    def set_running(self, running):
        self.pulse.set_running(running)
        self._draw()
        if running:
            self._schedule_tick()
        elif self._timer is not None:
            self.after_cancel(self._timer)
            self._timer = None

    def note_activity(self):
        self.pulse.note_activity()
        if self.pulse.running:
            self._schedule_tick()

    def _schedule_tick(self):
        if self._timer is None:
            self._timer = self.after(95, self._tick)

    def _indicator_x(self):
        w = max(80, self.winfo_width())
        extent = max(5, w - 32)
        # A triangle wave visibly bounces without altering reported percentage.
        bounce = 1.0 - abs(2.0 * self.pulse.phase - 1.0)
        return 16 + bounce * extent

    def _tick(self):
        self._timer = None
        if not self.pulse.advance():
            return
        if self._activity_id is not None:
            self.coords(self._activity_id, self._indicator_x(), 20)
        self._schedule_tick()

    def set_percent(self, value):
        try:
            next_percent = max(0., min(100., float(value)))
        except (ValueError, TypeError):
            next_percent = 0.
        if not (0 <= next_percent <= 100):  # guard NaN too
            next_percent = 0.
        was = round(self.percent)
        self.percent = next_percent
        # Skip the expensive full-width re-rasterization for same integer %.
        if round(next_percent) != was:
            self._draw()

    def _draw(self):
        from PIL import ImageTk
        from render_quality import progress_patch
        width = max(80, self.winfo_width())
        img = progress_patch(width, 43, self.percent, self._surface,
                             progress_color)
        self._photo = ImageTk.PhotoImage(img)
        self.delete('all')
        self.create_image(0, 0, image=self._photo, anchor='nw')
        self.create_text(width - 38, 20, text=f'{round(self.percent):d}%',
                         fill='#FFFFFF', font=('Helvetica Neue', 10, 'bold'))
        self._activity_id = None
        if self.pulse.running:
            self._activity_id = self.create_image(
                self._indicator_x(), 20, image=self._activity_photo)

