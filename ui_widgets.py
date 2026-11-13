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
        self.state = state
        self.hover = False
        self.pressed = False
        self.focused = False
        self._font = tkfont.Font(root=parent, family='Helvetica Neue', size=11, weight='bold')
        self._height = int(height)
        self._width = max(96, int(width) if width else self._font.measure(self.caption) + 38)
        super().__init__(parent, width=self._width, height=self._height,
                         bg=SURFACE, borderwidth=0, highlightthickness=0,
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
        from PIL import Image, ImageDraw, ImageTk
        width = max(5, self.winfo_width() if self.winfo_width() > 1 else self._width)
        height = max(5, self.winfo_height() if self.winfo_height() > 1 else self._height)
        image = Image.new('RGBA', (width, height), (*_rgb(SURFACE), 255))
        d = ImageDraw.Draw(image, 'RGBA')
        radius = min(13, height // 3)
        d.rounded_rectangle((3, 4, width - 3, height - 1), radius=radius,
                            fill=(20, 43, 76, 42))
        if self.state == 'disabled':
            top, bottom, border, foreground = ('#E1E8F0', '#CED7E3',
                                               '#C6D2DE', '#8292A6')
        elif self.accent:
            top, bottom, border, foreground = (
                ('#3D96E8' if self.hover else '#2786D8'),
                ('#1162A6' if self.hover else '#134F91'), '#1F5F9D', '#FFFFFF')
        else:
            top, bottom, border, foreground = (
                ('#FFFFFF' if self.hover else '#F7FAFF'),
                ('#DFEDFB' if self.hover else '#E8F1FC'), '#B2C8E1', NAVY)
        if self.pressed:
            top, bottom = bottom, top
        d.rounded_rectangle((2, 2, width - 3, height - 4), radius=radius,
                            fill=(*_rgb(bottom), 255), outline=(*_rgb(border), 255),
                            width=1)
        interior = Image.new('RGBA', (width, height), (0, 0, 0, 0))
        idraw = ImageDraw.Draw(interior)
        for y in range(3, height - 4):
            tint = _blend(_rgb(top), _rgb(bottom), (y - 3) / max(1, height - 7))
            idraw.line((3, y, width - 4, y), fill=(*tint, 255), width=1)
        mask = Image.new('L', (width, height), 0)
        ImageDraw.Draw(mask).rounded_rectangle((3, 3, width - 4, height - 5),
                                                radius=max(1, radius - 1), fill=255)
        image.paste(interior, (0, 0), mask)
        d = ImageDraw.Draw(image, 'RGBA')
        d.arc((5, 4, width - 6, height - 8), 190, 345, fill=(255, 255, 255, 50), width=1)
        if self.focused and self.state == 'normal':
            d.rounded_rectangle((1, 1, width - 2, height - 2),
                                radius=radius + 1, outline=(66, 174, 241, 255), width=2)
        self._photo = ImageTk.PhotoImage(image)
        self.delete('all')
        self.create_image(0, 0, image=self._photo, anchor='nw')
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
        from PIL import Image, ImageDraw, ImageTk
        width = max(80, self.winfo_width())
        height = 43
        img = Image.new('RGBA', (width, height), (*_rgb(SURFACE), 255))
        draw = ImageDraw.Draw(img, 'RGBA')
        x1, y1, x2, y2 = 3, 6, width - 4, 35
        draw.rounded_rectangle((x1, y1 + 3, x2, y2 + 3), radius=13,
                               fill=(19, 44, 74, 53))
        draw.rounded_rectangle((x1, y1, x2, y2), radius=13,
                               fill=(24, 48, 76, 255), outline=(109, 139, 167, 255),
                               width=2)
        full = max(1, x2 - x1 - 4)
        # The small red cap makes the 0%-colour visible even for an empty bar.
        fill_width = max(10, round(full * self.percent / 100))
        gradient = Image.new('RGBA', (width, height), (0, 0, 0, 0))
        gd = ImageDraw.Draw(gradient)
        for px in range(x1 + 2, min(x2 - 1, x1 + 2 + fill_width)):
            position = (px - (x1 + 2)) / full * 100
            colour = _rgb(progress_color(position))
            gd.line((px, y1 + 2, px, y2 - 2), fill=(*colour, 255))
            gd.line((px, y1 + 3, px, y1 + 10), fill=(
                *_blend(colour, (255, 255, 255), .35), 255))
            gd.line((px, y2 - 8, px, y2 - 2), fill=(
                *_blend(colour, (15, 35, 47), .25), 255))
        mask = Image.new('L', (width, height), 0)
        ImageDraw.Draw(mask).rounded_rectangle((x1 + 2, y1 + 2,
                                                min(x2 - 2, x1 + 2 + fill_width), y2 - 2),
                                                radius=10, fill=255)
        img.paste(gradient, (0, 0), mask)
        draw = ImageDraw.Draw(img, 'RGBA')
        draw.arc((x1 + 2, y1 + 1, x2 - 2, y2 - 4), 195, 345,
                 fill=(255, 255, 255, 130), width=1)
        # High-contrast percent pill at right, readable over any gradient colour.
        draw.rounded_rectangle((max(x1 + 8, x2 - 61), 10, x2 - 7, 31),
                               radius=10, fill=(15, 33, 55, 229))
        self._photo = ImageTk.PhotoImage(img)
        self.delete('all')
        self.create_image(0, 0, anchor='nw', image=self._photo)
        self.create_text(x2 - 34, 20, text=f'{round(self.percent):d}%',
                         fill='#FFFFFF', font=('Helvetica Neue', 10, 'bold'))
