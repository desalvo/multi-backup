"""Cross-platform, accessible canvas controls with consistent Aqua/Windows/Linux rendering.

The macOS native button theme can suppress Tk/ttk foreground colours. These widgets
paint their own labelled surfaces; no platform-specific button styling is required.
"""
from __future__ import annotations

import colorsys
import tkinter as tk
from tkinter import font as tkfont

SURFACE = '#F5F8FC'
NAVY = '#163451'
PALE = '#EDF5FF'


def _rgb(value: str) -> tuple[int, int, int]:
    return tuple(int(value[index:index + 2], 16) for index in (1, 3, 5))


def _blend(left: tuple[int, int, int], right: tuple[int, int, int],
           fraction: float) -> tuple[int, int, int]:
    t = max(0., min(1., fraction))
    return tuple(round(a * (1 - t) + b * t) for a, b in zip(left, right))


def progress_color(percent: float) -> str:
    """Red at 0%, yellow at midpoint, and green at 100%, inclusive."""
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
        self._width = max(96, int(width) if width else self._font.measure(self.caption) + 38)
        super().__init__(parent, width=self._width, height=self._height,
                         bg='#0D1A3D' if self.dark else SURFACE,
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
        bg = '#0D1A3D' if self.dark else SURFACE
        if self.state == 'disabled':
            top, bottom, border, foreground = (
                '#E1E8F0', '#CED7E3', '#C6D2DE', '#8292A6')
        elif self.accent:
            top, bottom, border, foreground = (
                '#3D96E8' if self.hover else '#2786D8',
                '#1162A6' if self.hover else '#134F91',
                '#1F5F9D', '#FFFFFF')
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


class GradientProgress(tk.Canvas):
    """Rounded glass progress pill, with a true red→yellow→green position gradient."""

    def __init__(self, parent, *, width=440):
        super().__init__(parent, height=43, width=width, bg=SURFACE,
                         highlightthickness=0, borderwidth=0)
        self.percent = 0.
        self._photo = None
        self.bind('<Configure>', lambda _event: self._draw())
        self._draw()

    def set_percent(self, value) -> None:
        try:
            self.percent = max(0., min(100., float(value)))
        except (ValueError, TypeError):
            self.percent = 0.
        self._draw()

    def _draw(self) -> None:
        from PIL import ImageTk
        from render_quality import progress_patch

        width = max(80, self.winfo_width())
        height = 43
        img = progress_patch(width, height, self.percent, SURFACE,
                             progress_color)
        self._photo = ImageTk.PhotoImage(img)
        self.delete('all')
        self.create_image(0, 0, anchor='nw', image=self._photo)
        self.create_text(width - 38, 20, text=f'{round(self.percent):d}%',
                         fill='#FFFFFF', font=('Helvetica Neue', 10, 'bold'))
