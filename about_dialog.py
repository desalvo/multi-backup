"""Native, keyboard-accessible About window for Multi Backup.

Shows build provenance and licensing without unlocking the vault or contacting
the network. The project link opens only after an explicit user action.
"""
from __future__ import annotations

import tkinter as tk
import webbrowser
from pathlib import Path

from PIL import Image, ImageDraw, ImageEnhance, ImageFilter, ImageOps, ImageTk

from render_quality import rounded_overlay
from ui_widgets import PremiumButton
from version import VERSION, BUILD, AUTHOR, REPOSITORY

BG = '#0D1A3D'
TEXT = '#F4F7FF'
MUTED = '#C5D3F2'
PROJECT_URL = 'https://github.com/' + REPOSITORY


class AboutDialog(tk.Toplevel):
    """Non-destructive single-window application information panel."""

    def __init__(self, parent, asset_resolver):
        super().__init__(parent)
        self.title('About Multi Backup')
        self.configure(background=BG)
        self.geometry('680x580')
        self.minsize(610, 550)
        self.transient(parent)
        self.resizable(True, True)
        self._asset = asset_resolver
        self._background = None
        self._icon = None
        self._raw_photo = None
        self._drawn_size = None
        self.protocol('WM_DELETE_WINDOW', self.destroy)
        self.bind('<Escape>', lambda _ev: self.destroy())
        self.bind('<Command-w>', lambda _ev: self.destroy())
        self.bind('<Control-w>', lambda _ev: self.destroy())

        self.surface = tk.Canvas(
            self, background=BG, highlightthickness=0,
            borderwidth=0, takefocus=0)
        self.surface.pack(side='top', fill='both', expand=True)
        self.surface.bind('<Configure>', self._redraw)

        controls = tk.Frame(self, background=BG)
        controls.pack(side='bottom', fill='x', padx=26, pady=(6, 20))
        PremiumButton(
            controls, text='Copy version', style='Dark.TButton',
            command=self._copy_version, width=158).pack(side='left')
        PremiumButton(
            controls, text='Project website', style='Dark.TButton',
            command=self._open_project, width=174).pack(side='left', padx=10)
        close_button = PremiumButton(
            controls, text='Close', style='Accent.TButton',
            command=self.destroy, width=128)
        close_button.pack(side='right')
        close_button.focus_set()
        self.after_idle(self._center_on_parent)

    def _center_on_parent(self):
        try:
            self.update_idletasks()
            parent = self.master
            x = parent.winfo_rootx() + max(0, (parent.winfo_width() - self.winfo_width()) // 2)
            y = parent.winfo_rooty() + max(0, (parent.winfo_height() - self.winfo_height()) // 2)
            self.geometry(f'+{x}+{y}')
        except tk.TclError:
            pass

    def _copy_version(self):
        self.clipboard_clear()
        self.clipboard_append(f'Multi Backup {VERSION} (build {BUILD})')
        self.update_idletasks()

    def _open_project(self):
        webbrowser.open(PROJECT_URL)

    def _image(self, name):
        path = Path(self._asset(name))
        if not path.is_file():
            return None
        try:
            with Image.open(path) as src:
                return src.convert('RGBA').copy()
        except (OSError, ValueError):
            return None

    def _redraw(self, event):
        width, height = max(event.width, 10), max(event.height, 10)
        if self._drawn_size == (width, height):
            return
        self._drawn_size = (width, height)
        image = Image.new('RGBA', (width, height), (13, 26, 61, 255))
        draw = ImageDraw.Draw(image, 'RGBA')
        for y in range(height):
            frac = y / max(1, height)
            draw.line((0, y, width, y),
                      fill=(round(18 - 5 * frac), round(33 - 9 * frac),
                            round(77 - 24 * frac), 255))

        if self._raw_photo is None:
            self._raw_photo = (self._image('backup-hero.jpg') or
                               self._image('hero.jpg'))
        if self._raw_photo is not None:
            # ImageOps.contain preserves the entire photograph without stretch/crop.
            photo = ImageOps.contain(self._raw_photo, (int(width * .36), 174),
                                     method=Image.Resampling.LANCZOS)
            photo = ImageEnhance.Color(photo).enhance(1.12)
            image.alpha_composite(photo, (width - photo.width,
                                          (174 - photo.height) // 2))
        veil = Image.new('RGBA', (width, 180), (0, 0, 0, 0))
        vd = ImageDraw.Draw(veil, 'RGBA')
        for x in range(width):
            alpha = max(35, round(234 * (1 - x / max(1, width)) ** 2))
            vd.line((x, 0, x, 180), fill=(10, 24, 64, alpha))
        image.alpha_composite(veil, (0, 0))

        glow = Image.new('RGBA', (width, height), (0, 0, 0, 0))
        gd = ImageDraw.Draw(glow, 'RGBA')
        gd.ellipse((width * .10, 98, width * .78, 330),
                   fill=(110, 67, 225, 84))
        gd.ellipse((width * .34, 190, width * 1.05, 430),
                   fill=(27, 168, 241, 67))
        image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(40)))

        rounded_overlay(image, (24, 192, width - 24, height - 9), 23,
                        (26, 41, 90, 234), (151, 170, 252, 105))
        rounded_overlay(image, (48, 399, width - 48, 442), 15,
                        (31, 57, 115, 220), (151, 183, 255, 80))
        self._background = ImageTk.PhotoImage(image.convert('RGB'))
        canvas = self.surface
        canvas.delete('all')
        canvas.create_image(0, 0, image=self._background, anchor='nw')
        if self._icon is None:
            icon = self._image('icon-128.png')
            if icon is not None:
                icon = ImageOps.contain(icon, (80, 80),
                                        method=Image.Resampling.LANCZOS)
                self._icon = ImageTk.PhotoImage(icon)
        if self._icon is not None:
            canvas.create_image(47, 49, image=self._icon, anchor='nw')

        canvas.create_text(144, 55, text='MULTI BACKUP', anchor='nw',
                           font=('Helvetica Neue', 24, 'bold'), fill=TEXT)
        canvas.create_text(145, 98, text='Encrypted recovery',
                           anchor='nw', font=('Helvetica Neue', 12),
                           fill='#D1E5FF')
        canvas.create_text(50, 211, text='About this application',
                           anchor='nw', font=('Helvetica Neue', 17, 'bold'),
                           fill=TEXT)
        entries = [
            ('Version', VERSION), ('Build', BUILD), ('Author', AUTHOR),
            ('License', 'EUPL-1.2'), ('Channel', 'Release Candidate'),
        ]
        for i, (label, value) in enumerate(entries):
            y = 252 + i * 28
            canvas.create_text(53, y, text=label, anchor='nw',
                               font=('Helvetica Neue', 11, 'bold'), fill=MUTED)
            canvas.create_text(186, y, text=value, anchor='nw',
                               font=('Helvetica Neue', 11), fill=TEXT,
                               width=max(160, width - 239))

        canvas.create_text(53, 410, text='GitHub  ·  Dropbox  ·  age encryption',
                           anchor='nw', font=('Helvetica Neue', 12, 'bold'),
                           fill='#93E1FF')
        canvas.create_text(
            50, min(height - 43, 459),
            text='Early releases are test builds. Stable distribution requires '
                 'production signing,\nnotarization and complete recovery testing.',
            anchor='nw', font=('Helvetica Neue', 10),
            width=max(200, width - 110), fill='#BACAE7')
