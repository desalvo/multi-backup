"""Multi Backup native desktop shell: backend plugins, backups, archive viewer, keys, updates.

Built with standard Tk/ttk and packaged photographic assets. The core engine is
also accessible via CLI. GUI never performs destructive backup operations by
implicit default; GitHub CI cleanup is explicit and starts as a simulation.
"""
from __future__ import annotations
import json
import os
import queue
import re
import shlex
import shutil
import signal
import subprocess
import sys
import threading
import tkinter as tk
from contextlib import contextmanager
from pathlib import Path
from tkinter import ttk,filedialog,messagebox,simpledialog
from archive_ops import catalog,list_members,read_member,extract_archive,checkout_git,mount_archive
from settings import load_settings,save_settings,set_backend_enabled
from version import VERSION, BUILD, AUTHOR
from ui_widgets import PremiumButton, GradientProgress, HamburgerButton
from premium_dashboard import DashboardView
from archive_management import deletion_plan,delete_local_archive,ArchiveDeletionError
from backup_sessions import dropbox_active_manifest
from key_management import private_key_path
from secure_vault import (vault_path,load_vault,unlock,create_identity,migrate_legacy,
                          export_recovery_copy,change_password,import_recovery_copy,recover_existing_vault,
                          temporary_identity,VaultError)
import biometric
from restore_ops import (RestoreError,restore_provider,validate_plugin,
                         restore_local,restore_github_new_repository,
                         restore_dropbox_new_file)
from backup_paths import prepare_destination
from settings import settings_path
from sidebar_nav import SIDEBAR_BG, SIDEBAR_SEPARATOR, SIDEBAR_HEADING, NAV_ITEMS, SidebarNavItem

ROOT=Path(__file__).resolve().parent
PAGE_BG = '#152753'
PANEL_BG = '#243969'

def asset(name):
    if getattr(sys,'_MEIPASS',None):return Path(sys._MEIPASS)/'assets'/name
    return ROOT/'assets'/name

class BackupGUI(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title('Multi Backup  ·  '+VERSION+'  (build '+BUILD+')')
        self.geometry('1480x960');self.minsize(1070,760)
        self.configure(background='#0D1A3D')
        try:self.iconphoto(True,tk.PhotoImage(file=str(asset('icon-64.png'))))
        except Exception:pass
        self.cfg=load_settings()
        self.events=queue.Queue();self.proc=None;self.selected_archive=None;self.archive_index=[]
        self._session_secret=None
        self._locked=True
        self._startup_completed=False
        self.root_dir=tk.StringVar(value=self.cfg['output_directory'])
        self.staging=tk.StringVar();self.identity=tk.StringVar();self.recipient=tk.StringVar()
        self.github_user=tk.StringVar(value=self.cfg.get('github_username',''))
        self.github_status=tk.StringVar(value='Checking your GitHub connection…')
        self.key_location=tk.StringVar(value='Password-protected recovery vault: '+str(vault_path()))
        self.backup_ci_runs=tk.StringVar(value=str(self.cfg.get('backup_ci_runs',5)))
        self.ci_hint=tk.StringVar()
        self.backup_ci_runs.trace_add('write',lambda *_:self._update_ci_hint())
        self._update_ci_hint()
        self.sidebar_visible=True
        self.advanced_visible=False
        self.backend=tk.StringVar(value='github');self.dropbox_path=tk.StringVar()
        self.keep=tk.StringVar(value=str(self.cfg['keep_ci_runs']))
        self.workers=tk.StringVar(value=str(self.cfg['metadata_workers']))
        self.cleanup=tk.BooleanVar(value=self.cfg['cleanup_ci_artifacts']);self.dryrun=tk.BooleanVar(value=True)
        self.ignore_ci_artifact_errors=tk.BooleanVar(value=self.cfg.get('ignore_ci_artifact_errors',False))
        self._backup_had_omissions=False
        self.restart=tk.BooleanVar(value=False);self.skiplogs=tk.BooleanVar(value=False)
        self.update_channel=tk.StringVar(value=self.cfg['update_channel'])
        self.autocheck=tk.BooleanVar(value=self.cfg['auto_update_check'])
        self.update_status=tk.StringVar(value='Not checked yet')
        self._make_styles();self._make_shell();self._install_app_menu()
        self.protocol('WM_DELETE_WINDOW',self._quit_securely)
        self.bind('<Control-b>',lambda _event:self._toggle_sidebar())
        self.bind('<F1>',lambda _event:self._show_about())
        for index,(name,_) in enumerate(NAV_ITEMS,1):
            self.bind('<Control-Key-'+str(index)+'>',
                      lambda _event,n=name:self._show(n))
        self.bind_all('<MouseWheel>',self._scroll_pages)
        self.bind_all('<Button-4>',self._scroll_pages)
        self.bind_all('<Button-5>',self._scroll_pages)
        self.after(120,self._poll)
        # No network requests, page actions or background jobs before key unlock.
        self.after(35,self._startup_security)

    def _install_app_menu(self):
        """Expose the native Help/About command without touching the vault."""
        bar = tk.Menu(self)
        help_menu = tk.Menu(bar, tearoff=False)
        help_menu.add_command(label='About Multi Backup…',
                              command=self._show_about, accelerator='F1')
        bar.add_cascade(label='Help', menu=help_menu)
        self.configure(menu=bar)
        self._app_menu = bar
        if sys.platform == 'darwin':
            # Finder's application menu routes About to the same single popup.
            try:
                self.tk.createcommand('tk::mac::ShowAbout', self._show_about)
            except tk.TclError:
                pass

    def _show_about(self):
        current = getattr(self, '_about_dialog', None)
        if current is not None:
            try:
                if current.winfo_exists():
                    current.lift()
                    current.focus_set()
                    return
            except tk.TclError:
                pass
        from about_dialog import AboutDialog
        self._about_dialog = AboutDialog(self, asset)

    def _make_styles(self):
        # The entire native shell uses the approved navy/violet glass palette,
        # including secondary pages rather than reverting to stark white forms.
        style = ttk.Style(self)
        try:
            style.theme_use('clam')
        except tk.TclError:
            pass
        style.configure('TFrame', background=PAGE_BG)
        style.configure('TLabel', background=PAGE_BG, foreground='#F2F6FF',
                        font=('Helvetica Neue', 11))
        style.configure('Title.TLabel', background=PAGE_BG,
                        font=('Helvetica Neue', 19, 'bold'),
                        foreground='#F6F8FF')
        style.configure('Sub.TLabel', background=PAGE_BG,
                        font=('Helvetica Neue', 10),
                        foreground='#C3D2F2')
        style.configure('TButton', padding=(12, 9),
                        font=('Helvetica Neue', 10, 'bold'))
        style.configure('Accent.TButton', background='#625DD8',
                        foreground='white', borderwidth=0)
        style.map('Accent.TButton',
                  background=[('active', '#8676EA'),
                              ('disabled', '#52628B')])
        style.configure('TEntry', padding=8, fieldbackground='#EAF1FF',
                        foreground='#182C50', insertcolor='#182C50')
        style.configure('TCombobox', padding=7, fieldbackground='#EAF1FF',
                        background='#5068A9', foreground='#182C50',
                        arrowcolor='#E6F2FF')
        style.map('TCombobox',
                  fieldbackground=[('readonly', '#EAF1FF')],
                  foreground=[('readonly', '#182C50')])
        style.configure('TCheckbutton', background=PAGE_BG,
                        foreground='#EAF2FF', font=('Helvetica Neue', 10),
                        padding=3)
        style.map('TCheckbutton', background=[('active', PAGE_BG)],
                  foreground=[('active', '#FFFFFF')])
        style.configure('Panel.TCheckbutton', background=PANEL_BG,
                        foreground='#F4F7FF', font=('Helvetica Neue', 10),
                        padding=3)
        style.map('Panel.TCheckbutton', background=[('active', PANEL_BG)],
                  foreground=[('active', '#FFFFFF')])
        style.configure('TLabelframe', background=PAGE_BG,
                        borderwidth=1)
        style.configure('TLabelframe.Label', background=PAGE_BG,
                        font=('Helvetica Neue', 11, 'bold'),
                        foreground='#DFE8FF')
        style.configure('Horizontal.TProgressbar', troughcolor='#182B57',
                        background='#7062E1', thickness=16)
        style.configure('Vertical.TScrollbar', background='#4B5D9E',
                        troughcolor=PAGE_BG, arrowcolor='#ECF2FF')

    def _make_shell(self):
        # Canvas navigation ensures explicitly readable text on macOS Aqua:
        # native Tk buttons can ignore fg/activeforeground in dark appearance.
        self.sidebar=tk.Frame(self,bg=SIDEBAR_BG,width=252)
        self.sidebar.pack(side='left',fill='y')
        self.sidebar.pack_propagate(False)
        brand=tk.Frame(self.sidebar,bg=SIDEBAR_BG)
        brand.pack(fill='x',padx=15,pady=(20,12))
        try:
            from PIL import Image,ImageTk
            icon=Image.open(asset('multi-backup.png')).convert('RGBA')
            icon.thumbnail((87,87),Image.Resampling.LANCZOS)
            self.icon_image=ImageTk.PhotoImage(icon)
            tk.Label(brand,image=self.icon_image,bg=SIDEBAR_BG,bd=0).pack(anchor='center')
        except (OSError,ValueError):
            pass
        tk.Label(brand,text='Multi Backup',font=('Helvetica Neue',17,'bold'),
                 fg='#FFFFFF',bg=SIDEBAR_BG).pack(anchor='center',pady=(5,0))
        tk.Label(brand,text=VERSION,font=('Helvetica Neue',10),
                 fg='#C6D4F9',bg=SIDEBAR_BG).pack(anchor='center',pady=(2,0))
        tk.Frame(self.sidebar,height=1,bg=SIDEBAR_SEPARATOR).pack(fill='x',padx=18,pady=(0,12))

        # The footer and the complete photographic inset stay anchored at
        # the bottom. The navigation itself scrolls on shorter notebook screens
        # rather than clipping the last route or overlapping author/build data.
        nav_holder = tk.Frame(self.sidebar, bg=SIDEBAR_BG)
        nav_holder.pack(side='top', fill='both', expand=True)
        self.nav_scroller = tk.Canvas(nav_holder, bg=SIDEBAR_BG,
                                      borderwidth=0, highlightthickness=0)
        scrollbar = ttk.Scrollbar(nav_holder, orient='vertical',
                                  command=self.nav_scroller.yview)
        self.nav_scroller.configure(yscrollcommand=scrollbar.set)
        scrollbar.pack(side='right', fill='y')
        self.nav_scroller.pack(side='left', fill='both', expand=True)
        self.nav_content = tk.Frame(self.nav_scroller, bg=SIDEBAR_BG)
        self._nav_window = self.nav_scroller.create_window(
            (0, 0), window=self.nav_content, anchor='nw')
        self.nav_content.bind(
            '<Configure>', lambda _ev: self.nav_scroller.configure(
                scrollregion=self.nav_scroller.bbox('all')))
        self.nav_scroller.bind(
            '<Configure>', lambda ev: self.nav_scroller.itemconfigure(
                self._nav_window, width=ev.width))
        for item in (self.nav_scroller, self.nav_content):
            item.bind('<MouseWheel>', self._scroll_sidebar)
            item.bind('<Button-4>', self._scroll_sidebar)
            item.bind('<Button-5>', self._scroll_sidebar)

        self.nav = {}
        translated={'Dashboard':'Dashboard','Backup':'Backup','Archives':'Archivi',
                    'Restore':'Ripristina','Plugins':'Plugin',
                    'Encryption':'Vault e chiavi','Updates':'Aggiornamenti',
                    'Settings':'Impostazioni'}
        for name,icon in NAV_ITEMS:
            if name in ('Dashboard','Encryption'):
                section='IL TUO SPAZIO' if name=='Dashboard' else 'PREFERENZE'
                tk.Label(self.nav_content,text=section,font=('Helvetica Neue',9,'bold'),
                         fg=SIDEBAR_HEADING,bg=SIDEBAR_BG,
                         anchor='w').pack(fill='x',padx=25,
                                          pady=(10 if name=='Dashboard' else 19,6))
            btn=SidebarNavItem(self.nav_content,translated[name],icon,
                               lambda n=name:self._show(n))
            btn.pack(fill='x',padx=13,pady=2)
            for wheel in ('<MouseWheel>','<Button-4>','<Button-5>'):
                btn.bind(wheel, self._scroll_sidebar)
            self.nav[name]=btn
        foot=tk.Frame(self.sidebar,bg=SIDEBAR_BG)
        foot.pack(side='bottom',fill='x',padx=18,pady=(6,17))
        tk.Frame(foot,height=1,bg=SIDEBAR_SEPARATOR).pack(fill='x',pady=(0,9))
        for value in ('Licenza: EUPL-1.2',
                      'Autore: '+AUTHOR,
                      'Versione: '+VERSION,
                      'Build: '+BUILD):
            tk.Label(foot,text=value,font=('Helvetica Neue',9),
                     fg='#B4CAE4',bg=SIDEBAR_BG).pack(anchor='center',pady=0)
        PremiumButton(foot,text='About Multi Backup',style='Dark.TButton',
                      command=self._show_about,width=210,height=34).pack(
                      anchor='center',pady=(9,0))
        try:
            from PIL import Image,ImageTk,ImageOps,ImageEnhance,ImageDraw
            with Image.open(asset('backup-sidebar.jpg') if asset('backup-sidebar.jpg').is_file() else asset('hero.jpg')) as photo:
                preview=ImageOps.contain(photo.convert('RGB'),(218,130),
                                         Image.Resampling.LANCZOS)
                # Preserve all original content, without any crop/stretch.
                letterbox=Image.new('RGB',(218,130),SIDEBAR_BG)
                letterbox.paste(preview,((218-preview.width)//2,(130-preview.height)//2))
                preview=letterbox
            preview=ImageEnhance.Color(preview).enhance(1.2)
            mask=Image.new('L',preview.size,0)
            ImageDraw.Draw(mask).rounded_rectangle((0,0,217,129),radius=16,fill=255)
            background=Image.new('RGB',preview.size,SIDEBAR_BG)
            background.paste(preview,(0,0),mask)
            self.sidebar_picture=ImageTk.PhotoImage(background)
            tk.Label(self.sidebar,image=self.sidebar_picture,
                     bg=SIDEBAR_BG,bd=0).pack(side='bottom',padx=14,pady=(0,7))
        except (OSError,ValueError):
            pass
        self.content=tk.Frame(self,bg='#0D1A3D');self.content.pack(fill='both',expand=True)
        toolbar=tk.Frame(self.content,bg='#0D1A3D')
        toolbar.pack(fill='x',padx=(16,18),pady=(5,3))
        self.menu_button=HamburgerButton(
            toolbar,command=self._toggle_sidebar,expanded=True)
        self.menu_button.pack(side='left')
        PremiumButton(toolbar,text='About',style='Dark.TButton',
                      command=self._show_about,width=115,height=36).pack(
                      side='right',padx=(12,0))
        tk.Label(toolbar,text='SPAZIO BACKUP PROTETTO',bg='#0D1A3D',
                 fg='#BDCDF1',font=('Helvetica Neue',10,'bold')).pack(side='right')
        self.hero=tk.Canvas(self.content,height=185,bg='#183855',highlightthickness=0)
        self.hero.pack(fill='x');self.hero.bind('<Configure>',self._draw_hero)
        self.body=tk.Frame(self.content,bg='#0D1A3D')
        self.body.pack(fill='both',expand=True)
        self.page_canvas=tk.Canvas(self.body,bg='#0D1A3D',highlightthickness=0,borderwidth=0)
        page_scroll=ttk.Scrollbar(self.body,orient='vertical',command=self.page_canvas.yview)
        self.page_canvas.configure(yscrollcommand=page_scroll.set)
        page_scroll.pack(side='right',fill='y')
        self.page_canvas.pack(side='left',fill='both',expand=True)
        self.page_host=ttk.Frame(self.page_canvas)
        self.page_window=self.page_canvas.create_window((0,0),window=self.page_host,anchor='nw')
        self.page_host.bind('<Configure>',lambda _ev:self._sync_page_scroll())
        self.page_canvas.bind('<Configure>',self._resize_page_host)
        self.pages={}
        for name in self.nav:
            page=(tk.Frame(self.page_host,bg='#0D1A3D') if name=='Dashboard'
                  else ttk.Frame(self.page_host))
            self.pages[name]=page
        self._build_dashboard(self.pages['Dashboard'])
        self._build_backup(self.pages['Backup'])
        self._build_archives(self.pages['Archives'])
        self._build_restore(self.pages['Restore'])
        self._build_plugins(self.pages['Plugins'])
        self._build_encryption(self.pages['Encryption'])
        self._build_updates(self.pages['Updates'])
        self._build_settings(self.pages['Settings'])

    def _scroll_sidebar(self, event):
        if getattr(event, 'num', None) == 4:
            direction = -1
        elif getattr(event, 'num', None) == 5:
            direction = 1
        elif getattr(event, 'delta', 0):
            direction = -1 if event.delta > 0 else 1
        else:
            return 'break'
        self.nav_scroller.yview_scroll(direction * 2, 'units')
        return 'break'

    def _toggle_sidebar(self):
        if self._locked:return
        self.sidebar_visible=not self.sidebar_visible
        if self.sidebar_visible:
            self.sidebar.pack(side='left',fill='y',before=self.content)
        else:
            self.sidebar.pack_forget()
        self.menu_button.set_expanded(self.sidebar_visible)

    def _resize_page_host(self,event):
        self.page_canvas.itemconfigure(self.page_window,width=event.width)
        self._sync_page_scroll()

    def _sync_page_scroll(self):
        bounds=self.page_canvas.bbox('all')
        if bounds:self.page_canvas.configure(scrollregion=bounds)

    def _scroll_pages(self,event):
        if self._locked or not hasattr(self,'page_canvas'):
            return
        if isinstance(event.widget,(tk.Text,tk.Listbox,ttk.Combobox)):
            return
        if getattr(event,'num',None)==4:direction=-1
        elif getattr(event,'num',None)==5:direction=1
        elif getattr(event,'delta',0):direction=-1 if event.delta>0 else 1
        else:return
        self.page_canvas.yview_scroll(direction*2,'units')

    def _quit_securely(self):
        if isinstance(self._session_secret,bytearray):
            for index in range(len(self._session_secret)):
                self._session_secret[index]=0
        self._session_secret=None
        self.destroy()

    def _ask_new_password(self,title='Protect your encryption key'):
        password=simpledialog.askstring(title,
            'Choose a strong password (at least 12 characters).\n'
            'You will need it to unlock Multi Backup on every launch.',
            show='*',parent=self)
        if password is None:
            return None
        if len(password)<12:
            messagebox.showerror('Weak password','Use at least 12 characters.')
            return None
        again=simpledialog.askstring(title,'Confirm your encryption password:',
                                     show='*',parent=self)
        if again!=password:
            messagebox.showerror('Password mismatch','The two passwords did not match.')
            return None
        return password

    def _unlock_password(self,password):
        clear,recipient=unlock(password)
        self._session_secret=bytearray(clear)
        self.recipient.set(recipient)
        self.key_location.set('Password-protected age vault: '+str(vault_path()))

    def _startup_security(self):
        if self._startup_completed:
            return
        protected=vault_path()
        legacy=private_key_path()
        if protected.is_symlink() or legacy.is_symlink():
            messagebox.showerror('Encryption vault error','Refusing a symlinked encryption key.')
            self._quit_securely()
            return
        if protected.exists():
            unlocked=False
            if self.cfg.get('biometric_enabled') and biometric.available():
                if messagebox.askyesno('Unlock Multi Backup',
                                       'Use Touch ID to unlock your encrypted key?\n'
                                       'Choose No to enter your application password.'):
                    try:
                        self._unlock_password(biometric.unlock_password())
                        unlocked=True
                    except (VaultError,biometric.BiometricError,OSError) as exc:
                        messagebox.showwarning('Touch ID unavailable',
                                               'Biometric unlock was not successful. '
                                               'Your password is still available.\n'+str(exc))
            if not unlocked:
                for attempt in range(5):
                    password=simpledialog.askstring(
                        'Unlock Multi Backup encryption key',
                        'Enter the password protecting your age key.\n'
                        'It will not be requested again until you close the app.',
                        parent=self,show='*')
                    if password is None:
                        self._quit_securely()
                        return
                    try:
                        self._unlock_password(password)
                        unlocked=True
                        break
                    except (VaultError,OSError,ValueError) as exc:
                        messagebox.showerror('Cannot unlock encryption key',str(exc))
            if not unlocked:
                if messagebox.askyesno('Recover encryption key?',
                    'The vault could not be unlocked.\n\n'
                    'Do you have a separately password-protected recovery copy? '
                    'Choose Yes to import it without discarding the previous vault.'):
                    unlocked=self._recover_at_startup()
            if not unlocked:
                messagebox.showerror('Access denied',
                                     'Encryption key remains locked. No data was changed.')
                self._quit_securely()
                return
        elif legacy.exists():
            messagebox.showinfo(
                'Protect your existing age key',
                'A previous unencrypted age identity was found.\n'
                'It will be encrypted with a new password without changing the key, '
                'so your existing backups remain restorable.')
            password=self._ask_new_password('Protect existing encryption key')
            if password is None:
                self._quit_securely()
                return
            try:
                clear,recipient=migrate_legacy(password)
                self._session_secret=bytearray(clear)
                self.recipient.set(recipient)
                self.key_location.set('Password-protected age vault: '+str(protected))
            except (VaultError,OSError,ValueError) as exc:
                messagebox.showerror('Cannot protect existing key',
                                     str(exc)+'\nThe original identity has been preserved.')
                self._quit_securely()
                return
        self._startup_completed=True
        self._locked=False
        self._show('Dashboard')
        self.after(400,self._discover_github_account)
        if self.cfg.get('auto_update_check'):
            self.after(1400,self._check_updates)
            self.after(self.cfg['update_interval_hours']*3600*1000,
                       self._periodic_update)

    def _recover_at_startup(self):
        source=filedialog.askopenfilename(title='Encrypted age recovery copy',
                                          filetypes=[('Age vault','*.agevault'),
                                                     ('All files','*')])
        if not source:return False
        password=simpledialog.askstring('Recovery copy password',
                'Enter the separate password of this recovery file:',
                parent=self,show='*')
        if password is None:return False
        new_password=self._ask_new_password('Set new local vault password')
        if new_password is None:return False
        try:
            clear,recipient=recover_existing_vault(
                source,password,new_password)
            self._session_secret=bytearray(clear)
            self.recipient.set(recipient)
            self.key_location.set('Password-protected age vault: '+str(vault_path()))
            messagebox.showinfo('Recovery successful',
                'Your age identity is restored. The previous vault was preserved '
                'as a .previous file. You can decrypt your existing backups.')
            return True
        except (VaultError,OSError,ValueError) as exc:
            messagebox.showerror('Recovery failed',str(exc))
            return False

    def _prepare_identity(self):
        if self._session_secret is not None:
            return False
        if vault_path().exists():
            raise VaultError('Encryption key is locked. Restart and unlock the application.')
        if private_key_path().exists():
            raise VaultError('A previous key must be protected on application startup.')
        if not messagebox.askyesno('Generate encryption key?',
                    'No encryption key exists yet.\n\n'
                    'Generate a new age key and protect it with a password now?'):
            raise VaultError('Encryption key creation cancelled')
        password=self._ask_new_password()
        if password is None:
            raise VaultError('Backup cancelled before creating the encryption key')
        clear,recipient=create_identity(password)
        self._session_secret=bytearray(clear)
        self.recipient.set(recipient)
        self.key_location.set('Password-protected age vault: '+str(vault_path()))
        return True

    @contextmanager
    def _identity_file(self):
        if self._session_secret is None:
            raise VaultError('Create an encryption key before restoring or browsing backups')
        with temporary_identity(bytes(self._session_secret)) as path:
            yield path

    def _ask_current_password(self,reason):
        password=simpledialog.askstring('Confirm encryption password',
                                         reason+'\nEnter your app encryption password:',
                                         parent=self,show='*')
        if password is None:
            return None
        try:
            clear,_recipient=unlock(password)
            if self._session_secret is None or clear!=bytes(self._session_secret):
                raise VaultError('This is not the unlocked session identity')
            return password
        except (VaultError,ValueError,OSError) as exc:
            messagebox.showerror('Password verification failed',str(exc))
            return None

    def _export_recovery(self):
        password=self._ask_current_password('Protect a recovery copy in another location.')
        if password is None:return
        destination=filedialog.asksaveasfilename(
            title='Save separately encrypted recovery key',
            defaultextension='.agevault',
            initialfile='Multi-Backup-recovery.agevault')
        if not destination:return
        backup_password=self._ask_new_password('Choose recovery-copy password')
        if backup_password is None:return
        try:
            exported=export_recovery_copy(password,backup_password,Path(destination))
            messagebox.showinfo('Recovery copy created',
                'Encrypted recovery copy saved to:\n'+str(exported)+
                '\n\nKeep the copy and its password in a separate safe place.')
        except (VaultError,OSError,ValueError,FileExistsError) as exc:
            messagebox.showerror('Recovery export failed',str(exc))

    def _import_recovery(self):
        if vault_path().exists() or private_key_path().exists():
            messagebox.showwarning('Identity exists',
                'Import is allowed only when no local encryption identity exists. '
                'Existing backups must not lose their decryption key.')
            return
        source=filedialog.askopenfilename(title='Select encrypted age recovery copy',
                                          filetypes=[('Age vault files','*.agevault'),('All files','*')])
        if not source:return
        old=simpledialog.askstring('Recovery copy password',
                                  'Enter the password of the saved recovery copy:',
                                  parent=self,show='*')
        if old is None:return
        new=self._ask_new_password('New local encryption password')
        if new is None:return
        try:
            clear,recipient=import_recovery_copy(Path(source),old,new)
            self._session_secret=bytearray(clear)
            self.recipient.set(recipient)
            self.key_location.set('Password-protected age vault: '+str(vault_path()))
            messagebox.showinfo('Recovery key imported',
                                'Your previous encryption key is ready to restore existing backups.')
        except (VaultError,OSError,ValueError) as exc:
            messagebox.showerror('Cannot import recovery key',str(exc))

    def _change_vault_password(self):
        if self._session_secret is None:
            messagebox.showinfo('No encryption key','Create a key with your first backup.')
            return
        password=self._ask_current_password('Change the primary vault password.')
        if password is None:return
        replacement=self._ask_new_password('New vault password')
        if replacement is None:return
        try:
            change_password(password,replacement)
            if self.cfg.get('biometric_enabled'):
                try:biometric.disable()
                except (biometric.BiometricError,OSError):pass
                self.cfg['biometric_enabled']=False
                save_settings(self.cfg)
            messagebox.showinfo('Password changed',
                                'New password saved. Enable Touch ID again if desired.')
        except (VaultError,OSError,ValueError) as exc:
            messagebox.showerror('Cannot change password',str(exc))

    def _enable_biometrics(self):
        if self._session_secret is None:
            messagebox.showwarning('No encryption key','Create or import an age vault first.')
            return
        if not biometric.available():
            messagebox.showwarning('Touch ID unavailable',
                'Touch ID must be supported and enabled in macOS, and the native '
                'Keychain helper must be installed. Other platforms use the vault password.')
            return
        password=self._ask_current_password('Enable Touch ID for subsequent app launches.')
        if password is None:return
        try:
            if not biometric.enable(password):
                raise biometric.BiometricError('Touch ID enrollment was not confirmed')
            self.cfg['biometric_enabled']=True
            save_settings(self.cfg)
            messagebox.showinfo('Touch ID enabled',
                'This Mac can now unlock the age vault with enrolled Touch ID. '
                'The password remains a recovery option.')
        except (biometric.BiometricError,OSError,ValueError) as exc:
            messagebox.showerror('Touch ID enrollment failed',str(exc))

    def _disable_biometrics(self):
        password=self._ask_current_password('Disable biometric app unlocking.')
        if password is None:return
        try:
            biometric.disable()
            self.cfg['biometric_enabled']=False
            save_settings(self.cfg)
            messagebox.showinfo('Touch ID disabled',
                                'The vault password is required at the next app launch.')
        except (biometric.BiometricError,OSError,ValueError) as exc:
            messagebox.showerror('Cannot disable Touch ID',str(exc))

    def _update_ci_hint(self):
        count=self.backup_ci_runs.get().strip() or '?'
        self.ci_hint.set('Latest '+count+
            ' GitHub Actions runs per repository backed up regardless of CI cleanup.')

    def _draw_hero(self,event):
        canvas=self.hero;canvas.delete('all');w=max(1,event.width);h=185
        try:
            from PIL import Image,ImageTk,ImageEnhance,ImageDraw,ImageOps
            source=asset('backup-hero.jpg') if asset('backup-hero.jpg').is_file() else asset('hero.jpg')
            with Image.open(source) as original:
                photo=ImageOps.contain(original.convert('RGB'),(w,h),
                                       Image.Resampling.LANCZOS)
            background=Image.new('RGB',(w,h),'#101F46')
            # Align to the right, keeping the COMPLETE photographic composition.
            background.paste(photo,(max(0,w-photo.width),(h-photo.height)//2))
            overlay=Image.new('RGBA',(w,h),(0,0,0,0))
            pen=ImageDraw.Draw(overlay)
            for x in range(w):
                a=int(235*(1-x/max(w,1))**1.7)
                pen.line((x,0,x,h),fill=(6,17,49,a))
            background=Image.alpha_composite(background.convert('RGBA'),overlay)
            self.hero_image=ImageTk.PhotoImage(background.convert('RGB'))
            canvas.create_image(0,0,anchor='nw',image=self.hero_image)
        except (OSError,ValueError):pass
        canvas.create_text(35,57,text='Multi Backup',font=('Helvetica',28,'bold'),fill='white',anchor='w')
        canvas.create_text(35,92,text='I tuoi dati, sempre al sicuro.',font=('Helvetica',13),fill='#eaf5ff',anchor='w')
        canvas.create_text(35,133,text='Backup cifrati  /  GitHub + Dropbox  /  Ripresa affidabile',
                           font=('Helvetica',10),fill='#dbeeff',anchor='w')

    def _show(self,name):
        if self._locked or name not in self.pages:
            return
        for key,page in self.pages.items():
            page.pack_forget()
            self.nav[key].set_active(key==name)
        if name=='Dashboard':
            self.hero.pack_forget()
            self.page_canvas.configure(bg='#0D1A3D')
            self.pages[name].pack(fill='both',expand=True,padx=0,pady=0)
            self.dashboard.refresh()
        else:
            self.hero.pack(fill='x',before=self.body)
            self.page_canvas.configure(bg=PAGE_BG)
            self.pages[name].pack(fill='both',expand=True,padx=28,pady=22)
        self.page_canvas.yview_moveto(0)
        self._sync_page_scroll()
        if name=='Archives':self._refresh()
        if name=='Restore':self._refresh_restore()

    def _heading(self,parent,title,sub=''):
        ttk.Label(parent,text=title,style='Title.TLabel').pack(anchor='w',pady=(0,5))
        if sub:ttk.Label(parent,text=sub,style='Sub.TLabel').pack(anchor='w',pady=(0,17))

    def _build_dashboard(self,page):
        """Approved photographic dashboard; indicators reflect actual local state."""
        self.dashboard=DashboardView(page,self,asset)
        self.dashboard.pack(fill='both',expand=True)

    def _line(self,parent,label,var,browse=None):
        row=ttk.Frame(parent);row.pack(fill='x',pady=4)
        ttk.Label(row,text=label,width=20).pack(side='left')
        ttk.Entry(row,textvariable=var).pack(side='left',fill='x',expand=True)
        if browse:
            PremiumButton(row,text='Browse',command=lambda:var.set(
                filedialog.askopenfilename() if browse=='file' else filedialog.askdirectory())).pack(side='left',padx=(6,0))
        return row

    def _build_backup(self,page):
        self._heading(page,'GitHub backup',
                      'Choose where to store backups and which GitHub account to protect.')
        self._line(page,'Backup destination',self.root_dir,'directory')
        self.source_fields=ttk.Frame(page)
        self.source_fields.pack(fill='x')
        self.github_row=self._line(self.source_fields,'GitHub username',self.github_user)
        PremiumButton(self.github_row,text='Detect',
                      command=lambda:self._discover_github_account(True)).pack(side='left',padx=(8,0))
        self.dropbox_row=self._line(self.source_fields,'Dropbox folder',self.dropbox_path)
        self.auth_status_label=ttk.Label(page,textvariable=self.github_status,style='Sub.TLabel')
        self.auth_status_label.pack(anchor='w',pady=(3,0))
        self.connect_button=PremiumButton(page,text='Connect GitHub account',
                                          command=self._connect_github)
        self.connect_button.pack(anchor='w',pady=(5,9))
        ttk.Label(page,textvariable=self.ci_hint,style='Sub.TLabel',
                  wraplength=710).pack(anchor='w',pady=(2,8))
        self.advanced_button=PremiumButton(page,text='Advanced options ▾',
                                            command=self._toggle_advanced)
        self.advanced_button.pack(anchor='w',pady=(4,6))
        self.advanced_frame=ttk.Frame(page)
        backend_row=ttk.Frame(self.advanced_frame)
        backend_row.pack(anchor='w',fill='x',pady=6)
        ttk.Label(backend_row,text='Backup provider').pack(side='left',padx=(0,12))
        combo=ttk.Combobox(backend_row,textvariable=self.backend,
                           values=('github','dropbox'),state='readonly',width=18)
        combo.pack(side='left')
        combo.bind('<<ComboboxSelected>>',lambda _:self._update_backend_fields())
        options=ttk.Frame(self.advanced_frame)
        options.pack(fill='x',pady=5)
        for label,var in [
            ('Cleanup CI artifacts only after verified backup',self.cleanup),
            ('Simulate cleanup only',self.dryrun),
            ('Skip CI logs',self.skiplogs),
            ('Ignora errori di download degli artifact CI (salva il report delle omissioni)',self.ignore_ci_artifact_errors),
        ]:
            ttk.Checkbutton(options,text=label,variable=var).pack(anchor='w',pady=2)

        self.commands_frame=ttk.Frame(page)
        self.commands_frame.pack(fill='x',pady=(8,6))
        self.start_button=PremiumButton(self.commands_frame,text='Start · Nuovo backup',
                                        style='Accent.TButton',
                                        command=lambda:self._start(mode='new'))
        self.start_button.pack(side='left')
        self.resume_button=PremiumButton(self.commands_frame,text='Resume · Riprendi backup',
                                         style='Dark.TButton',
                                         command=lambda:self._start(mode='resume'))
        self.resume_button.pack(side='left',padx=(9,0))
        self.stop_button=PremiumButton(self.commands_frame,text='Interrompi in sicurezza',
                                       command=self._stop,state='disabled')
        self.stop_button.pack(side='left',padx=(9,0))
        self.status=tk.StringVar(value='Ready to protect your account')
        ttk.Label(page,textvariable=self.status,wraplength=710).pack(anchor='w',pady=5)
        self.progress=GradientProgress(page)
        self.progress.pack(fill='x',pady=(1,8))
        logs=ttk.Frame(page)
        logs.pack(fill='both',expand=True,pady=5)
        self.text=tk.Text(logs,height=8,wrap='word',background='#182a40',
                          foreground='#E5F3FF',insertbackground='white',
                          relief='flat',font=('Courier',10),state='disabled')
        bar=ttk.Scrollbar(logs,command=self.text.yview)
        self.text.configure(yscrollcommand=bar.set)
        self.text.pack(side='left',fill='both',expand=True)
        bar.pack(side='right',fill='y')
        self._update_backend_fields()

    def _toggle_advanced(self):
        self.advanced_visible=not self.advanced_visible
        if self.advanced_visible:
            self.advanced_frame.pack(fill='x',before=self.commands_frame,pady=(0,5))
        else:
            self.advanced_frame.pack_forget()
        self.advanced_button.set_text('Advanced options ▴' if self.advanced_visible
                                      else 'Advanced options ▾')

    def _update_backend_fields(self):
        if self.backend.get()=='dropbox':
            self.github_row.pack_forget()
            self.dropbox_row.pack(fill='x')
            self.auth_status_label.pack_forget()
            self.connect_button.pack_forget()
        else:
            self.dropbox_row.pack_forget()
            self.github_row.pack(fill='x')
            self.auth_status_label.pack(anchor='w',pady=(3,0))
            self.connect_button.pack(anchor='w',pady=(5,9))

    def _build_archives(self,page):
        self._heading(page,'Archive library','Inspect, extract, checkout or mount encrypted repository snapshots.')
        row=ttk.Frame(page);row.pack(fill='x')
        for name,fn in [('Refresh',self._refresh),('List contents',self._view),('Show file',self._show_member),
                        ('Extract',self._extract),('Checkout Git',self._checkout),('Mount read-only',self._mount),
                        ('Restore…',lambda:self._show('Restore')),
                        ('Elimina archivio…',self._delete_selected_archive)]:
            PremiumButton(row,text=name,command=fn).pack(side='left',padx=(0,7))
        self.archives=tk.Listbox(page,height=9,background='#0D214C',
                                 foreground='#EDF3FF',border=0,
                                 selectforeground='#FFFFFF',selectbackground='#6056C9',
                                 font=('Courier',10))
        self.archives.pack(fill='both',expand=True,pady=(12,5))
        ttk.Label(page,text='Encrypted archive members').pack(anchor='w')
        self.members=tk.Listbox(page,height=9,background='#0D214C',
                                foreground='#EDF3FF',border=0,
                                selectforeground='#FFFFFF',selectbackground='#6056C9',
                                font=('Courier',10))
        self.members.pack(fill='both',expand=True,pady=7)

    def _build_restore(self,page):
        self._heading(page,'Restore protected backups',
            'Select an encrypted archive. Only its original backup plugin can restore it.')
        ttk.Label(page,text='GitHub snapshots restore to a new repository, Dropbox objects '
                  'to a new file. Local extraction is also supported.',
                  style='Sub.TLabel',wraplength=710).pack(anchor='w',pady=(0,8))
        row=ttk.Frame(page)
        row.pack(fill='x',pady=5)
        PremiumButton(row,text='Refresh backup catalog',
                      command=self._refresh_restore).pack(side='left',padx=(0,10))
        PremiumButton(row,text='Browse archives',
                      command=lambda:self._show('Archives')).pack(side='left')
        self.restore_list=tk.Listbox(page,height=9,background='#0D214C',border=0,
                                     foreground='#EDF3FF',selectforeground='#FFFFFF',
                                     selectbackground='#6056C9',font=('Courier',10),
                                     exportselection=False)
        self.restore_list.pack(fill='x',pady=(8,7))
        self.restore_list.bind('<<ListboxSelect>>',self._restore_picked)
        self.restore_summary=tk.StringVar(value='Choose an encrypted backup')
        ttk.Label(page,textvariable=self.restore_summary,style='Sub.TLabel',
                  wraplength=720).pack(anchor='w',pady=4)
        row2=ttk.Frame(page)
        row2.pack(fill='x',pady=(8,5))
        ttk.Label(row2,text='Backup plugin').pack(side='left',padx=(0,12))
        self.restore_plugin=tk.StringVar(value='github')
        picker=ttk.Combobox(row2,textvariable=self.restore_plugin,
                           values=('github','dropbox'),width=16,state='readonly')
        picker.pack(side='left')
        ttk.Label(row2,text='Restore mode').pack(side='left',padx=(24,12))
        self.restore_mode=tk.StringVar(value='Local extraction')
        options=ttk.Combobox(row2,textvariable=self.restore_mode,
                             values=('Local extraction','Restore to provider'),
                             state='readonly',width=21)
        options.pack(side='left')
        self.restore_destination=tk.StringVar()
        self._line(page,'Restore destination',self.restore_destination)
        ttk.Label(page,text='Local: new directory path. GitHub remote: account/new-repo '
                  '(must not exist). Dropbox remote: /new/path/file (must not exist). '
                  'Your provider authentication will be checked at restore time.',
                  style='Sub.TLabel',wraplength=710).pack(anchor='w',pady=(5,10))
        PremiumButton(page,text='Restore selected backup',
                      style='Accent.TButton',command=self._restore_now).pack(
                      anchor='w',pady=(3,8))
        ttk.Label(page,text='Remote restore never overwrites an existing target. '
                  'GitHub account settings and CI history are available for local '
                  'recovery but are not automatically recreated on GitHub.',
                  style='Sub.TLabel',wraplength=710).pack(anchor='w',pady=(8,0))
        self.restore_index=[]

    def _refresh_restore(self):
        try:
            self.restore_index=catalog(Path(self.root_dir.get()))
            selected=self.restore_list.curselection()
            self.restore_list.delete(0,'end')
            for item in self.restore_index:
                origin='Dropbox' if item['kind'].startswith('dropbox') else 'GitHub'
                self.restore_list.insert('end',origin+' · '+item['relative'])
            self.restore_summary.set(str(len(self.restore_index))+' encrypted backups available')
            if selected and selected[0]<len(self.restore_index):
                self.restore_list.selection_set(selected[0])
        except (OSError,ValueError,RuntimeError) as exc:
            messagebox.showerror('Cannot read backup catalog',str(exc))

    def _restore_picked(self,_event=None):
        selected=self.restore_list.curselection()
        if not selected or selected[0]>=len(self.restore_index):
            return
        archive=self.restore_index[selected[0]]
        try:
            plugin=restore_provider(Path(archive['path']),Path(self.root_dir.get()))
            self.restore_plugin.set(plugin)
            self.restore_summary.set('Original plugin: '+plugin+
                                     ' · '+archive['relative'])
        except RestoreError as exc:
            self.restore_summary.set('Cannot restore: '+str(exc))

    def _restore_now(self):
        if self._locked or self._session_secret is None:
            messagebox.showwarning('Encryption key required',
                                   'Unlock the password-protected key before restoring.')
            return
        selection=self.restore_list.curselection()
        if not selection or selection[0]>=len(self.restore_index):
            messagebox.showwarning('Select backup','Choose an encrypted backup first.')
            return
        selected=self.restore_index[selection[0]]
        archive=Path(selected['path'])
        plugin=self.restore_plugin.get()
        root=Path(self.root_dir.get())
        try:
            validate_plugin(archive,root,plugin)
        except RestoreError as exc:
            messagebox.showerror('Incompatible restore plugin',str(exc))
            return
        mode=self.restore_mode.get()
        destination=self.restore_destination.get().strip()
        if mode=='Local extraction' and not destination:
            parent=filedialog.askdirectory(title='Choose parent folder for a new restore directory')
            if not parent:return
            destination=str(Path(parent)/('Multi-Backup-Restore-'+
                               str(int(__import__('time').time()))))
            self.restore_destination.set(destination)
        if not destination:
            messagebox.showwarning('Destination required',
                                   'Provide a destination before restoring.')
            return
        if mode=='Restore to provider':
            action=('Create a NEW GitHub repository '+destination if plugin=='github'
                    else 'Upload a NEW Dropbox file '+destination)
            note='Remote restore will not overwrite an existing destination. '
            if plugin=='github' and selected['kind']!='git':
                messagebox.showerror('Unsupported remote restore',
                    'Select a Git mirror for GitHub remote restoration. '
                    'CI, account settings and metadata can be restored locally.')
                return
            if plugin=='dropbox' and selected['kind']!='dropbox':
                messagebox.showerror('Unsupported remote restore',
                    'Select an individual Dropbox object for remote file restoration.')
                return
        elif mode=='Local extraction':
            action='Extract encrypted '+plugin+' backup to '+destination
            note='An existing folder will never be overwritten. '
        else:
            messagebox.showerror('Invalid restore mode','Choose a supported restore mode.')
            return
        if not messagebox.askyesno('Confirm restore',
                                   action+'?\n\n'+note+
                                   'The original encrypted backup will remain unchanged.'):
            return
        token=None
        if mode=='Restore to provider' and plugin=='dropbox':
            token=simpledialog.askstring(
                'Dropbox restore credentials',
                'Enter a Dropbox access token with files.content.write permission. '
                'It is used only for this restore and is not saved.',
                parent=self,show='*')
            if token is None:return
        def work():
            with self._identity_file() as identity:
                if mode=='Local extraction':
                    return restore_local(archive,root,plugin,identity,Path(destination))
                if plugin=='github':
                    return restore_github_new_repository(
                        archive,root,identity,destination)
                return restore_dropbox_new_file(
                    archive,root,identity,destination,token)
        self.status.set('Restoring encrypted backup…')
        self._background(work)

    def _build_plugins(self,page):
        self._heading(page,'Backup engines','Each provider is an independent plugin with its own capabilities.')
        self.plugin_vars={}
        for backend,detail in [('github','Git repositories, releases, CI metadata and artifacts; optional cleanup'),
                                ('dropbox','Incremental file revisions; read-only remote operations; no purge')]:
            frame=tk.Frame(page,bg=PANEL_BG,highlightbackground='#586FA8',highlightthickness=1)
            frame.pack(fill='x',pady=10,ipady=18)
            tk.Label(frame,text=backend.upper(),font=('Helvetica Neue',14,'bold'),
                     fg='#F5F8FF',bg=PANEL_BG).pack(anchor='w',padx=16)
            tk.Label(frame,text=detail,font=('Helvetica Neue',10),
                     fg='#C8D8F8',bg=PANEL_BG).pack(anchor='w',padx=16)
            var=tk.BooleanVar(value=backend in self.cfg['enabled_backends']);self.plugin_vars[backend]=var
            check=ttk.Checkbutton(frame,text='Enabled',variable=var,
                                  style='Panel.TCheckbutton',
                                  command=lambda b=backend,v=var:self._set_backend(b,v.get()))
            check.pack(anchor='e',padx=16)
        ttk.Label(page,text='Dropbox uses MULTI_BACKUP_DROPBOX_TOKEN or the operating-system keyring. Credentials are never stored in plain settings.',
                  wraplength=700).pack(anchor='w',pady=10)

    def _set_backend(self,name,value):
        try:
            self.cfg=set_backend_enabled(name,value)
        except Exception as exc:messagebox.showerror('Plugin error',str(exc))

    def _build_encryption(self,page):
        self._heading(page,'Protected encryption key',
                      'Unlock once per app session; back up your private key separately.')
        ttk.Label(page,text='Your age private identity is encrypted with your password '
                  'and stored outside Multi Backup.app. Updates preserve it.',
                  style='Sub.TLabel',wraplength=710).pack(anchor='w',pady=(1,10))
        ttk.Label(page,textvariable=self.key_location,style='Sub.TLabel',
                  wraplength=710).pack(anchor='w',pady=(1,10))
        row=ttk.Frame(page)
        row.pack(fill='x',pady=7)
        PremiumButton(row,text='Create encryption key',style='Accent.TButton',
                      command=self._create_from_encryption_page).pack(side='left',padx=(0,10))
        PremiumButton(row,text='Locate encrypted vault',
                      command=self._reveal_key).pack(side='left')
        row2=ttk.Frame(page)
        row2.pack(fill='x',pady=7)
        PremiumButton(row2,text='Save encrypted recovery copy',
                      command=self._export_recovery,
                      style='Accent.TButton').pack(side='left',padx=(0,10))
        PremiumButton(row2,text='Import recovery copy',
                      command=self._import_recovery).pack(side='left')
        row3=ttk.Frame(page)
        row3.pack(fill='x',pady=7)
        PremiumButton(row3,text='Change vault password',
                      command=self._change_vault_password).pack(side='left',padx=(0,10))
        PremiumButton(row3,text='Show public recipient',
                      command=self._public_key).pack(side='left')
        ttk.Label(page,text='Touch ID (macOS): available after a password unlock '
                  'and explicit opt-in. Enrollment uses the macOS Keychain '
                  'and current biometric set.',
                  style='Sub.TLabel',wraplength=710).pack(anchor='w',pady=(19,5))
        row4=ttk.Frame(page)
        row4.pack(fill='x',pady=7)
        PremiumButton(row4,text='Enable Touch ID',
                      command=self._enable_biometrics).pack(side='left',padx=(0,10))
        PremiumButton(row4,text='Disable Touch ID',
                      command=self._disable_biometrics).pack(side='left')
        self.key_info=tk.Text(page,height=5,wrap='word',background='#0D214C',
                              foreground='#EDF3FF',insertbackground='#FFFFFF',
                              relief='flat',font=('Courier',10))
        self.key_info.pack(fill='x',pady=14)
        ttk.Label(page,text='Save the recovery copy to another location, protected by '
                  'a separate password. Without the key or its recovery copy, '
                  'encrypted backups cannot be decrypted.',
                  style='Sub.TLabel',wraplength=710).pack(anchor='w',pady=7)

    def _create_from_encryption_page(self):
        if self._session_secret is not None:
            messagebox.showinfo('Encryption key ready',
                                'Your current age key is already unlocked for this session.')
            return
        try:
            if self._prepare_identity():
                messagebox.showinfo('Encryption key created',
                    'Your age key is password-protected at:\n'+str(vault_path())+
                    '\n\nCreate an independently encrypted recovery copy before '
                    'relying on it for important backups.')
        except (VaultError,OSError,ValueError) as exc:
            messagebox.showerror('Key creation cancelled or failed',str(exc))

    def _reveal_key(self):
        path=vault_path()
        if not path.is_file():
            messagebox.showinfo('Protected vault',
                'The encrypted vault will be created on the first backup.\n'
                'Expected location:\n'+str(path))
            return
        messagebox.showinfo('Password-protected vault',
            'Your protected vault is saved outside the application at:\n'+str(path)+
            '\n\nIt is not your plaintext private key. '
            'Use Save encrypted recovery copy to create another protected backup.')
        if sys.platform=='darwin':
            try:subprocess.Popen(['open','-R',str(path)])
            except OSError:pass
        elif os.name=='nt':
            try:subprocess.Popen(['explorer','/select,',str(path)])
            except OSError:pass

    def _public_key(self):
        if self._session_secret is None:
            messagebox.showwarning('No encryption key',
                                   'Create and unlock the age identity first.')
            return
        self.key_info.insert('end','Public recipient: '+self.recipient.get()+'\n')
        self.key_info.see('end')

    def _discover_github_account(self,force=False):
        def work():
            try:
                result=subprocess.run(['gh','api','user','--jq','.login'],
                                      text=True,capture_output=True,timeout=25,check=False)
                login=result.stdout.strip()
                if result.returncode or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9-]{0,38}',login):
                    raise RuntimeError('Not connected. Choose Connect GitHub account.')
                self.events.put(('github-login',(login,force)))
            except (OSError,subprocess.TimeoutExpired,RuntimeError) as exc:
                self.events.put(('github-error',str(exc)))
        threading.Thread(target=work,daemon=True).start()

    def _connect_github(self):
        if sys.platform=='darwin':
            try:
                binary=shutil.which('gh')
                if not binary:
                    raise RuntimeError('GitHub CLI (gh) is unavailable')
                directory=settings_path().parent
                if directory.is_symlink():
                    raise RuntimeError('Refusing a symlinked application directory')
                directory.mkdir(parents=True,exist_ok=True,mode=0o700)
                script=directory/'connect-github.command'
                if script.is_symlink():
                    raise RuntimeError('Refusing a symlinked GitHub sign-in launcher')
                script.write_text('#!/bin/sh\n'+shlex.quote(binary)+
                                  ' auth login --hostname github.com --git-protocol https --web\n'
                                  'echo "Return to Multi Backup and click Detect after login."\n'
                                  'read -r _\n',encoding='utf-8')
                script.chmod(0o700)
                subprocess.Popen(['open','-a','Terminal',str(script)])
                self.github_status.set('Complete sign-in in Terminal, then click Detect.')
            except (OSError,RuntimeError) as exc:
                messagebox.showerror('GitHub sign-in',str(exc))
        else:
            messagebox.showinfo('GitHub authentication',
                                'Run gh auth login in a terminal once, then click Detect.\n'
                                'The connected account must match the GitHub username.')

    def _build_updates(self,page):
        self._heading(page,'Application updates','Check stable and early-release packages using trusted TLS and SHA-256.')
        row=ttk.Frame(page);row.pack(anchor='w',pady=16)
        ttk.Label(row,text='Release channel').pack(side='left',padx=(0,12))
        combo=ttk.Combobox(row,textvariable=self.update_channel,values=('stable','early-release'),width=22,state='readonly')
        combo.pack(side='left');combo.bind('<<ComboboxSelected>>',lambda _:self._save_preferences())
        PremiumButton(page,text='Check for updates',style='Accent.TButton',command=self._check_updates).pack(anchor='w',pady=12)
        PremiumButton(page,text='Download verified update',command=self._download_update).pack(anchor='w')
        ttk.Label(page,textvariable=self.update_status,wraplength=720).pack(anchor='w',pady=20)
        ttk.Label(page,text='After download, the operating system installer opens only with your approval.\n'
                  'Unattended installation is disabled until code signing and notarization are configured.',
                  style='Sub.TLabel',wraplength=720).pack(anchor='w',pady=12)

    def _build_settings(self,page):
        self._heading(page,'Preferences','Application updates, CI retention and advanced limits.')
        ttk.Checkbutton(page,text='Check for updates at startup and periodically',
                        variable=self.autocheck,command=self._save_preferences).pack(anchor='w',pady=7)
        ttk.Label(page,text='Update channel').pack(anchor='w',pady=6)
        box=ttk.Combobox(page,textvariable=self.update_channel,
                         values=('stable','early-release'),state='readonly')
        box.pack(anchor='w')
        box.bind('<<ComboboxSelected>>',lambda _:self._save_preferences())
        self._line(page,'CI runs to back up per repo',self.backup_ci_runs)
        ttk.Label(page,text='Always include this many recent GitHub Actions runs, '
                  'even if you do not enable cleanup.',style='Sub.TLabel',
                  wraplength=710).pack(anchor='w',pady=(1,12))
        self._line(page,'CI runs to keep if cleaning',self.keep)
        ttk.Label(page,text='Cleanup is optional, off by default, and happens only '
                  'after a successful backup.',style='Sub.TLabel',
                  wraplength=710).pack(anchor='w',pady=(1,12))
        self._line(page,'Parallel workers',self.workers)
        PremiumButton(page,text='Save all preferences',
                      style='Accent.TButton',command=self._save_preferences).pack(
                      anchor='w',pady=18)

    def _save_preferences(self):
        try:
            self.cfg.update({
                'output_directory':self.root_dir.get(),
                'github_username':self.github_user.get().strip(),
                'backup_ci_runs':int(self.backup_ci_runs.get()),
                'update_channel':self.update_channel.get(),
                'auto_update_check':self.autocheck.get(),
                'metadata_workers':int(self.workers.get()),
                'keep_ci_runs':int(self.keep.get()),
                'cleanup_ci_artifacts':self.cleanup.get(),
                'ignore_ci_artifact_errors':self.ignore_ci_artifact_errors.get(),
                'default_recipient':self.recipient.get(),
            })
            save_settings(self.cfg)
            return True
        except (ValueError,OSError,RuntimeError) as exc:
            messagebox.showerror('Settings error',str(exc))
            return False

    def _cli(self):
        if getattr(sys,'frozen',False):
            path=Path(sys.executable).with_name('multi-backup.exe' if os.name=='nt' else 'multi-backup')
            if not path.is_file():raise RuntimeError('Bundled CLI executable not found')
            return [str(path)]
        return [sys.executable,str(ROOT/'multi_backup.py')]

    def _start(self,mode='new'):
        if mode not in ('new','resume'):
            raise ValueError('Unknown backup action')
        if self.proc is not None and self.proc.poll() is None:return
        backend=self.backend.get()
        if backend not in self.cfg['enabled_backends']:
            messagebox.showwarning('Backend disabled',
                                   'Enable this backend in Plugins first.')
            return
        chosen=self.github_user.get().strip()
        if backend=='github' and not chosen:
            messagebox.showwarning('GitHub username required',
                                   'Enter your GitHub username or click Detect.')
            return
        if backend=='github' and not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9-]{0,38}',chosen):
            messagebox.showerror('Invalid GitHub username','Please enter a valid GitHub login.')
            return
        try:
            safe=prepare_destination(self.root_dir.get())
            self.root_dir.set(str(safe))
        except (OSError,ValueError,RuntimeError) as exc:
            messagebox.showerror('Destination unavailable',str(exc))
            return
        # Start and Resume are intentionally different operations. Resume
        # refuses a missing/incompatible checkpoint rather than creating a new
        # cycle under a different name.
        if backend=='github':
            checkpoint=Path(self.root_dir.get())/'.state'/'active.json'
        else:
            checkpoint=Path(self.root_dir.get())/'.state'/'dropbox-active.json'
        if mode=='resume' and not checkpoint.is_file():
            messagebox.showinfo('Nessun backup da riprendere',
                                'Non esiste un checkpoint incompleto in questa destinazione. '
                                'Usa Start per un nuovo backup.')
            return
        if mode=='new' and checkpoint.is_file():
            if not messagebox.askyesno(
                    'Avviare un nuovo backup?',
                    'Esiste un backup non terminato. Start creerà un NUOVO '
                    'nome/ciclo conservando il precedente checkpoint per '
                    'ispezione. Per continuarlo senza cambiare nome usa Resume. '
                    'Procedere con un backup nuovo?'):
                return
        if not self._save_preferences():return
        try:
            created=self._prepare_identity()
        except (OSError,ValueError,RuntimeError) as exc:
            messagebox.showerror('Backup encryption unavailable',str(exc))
            return
        if created:
            messagebox.showwarning(
                'Make an encrypted recovery copy',
                'Your protected age identity is saved at:\n\n'+str(vault_path())+
                '\n\nOpen Encryption → Save encrypted recovery copy and store it '
                'in a separate location. Without this vault or its recovery copy '
                'you cannot decrypt your backups.')
        args=self._cli()+['backup','--backend',backend,
                          '--output-dir',self.root_dir.get(),
                          '--max-runs-per-repo',self.backup_ci_runs.get(),
                          '--keep-ci-runs',self.keep.get(),
                          '--metadata-workers',self.workers.get(),
                          '--recipient',self.recipient.get(),
                          '--json-progress',
                          '--resume' if mode=='resume' else '--restart']
        # The public recipient is enough to encrypt new backups. Never give
        # the CLI a persistent plaintext private identity path.
        if backend=='github':
            args+=['--github-user',chosen]
            if self.ignore_ci_artifact_errors.get():
                args+=['--ignore-ci-artifact-errors']
            if self.cleanup.get():
                if not messagebox.askyesno(
                        'Confirm optional CI cleanup',
                        'Run the optional GitHub Actions artifact cleanup AFTER '
                        'the backup succeeds?\n\nThis may permanently delete '
                        'eligible artifacts from GitHub. Select No to back up '
                        'without cleanup.'):
                    self.cleanup.set(False)
                    self._save_preferences()
                else:
                    args+=['--cleanup-ci-artifacts']
                    if self.dryrun.get():args+=['--cleanup-ci-dry-run']
            if self.skiplogs.get():args+=['--skip-ci-logs']
        elif self.dropbox_path.get():
            args+=['--dropbox-path',self.dropbox_path.get()]
        self.progress.set_running(False)
        self.progress.set_percent(0)
        self._backup_had_omissions=False
        self._log(
            ('RESUME: riprendo il backup precedente, senza modificarne il nome. '
             if mode=='resume' else
             'START: creo un nuovo backup con un nuovo nome/ciclo. ')
            +'Controllo checkpoint e integrità.')
        self.start_button['state']='disabled'
        self.resume_button['state']='disabled'
        self.stop_button['state']='normal'
        self.status.set('Starting encrypted backup…')
        try:
            kw={'creationflags':subprocess.CREATE_NEW_PROCESS_GROUP} if os.name=='nt' else {}
            self.proc=subprocess.Popen(args,stdout=subprocess.PIPE,stderr=subprocess.PIPE,
                                       text=True,bufsize=1,**kw)
        except (OSError,RuntimeError) as exc:
            self.start_button['state']='normal'
            self.resume_button['state']='normal'
            self.stop_button['state']='disabled'
            self.progress.set_running(False)
            messagebox.showerror('Start failed',str(exc))
            return
        self.progress.set_running(True)
        readers=[]
        for kind,stream in [('out',self.proc.stdout),('err',self.proc.stderr)]:
            worker=threading.Thread(target=self._read,args=(kind,stream),daemon=True)
            worker.start()
            readers.append(worker)
        proc=self.proc
        def finish_after_logs():
            code=proc.wait()
            # Deliver all streamed progress and omission warnings before the
            # final UI status. Worker threads exit when both pipes close.
            for worker in readers:
                worker.join(timeout=10)
            self.events.put(('finished',code))
        threading.Thread(target=finish_after_logs,daemon=True).start()

    def _read(self,kind,stream):
        for line in iter(stream.readline,''):self.events.put((kind,line.rstrip()))

    def _stop(self):
        if self.proc and self.proc.poll() is None:
            try:self.proc.send_signal(signal.CTRL_BREAK_EVENT if os.name=='nt' else signal.SIGINT)
            except Exception:self.proc.terminate()
            self.progress.set_running(False)
            self.status.set('Stopping safely; checkpoints remain available…')

    def _poll(self):
        try:
            for _ in range(100):
                kind,payload=self.events.get_nowait()
                if kind=='finished':
                    self.progress.set_running(False)
                    self.start_button['state']='normal'
                    self.resume_button['state']='normal'
                    self.stop_button['state']='disabled'
                    if payload==0 and self._backup_had_omissions:
                        result='Backup concluso CON OMISSIONI: consultare il report delle CI non recuperate.'
                    elif payload==0:
                        result='Backup completato correttamente.'
                    elif payload==2:
                        result='Backup parziale: riprendi con gli stessi parametri; checkpoint conservato.'
                    else:
                        result=f'Backup interrotto (codice {payload}); verifica il log.'
                    self.status.set(result)
                    self._log(result)
                    self._refresh()
                    self.dashboard.refresh(force=True)
                elif kind=='err':
                    try:
                        obj=json.loads(payload)
                        if obj.get('type')=='progress':
                            self.progress.note_activity()
                            self.progress.set_percent(obj.get('percent',0))
                            self.status.set(obj.get('message',''))
                            if obj.get('stage') in (
                                    'resume-start','resume-summary','resume','ci-retry',
                                    'ci-warning','ci-summary','ci-shortfall','ci-ignored',
                                    'complete-with-warnings','incomplete','complete'):
                                if obj.get('stage') in ('ci-ignored','complete-with-warnings'):
                                    self._backup_had_omissions=True
                                self._log(
                                    f"[{obj.get('stage')}] {obj.get('message','')} "
                                    f"({obj.get('percent',0)}%)")
                        else:self._log(payload)
                    except ValueError:self._log(payload)
                elif kind=='out':
                    self.progress.note_activity()
                    self._log(payload)
                elif kind=='github-login':
                    login,force=payload
                    if force or not self.github_user.get().strip():
                        self.github_user.set(login)
                    self.github_status.set('Connected to GitHub as '+login)
                elif kind=='github-error':
                    self.github_status.set('GitHub not connected: '+str(payload))
                elif kind=='operation-result':
                    self._log(payload)
                    if hasattr(self,'restore_summary'):
                        self.restore_summary.set(payload)
                elif kind=='update':
                    self.update_status.set(payload)
                elif kind=='dashboard-metrics':
                    self.dashboard.apply_metrics(payload)
                else:self._log(payload)
        except queue.Empty:pass
        self.after(130,self._poll)

    def _log(self,line):
        self.text['state']='normal';self.text.insert('end',line+'\n');self.text.see('end');self.text['state']='disabled'

    def _refresh(self):
        self.archive_index=catalog(Path(self.root_dir.get()))
        self.archives.delete(0,'end');self.members.delete(0,'end')
        for item in self.archive_index:self.archives.insert('end',f'[{item["kind"]}] {item["relative"]}')

    def _delete_selected_archive(self):
        selection=self.archives.curselection()
        if not selection:
            messagebox.showinfo('Seleziona archivio','Seleziona prima un archivio locale.')
            return
        self._delete_archive_path(self.archive_index[selection[0]]['path'])

    def _delete_archive_path(self,path):
        if self._locked or self._session_secret is None:
            messagebox.showwarning('Vault bloccato','Sblocca il vault prima di eliminare gli archivi.')
            return
        if self.proc is not None and self.proc.poll() is None:
            messagebox.showwarning('Backup in corso','Non è possibile cancellare archivi durante un backup.')
            return
        try:
            root=Path(self.root_dir.get())
            selected=Path(path)
            files=deletion_plan(root,selected)
        except (OSError,ValueError,ArchiveDeletionError) as exc:
            messagebox.showerror('Impossibile eliminare',str(exc))
            return
        warning=(
            'Eliminazione DEFINITIVA LOCALE dell’archivio selezionato:\n\n'
            +str(selected)+'\n\n'
            +f'File interessati: {len(files)} (archivio e metadati associati).\n'
            +'Se è un oggetto Dropbox, eventuali ripristini che lo '
             'richiedono non saranno più possibili.\n'
            +'Nessun file su GitHub o Dropbox sarà cancellato.\n\n'
            +'Continuare?'
        )
        if not messagebox.askyesno('Conferma eliminazione definitiva',warning):
            return
        response=simpledialog.askstring(
            'Conferma nome esatto',
            'Per eliminare definitivamente questo archivio, scrivi il '
            'suo nome ESATTO (inclusa .tar.gz.age):\n'+selected.name,
            parent=self)
        if response is None:
            return
        try:
            removed=delete_local_archive(root,selected,confirmation=response,
                                         in_use=bool(self.proc and self.proc.poll() is None))
            self._log('ARCHIVIO ELIMINATO LOCAMENTE: '+str(selected)
                      +f' ({len(removed)} file).')
            self._refresh()
            self._refresh_restore()
            self.dashboard.refresh(force=True)
        except (OSError,ValueError,ArchiveDeletionError) as exc:
            messagebox.showerror('Eliminazione non completata',str(exc))
            self._log('ERRORE eliminazione archivio: '+str(exc))

    def _selected(self):
        sel=self.archives.curselection()
        if not sel:messagebox.showinfo('Select archive','Choose an archive first.');return None
        if self._session_secret is None:
            messagebox.showwarning('Key required','Create or unlock your password-protected age identity.')
            return None
        return Path(self.archive_index[sel[0]]['path'])

    def _background(self,fn):
        def work():
            try:self.events.put(('operation-result',str(fn())))
            except Exception as exc:self.events.put(('operation-result','ERROR: '+str(exc)))
        threading.Thread(target=work,daemon=True).start()

    def _view(self):
        a=self._selected()
        if a:
            try:
                with self._identity_file() as identity:
                    values=list_members(a,identity)
                self.members.delete(0,'end')
                for item in values:self.members.insert('end',item['path'])
                self.selected_archive=a
            except Exception as exc:messagebox.showerror('Archive error',str(exc))

    def _show_member(self):
        if not self.selected_archive or not self.members.curselection():
            messagebox.showinfo('Select file','Select an archive member first');return
        name=self.members.get(self.members.curselection()[0])
        try:
            with self._identity_file() as identity:
                content=read_member(self.selected_archive,identity,name)
        except Exception as exc:messagebox.showerror('Archive error',str(exc));return
        view=tk.Toplevel(self);view.title(name);view.geometry('900x600')
        body=tk.Text(view,wrap='none');body.pack(fill='both',expand=True)
        body.insert('1.0',content);body['state']='disabled'

    def _extract(self):
        a=self._selected()
        if a:
            dst=filedialog.askdirectory(title='Extract into private directory')
            if dst:
                def work():
                    with self._identity_file() as identity:
                        return extract_archive(a,identity,Path(dst))
                self._background(work)

    def _checkout(self):
        a=self._selected()
        if a:
            dst=filedialog.asksaveasfilename(title='Create new Git checkout directory')
            if dst:
                def work():
                    with self._identity_file() as identity:
                        return checkout_git(a,identity,Path(dst))
                self._background(work)

    def _mount(self):
        a=self._selected()
        if not a:return
        point=simpledialog.askstring('Mount archive','Select mountpoint (Windows: unused drive letter such as X:)',parent=self)
        if point:
            def work():
                with self._identity_file() as identity:
                    return mount_archive(a,identity,point,Path(self.root_dir.get())/'.state')
            self._background(work)

    def _check_updates(self):
        def work():
            try:
                from updater import update_info
                info=update_info(self.update_channel.get())
                msg=f"{self.update_channel.get()}: {info['tag']}; " + (f"{info['asset']['name']} available" if info['asset'] else 'no package yet')
            except Exception as exc:msg='Update check unavailable: '+str(exc)
            self.events.put(('update',msg))
        threading.Thread(target=work,daemon=True).start()

    def _periodic_update(self):
        if self.autocheck.get():self._check_updates()
        self.after(self.cfg['update_interval_hours']*3600*1000,self._periodic_update)

    def _download_update(self):
        target=filedialog.askdirectory(title='Download verified installer')
        if not target:return
        def work():
            try:
                from updater import update_info,verified_asset_from_release,download_asset
                asset,digest=verified_asset_from_release(update_info(self.update_channel.get()))
                path=download_asset(asset,Path(target),digest)
                msg=f'Installer verified at {path}. Review platform signature before opening.'
            except Exception as exc:msg='Update download failed: '+str(exc)
            self.events.put(('update',msg))
        threading.Thread(target=work,daemon=True).start()


def run_gui():
    app=BackupGUI();app.mainloop();return 0

if __name__=='__main__':run_gui()
