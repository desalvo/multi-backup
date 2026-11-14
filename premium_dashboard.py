"""Photographic premium dashboard built from the approved Multi Backup design.

The dashboard is a *view* over actual archive/catalog, disk and security data.
It never fabricates scheduled jobs, backup completion, byte counts or remote state.
All operations delegate to existing verified GUI actions and backends.
"""
from __future__ import annotations

import datetime as dt
import math
import shutil
import threading
import time
import tkinter as tk
from pathlib import Path

from PIL import Image, ImageDraw, ImageEnhance, ImageFilter, ImageOps, ImageTk
from archive_ops import catalog
from version import VERSION
from render_quality import rounded_overlay, glossy_patch, smooth_polyline

NAVY = '#0D1A3D'
ICE = '#F4F7FF'
MUTED = '#C2D1F3'
CYAN = '#62D8FF'
LAVENDER = '#BDB3FF'
PANEL = (34, 52, 102, 239)


def format_size(size):
    """A truthful human-readable byte count, without rounding a zero to 1 GB."""
    if size is None:
        return '—'
    value = float(max(0, size))
    for unit in ('B', 'KB', 'MB', 'GB', 'TB', 'PB'):
        if value < 1024 or unit == 'PB':
            return f'{value:.0f} {unit}' if unit == 'B' else f'{value:.1f} {unit}'
        value /= 1024
    return '—'


def summarize_catalog(items, output_directory, now=None):
    """Build display-only metrics from existing archives and filesystem metadata."""
    now = time.time() if now is None else now
    rows = []
    days = [0] * 7
    for item in items:
        try:
            path = Path(item['path'])
            stat = path.stat()
            stamp = float(stat.st_mtime)
            provider = 'Dropbox' if str(item.get('kind', '')).startswith('dropbox') else 'GitHub'
            row = {
                'name': str(item.get('relative') or path.name),
                'provider': provider,
                'size': max(0, int(stat.st_size)),
                'timestamp': stamp,
            }
            rows.append(row)
            elapsed = now - stamp
            if 0 <= elapsed < 7 * 86400:
                days[6 - int(elapsed // 86400)] += 1
        except (KeyError, ValueError, OSError, TypeError, OverflowError):
            continue
    rows.sort(key=lambda row: row['timestamp'], reverse=True)
    disk = None
    try:
        directory = Path(output_directory).expanduser()
        while not directory.exists() and directory != directory.parent:
            directory = directory.parent
        usage = shutil.disk_usage(directory)
        disk = {'total': usage.total, 'used': usage.used, 'free': usage.free}
    except (OSError, ValueError):
        pass
    return {'count': len(rows), 'recent': rows[:3], 'days': days, 'disk': disk}


def _color(value):
    return tuple(int(value[i:i + 2], 16) for i in (1, 3, 5))


def _mix(first, second, fraction):
    t = min(1., max(0., fraction))
    return tuple(round(a * (1 - t) + b * t) for a, b in zip(first, second))


def _glass(image, box, tone=PANEL, radius=19):
    x, y, x2, y2 = (round(v) for v in box)
    width, height = x2 - x, y2 - y
    if width < 8 or height < 8:
        return
    # Multi-layer supersampled geometry removes staircase corners and keeps
    # the subtle luminous border and glass reflection faithful to the mock.
    rounded_overlay(image, (x + 1, y + 3, x2 - 2, y2 - 1),
                    radius, (4, 9, 28, 85))
    rounded_overlay(image, (x, y, x2 - 1, y2 - 4),
                    radius, tone, (157, 179, 255, 101))
    rounded_overlay(image, (x + 2, y + 2, x2 - 3,
                            min(y + 39, y2 - 6)),
                    min(radius - 2, 15), (173, 198, 255, 18))
    smooth_polyline(image, [(x + radius, y + 2),
                            (x2 - radius, y + 2)],
                    (230, 235, 255, 59), 1)


def _button_background(image, box, highlighted=False, primary=False):
    x, y, x2, y2 = (round(v) for v in box)
    width, height = x2 - x, y2 - y
    if width < 10 or height < 10:
        return
    top = (168, 117, 253) if primary else (77, 101, 164)
    bottom = (48, 93, 227) if primary else (31, 52, 106)
    if highlighted:
        top = _mix(top, (235, 220, 255), .18)
        bottom = _mix(bottom, (91, 168, 255), .28)
    patch = glossy_patch(
        width, height, top=top, bottom=bottom,
        border=(187, 205, 255, 200), radius=18, focused=False)
    image.alpha_composite(patch, (x, y))


class DashboardView(tk.Canvas):
    """Responsive glass dashboard with real data, canvas-based keyboard actions."""

    def __init__(self, parent, owner, assets):
        super().__init__(parent, bg=NAVY, height=980, borderwidth=0,
                         highlightthickness=0, takefocus=1)
        self.owner = owner
        self.assets = assets
        self.metrics = {'count': 0, 'recent': [], 'days': [0] * 7, 'disk': None}
        self._picture = None
        self._hero = None
        self._hotspots = []
        self._hover = -1
        self._focus = -1
        self._busy = False
        self._last_scan = 0.
        self._drawn_width = 0
        self._drawn_height = 0
        self._labels = []
        self.bind('<Configure>', self._on_resize)
        self.bind('<Button-1>', self._click)
        self.bind('<Motion>', self._move)
        self.bind('<Leave>', self._leave)
        self.bind('<Return>', self._activate)
        self.bind('<space>', self._activate)
        self.bind('<Tab>', lambda event: self._tab(event, 1))
        self.bind('<Shift-Tab>', lambda event: self._tab(event, -1))
        try:
            self.owner.update_status.trace_add('write', lambda *_: self.redraw())
        except (AttributeError, tk.TclError):
            pass

    def _on_resize(self, event):
        if event.width != self._drawn_width:
            self.redraw()

    def _tab(self, _event, direction):
        if self._hotspots:
            self._focus = (self._focus + direction) % len(self._hotspots)
            self.redraw()
        return 'break'

    def _activate(self, _event):
        if 0 <= self._focus < len(self._hotspots) and not self.owner._locked:
            self._hotspots[self._focus][1]()
        return 'break'

    def _click(self, event):
        self.focus_set()
        if self.owner._locked:
            return
        for idx, (box, action) in enumerate(self._hotspots):
            x, y, x2, y2 = box
            if x <= event.x <= x2 and y <= event.y <= y2:
                self._focus = idx
                action()
                return

    def _move(self, event):
        hovered = -1
        for index, (box, _) in enumerate(self._hotspots):
            x, y, x2, y2 = box
            if x <= event.x <= x2 and y <= event.y <= y2:
                hovered = index
                break
        if hovered != self._hover:
            self._hover = hovered
            self.configure(cursor='hand2' if hovered >= 0 else 'arrow')
            self.redraw()

    def _leave(self, _event):
        if self._hover >= 0:
            self._hover = -1
            self.configure(cursor='arrow')
            self.redraw()

    def _text(self, x, y, text, size=12, *, weight='normal', colour=ICE,
              width=None, anchor='nw'):
        self._labels.append((x, y, str(text), size, weight, colour, width, anchor))

    def _button(self, image, x, y, width, height, text, action, *, primary=False):
        index = len(self._hotspots)
        box = (int(x), int(y), int(x + width), int(y + height))
        self._hotspots.append((box, action))
        _button_background(image, box, self._hover == index or self._focus == index, primary)
        self._text(x + width / 2, y + (height - 4) / 2, text,
                   12 if width < 150 else 13, weight='bold', anchor='center')

    def _tag(self, image, x, y, text, *, enabled=True, width=85):
        rounded_overlay(
            image, (x, y, x + width, y + 25), 12,
            (16, 137, 96, 210) if enabled else (74, 90, 130, 210),
            (113, 241, 192, 185) if enabled else (151, 172, 205, 90))
        self._text(x + width / 2, y + 12, text, 10, weight='bold', anchor='center')

    def _paint(self, width, height):
        image = Image.new('RGBA', (width, height), (13, 26, 62, 255))
        d = ImageDraw.Draw(image, 'RGBA')
        for y in range(height):
            rgb = _mix((17, 33, 74), (10, 21, 50), y / max(1, height))
            d.line((0, y, width, y), fill=(*rgb, 255))
        try:
            if self._hero is None:
                with Image.open(self.assets('hero.jpg')) as source:
                    self._hero = source.convert('RGB').copy()
            photo = ImageOps.fit(self._hero, (width, 255), method=Image.Resampling.LANCZOS,
                                 centering=(.5, .38))
            photo = ImageEnhance.Color(photo).enhance(1.13)
            image.alpha_composite(photo.convert('RGBA'), (0, 0))
        except (OSError, ValueError):
            pass
        layer = Image.new('RGBA', (width, 257), (0, 0, 0, 0))
        painter = ImageDraw.Draw(layer, 'RGBA')
        for x in range(width):
            alpha = max(20, int(236 * (1 - x / max(1, width)) ** 2.4))
            painter.line((x, 0, x, 256), fill=(8, 24, 66, alpha))
        painter.rectangle((0, 206, width, 256), fill=(11, 25, 62, 95))
        image.alpha_composite(layer, (0, 0))
        glow = Image.new('RGBA', (width, height), (0, 0, 0, 0))
        gp = ImageDraw.Draw(glow, 'RGBA')
        gp.ellipse((int(width * .22), 140, int(width * .82), 345),
                   fill=(78, 40, 234, 117))
        gp.ellipse((int(width * .43), 180, int(width * 1.05), 300),
                   fill=(0, 167, 232, 86))
        image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(43)))
        d = ImageDraw.Draw(image, 'RGBA')
        for offset, colour in ((0, (101, 73, 237, 120)), (23, (70, 208, 255, 119)),
                               (38, (176, 95, 252, 112))):
            coords = []
            for x in range(round(width * .34), width + 1, 5):
                portion = x / max(1, width)
                y = 195 + offset + 38 * math.sin(portion * 6 - 2)
                coords.append((x, round(y)))
            if len(coords) > 1:
                smooth_polyline(image, coords, colour, 4)
        return image

    def _run_backup(self):
        # _start performs provider, destination, credential, encryption and
        # destructive-operation confirmations before launching an actual job.
        self.owner._show('Backup')
        self.owner._start()

    def _row(self, image, x, y, width, data, index):
        row = (x + 12, y, x + width - 12, y + 62)
        _glass(image, row, (34, 51, 102, 155), 13)
        provider = data['provider']
        icon = '◆' if provider == 'Dropbox' else '◉'
        colour = CYAN if provider == 'Dropbox' else LAVENDER
        self._text(x + 31, y + 10, icon, 28, colour=colour)
        name = data['name']
        if len(name) > 54:
            name = '…' + name[-53:]
        self._text(x + 76, y + 11, name, 12, weight='bold',
                   width=max(145, width - 260))
        stamp = dt.datetime.fromtimestamp(data['timestamp']).strftime('%d/%m/%Y %H:%M')
        self._text(x + 76, y + 36, f'{provider}  ·  {stamp}', 10, colour=MUTED,
                   width=max(145, width - 258))
        self._text(x + width - 35, y + 22, '›', 22, weight='bold',
                   anchor='center', colour=LAVENDER)
        self._hotspots.append((row, lambda: self.owner._show('Archives')))

    def _stats(self, image, x, y, total_width, columns):
        card_gap = 10
        card_width = (total_width - (columns - 1) * card_gap) / columns
        recent = self.metrics['recent']
        enabled = self.owner.cfg.get('enabled_backends', [])
        unlocked = self.owner._session_secret is not None
        panels = (
            ('◉', 'ULTIMO ARCHIVIO',
             dt.datetime.fromtimestamp(recent[0]['timestamp']).strftime('%d/%m %H:%M')
             if recent else 'Nessuno', 'Rilevato localmente', '#86EFC5'),
            ('▤', 'ARCHIVI DISPONIBILI', str(self.metrics['count']),
             'Nella destinazione', LAVENDER),
            ('◇', 'CIFRATURA', 'Vault sbloccato' if unlocked else 'Vault non creato',
             'age · AES-256-GCM', '#92F1DA'),
            ('☁', 'PLUGIN ATTIVI', str(len(enabled)),
             ', '.join(item.title() for item in enabled) or 'Nessuno', CYAN),
        )
        for index, (symbol, title, value, subtitle, tint) in enumerate(panels):
            column = index % columns
            row = index // columns
            xx = x + column * (card_width + card_gap)
            yy = y + row * 118
            _glass(image, (xx, yy, xx + card_width, yy + 106), (30, 54, 102, 239))
            self._text(xx + 14, yy + 15, symbol, 24, colour=tint)
            self._text(xx + 53, yy + 15, title, 9, colour='#DFE7FF', weight='bold',
                       width=max(70, card_width - 59))
            self._text(xx + 53, yy + 41, value, 13, weight='bold',
                       width=max(65, card_width - 60))
            self._text(xx + 53, yy + 75, subtitle, 9, colour=MUTED,
                       width=max(66, card_width - 60))

    def _archive_panel(self, image, x, y, width, height):
        _glass(image, (x, y, x + width, y + height))
        self._text(x + 22, y + 20, 'I tuoi backup protetti', 18, weight='bold')
        self._text(x + 23, y + 54, 'Archivi locali verificabili dalla libreria',
                   10, colour=MUTED)
        self._button(image, x + width - 175, y + 17, 151, 37,
                     '+ Nuovo backup', lambda: self.owner._show('Backup'))
        rows = self.metrics['recent']
        if not rows:
            self._text(x + 27, y + 128, 'Nessun archivio locale rilevato', 16,
                       weight='bold')
            self._text(x + 27, y + 162,
                       'Avvia il primo backup per popolare automaticamente questa sezione.',
                       11, colour=MUTED, width=max(240, width - 60))
            self._text(x + 27, y + 216,
                       'La pianificazione automatica non è ancora disponibile.',
                       10, colour='#AABDEB', width=max(230, width - 60))
        else:
            for index, record in enumerate(rows):
                self._row(image, x, y + 89 + index * 69, width, record, index)

    def _vault_panel(self, image, x, y, width):
        _glass(image, (x, y, x + width, y + 220), (37, 52, 111, 244))
        unlocked = self.owner._session_secret is not None
        self._text(x + 21, y + 20, '♧', 34, colour=LAVENDER)
        self._text(x + 75, y + 22, 'Vault cifrato', 19, weight='bold')
        self._tag(image, x + 76, y + 59,
                  'Sbloccato' if unlocked else 'Non inizializzato',
                  enabled=unlocked, width=109 if unlocked else 137)
        self._text(x + 22, y + 111,
                   'La chiave privata rimane protetta fuori dall’app.',
                   11, width=width - 42, colour='#E2E9FF')
        self._button(image, x + 16, y + 161, (width - 45) / 2, 43,
                     'Touch ID', lambda: self.owner._show('Encryption'))
        self._button(image, x + 25 + (width - 45) / 2, y + 161,
                     (width - 45) / 2, 43, 'Gestisci chiavi',
                     lambda: self.owner._show('Encryption'))

    def _plugins_panel(self, image, x, y, width):
        _glass(image, (x, y, x + width, y + 183))
        self._text(x + 20, y + 15, '✣', 24, colour=LAVENDER)
        self._text(x + 58, y + 20, 'Plugin', 17, weight='bold')
        self._text(x + 21, y + 50, 'Origini di backup e ripristino', 10, colour=MUTED)
        enabled = self.owner.cfg.get('enabled_backends', [])
        for index, (name, symbol) in enumerate((('github', '◉'), ('dropbox', '◆'))):
            yy = y + 76 + index * 50
            self._text(x + 22, yy, symbol, 21,
                       colour=LAVENDER if name == 'github' else CYAN)
            self._text(x + 58, yy + 2, name.title(), 13, weight='bold')
            self._tag(image, x + width - 100, yy + 1,
                      'Attivo' if name in enabled else 'Spento',
                      enabled=name in enabled, width=77)
            self._hotspots.append(((x + 14, yy - 3, x + width - 12, yy + 38),
                                   lambda: self.owner._show('Plugins')))

    def _updates_panel(self, image, x, y, width):
        _glass(image, (x, y, x + width, y + 115))
        self._text(x + 20, y + 17, '↻', 28, colour=CYAN)
        self._text(x + 60, y + 19, 'Aggiornamenti', 15, weight='bold')
        self._text(x + 20, y + 54, VERSION, 11, colour='#E8F2FF')
        status = self.owner.update_status.get().replace('\n', ' ')
        if len(status) > 63:
            status = status[:62] + '…'
        self._text(x + 20, y + 80, status, 10, colour=MUTED, width=width - 35)
        self._hotspots.append(((x, y, x + width, y + 115),
                               lambda: self.owner._show('Updates')))

    def _storage_panel(self, image, x, y, width, height):
        _glass(image, (x, y, x + width, y + height))
        self._text(x + 22, y + 19, 'Spazio sul disco', 17, weight='bold')
        disk = self.metrics['disk']
        if not disk or disk['total'] <= 0:
            self._text(x + 22, y + 67, 'Spazio locale non disponibile',
                       11, colour=MUTED, width=width - 44)
            return
        used_percent = 100 * disk['used'] / disk['total']
        self._text(x + 22, y + 56,
                   f"{format_size(disk['used'])} usati su {format_size(disk['total'])}",
                   12, width=width - 36)
        start_x = int(x + 22)
        bar_y = int(y + 99)
        end_x = int(x + width - 24)
        rounded_overlay(image, (start_x, bar_y, end_x, bar_y + 23), 11,
                        (14, 25, 55, 255), (123, 157, 226, 180))
        fill_width = max(0, int((end_x - start_x - 5) * used_percent / 100))
        if fill_width >= 4:
            rounded_overlay(image, (start_x + 3, bar_y + 3,
                                    start_x + 3 + fill_width, bar_y + 20),
                            7, (103, 118, 247, 255))
            smooth_polyline(image, [(start_x + 7, bar_y + 5),
                                    (start_x + 3 + fill_width, bar_y + 5)],
                            (118, 223, 255, 185), 2)
        self._text(x + width - 25, y + 56, f'{used_percent:.0f}%',
                   12, anchor='ne', colour=CYAN)

    def _activity_panel(self, image, x, y, width, height):
        _glass(image, (x, y, x + width, y + height))
        self._text(x + 20, y + 15, 'Attività recente', 17, weight='bold')
        self._text(x + 20, y + 45, 'Archivi rilevati negli ultimi 7 giorni',
                   10, colour=MUTED)
        values = self.metrics['days']
        largest = max(1, *values)
        chart_start = x + 25
        chart_width = min(275, width * .48)
        segment = chart_width / 7
        for index, count in enumerate(values):
            h = 9 + 59 * count / largest if count else 6
            bx = int(chart_start + segment * index + 5)
            by = int(y + 124 - h)
            rounded_overlay(
                image, (bx, by, bx + max(10, int(segment - 12)), int(y + 124)),
                5, (94 + index * 8, 95, 238, 230),
                (117, 204, 255, 130))
            self._text(bx + 8, y + 130,
                       ('-6', '-5', '-4', '-3', '-2', '-1', 'Oggi')[index],
                       9, anchor='n', colour=MUTED)
        if width >= 510:
            recent = self.metrics['recent']
            self._text(x + chart_width + 52, y + 71,
                       'Ultimo archivio', 11, weight='bold')
            self._text(x + chart_width + 52, y + 95,
                       (recent[0]['name'][-34:] if recent else
                        'Nessuna attività locale rilevata'),
                       10, colour=MUTED, width=width - chart_width - 79)

    def redraw(self):
        if self._busy:
            return
        width = max(640, self.winfo_width())
        if width < 2:
            return
        wide = width >= 1000
        if wide:
            height = 990
        else:
            height = 1530
        self._busy = True
        try:
            self._drawn_width, self._drawn_height = width, height
            if self.winfo_height() != height:
                self.configure(height=height)
            self._labels = []
            self._hotspots = []
            image = self._paint(width, height)
            self._text(28, 33, 'Multi Backup', 36, weight='bold')
            self._text(31, 82, 'I tuoi dati, sempre al sicuro.', 18, colour='#E9F1FF')
            self._text(31, 117,
                       'Backup multipli, cifratura avanzata,\nplugin e ripristino semplificato.',
                       12, colour='#DCE8FF')
            self._button(image, 25, 163, 236, 51,
                         '▶   Esegui backup ora', self._run_backup, primary=True)
            self._button(image, 275, 163, 168, 51,
                         '◷   Configura…', lambda: self.owner._show('Backup'))
            if wide:
                x, left_width = 20, width - 376
                right_x, right_width = x + left_width + 15, 321
                self._stats(image, x, 246, left_width, 4)
                self._archive_panel(image, x, 374, left_width, 318)
                self._vault_panel(image, right_x, 240, right_width)
                self._plugins_panel(image, right_x, 475, right_width)
                self._updates_panel(image, right_x, 671, right_width)
                bottom_y = 807
                storage_width = min(400, width * .34)
                self._storage_panel(image, 20, bottom_y, storage_width, 152)
                self._activity_panel(image, 33 + storage_width, bottom_y,
                                     width - 53 - storage_width, 152)
            else:
                x, full = 20, width - 40
                self._stats(image, x, 247, full, 2)
                self._archive_panel(image, x, 489, full, 305)
                self._vault_panel(image, x, 811, full)
                self._plugins_panel(image, x, 1045, full)
                self._updates_panel(image, x, 1243, full)
                self._storage_panel(image, x, 1373, full * .46, 142)
                self._activity_panel(image, x + full * .46 + 12, 1373,
                                     full * .54 - 12, 142)
            self._picture = ImageTk.PhotoImage(image.convert('RGB'))
            self.delete('all')
            self.create_image(0, 0, image=self._picture, anchor='nw')
            for x, y, txt, size, weight, colour, maxwidth, anchor in self._labels:
                args = dict(text=txt, anchor=anchor, fill=colour,
                            font=('Helvetica Neue', size, weight))
                if maxwidth is not None:
                    args['width'] = maxwidth
                self.create_text(x, y, **args)
            self._focus = min(self._focus, len(self._hotspots) - 1)
        finally:
            self._busy = False

    def apply_metrics(self, metrics):
        if isinstance(metrics, dict):
            self.metrics = metrics
        self.redraw()

    def refresh(self, force=False):
        self.redraw()
        if self.owner._locked or self._busy or (
                not force and time.monotonic() - self._last_scan < 35):
            return
        self._last_scan = time.monotonic()
        directory = self.owner.root_dir.get()
        def collect():
            try:
                result = summarize_catalog(catalog(Path(directory)), directory)
            except (OSError, ValueError, RuntimeError):
                result = summarize_catalog([], directory)
            self.owner.events.put(('dashboard-metrics', result))
        threading.Thread(target=collect, daemon=True).start()
