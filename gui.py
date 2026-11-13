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
from version import VERSION
from ui_widgets import PremiumButton, GradientProgress
from key_management import private_key_path
from secure_vault import (vault_path,load_vault,unlock,create_identity,migrate_legacy,
                          export_recovery_copy,change_password,import_recovery_copy,
                          temporary_identity,VaultError)
import biometric
from restore_ops import (RestoreError,restore_provider,validate_plugin,
                         restore_local,restore_github_new_repository,
                         restore_dropbox_new_file)
from backup_paths import prepare_destination
from settings import settings_path
from sidebar_nav import SIDEBAR_BG, SIDEBAR_SEPARATOR, SIDEBAR_HEADING, NAV_ITEMS, SidebarNavItem

ROOT=Path(__file__).resolve().parent

def asset(name):
    if getattr(sys,'_MEIPASS',None):return Path(sys._MEIPASS)/'assets'/name
    return ROOT/'assets'/name

class BackupGUI(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title('Multi Backup  ·  '+VERSION)
        self.geometry('1160x850');self.minsize(960,710)
        self.configure(background='#eaf0f7')
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
        self.restart=tk.BooleanVar(value=False);self.skiplogs=tk.BooleanVar(value=False)
        self.update_channel=tk.StringVar(value=self.cfg['update_channel'])
        self.autocheck=tk.BooleanVar(value=self.cfg['auto_update_check'])
        self.update_status=tk.StringVar(value='Not checked yet')
        self._make_styles();self._make_shell()
        self.protocol('WM_DELETE_WINDOW',self._quit_securely)
        self.bind('<Control-b>',lambda _event:self._toggle_sidebar())
        for index,(name,_) in enumerate(NAV_ITEMS,1):
            self.bind('<Control-Key-'+str(index)+'>',
                      lambda _event,n=name:self._show(n))
        self.bind_all('<MouseWheel>',self._scroll_pages)
        self.bind_all('<Button-4>',self._scroll_pages)
        self.bind_all('<Button-5>',self._scroll_pages)
        self.after(120,self._poll)
        # No network requests, page actions or background jobs before key unlock.
        self.after(35,self._startup_security)

    def _make_styles(self):
        style=ttk.Style(self)
        try:style.theme_use('clam')
        except tk.TclError:pass
        style.configure('TFrame',background='#f5f8fc');style.configure('TLabel',background='#f5f8fc',foreground='#24334a',font=('Helvetica',11))
        style.configure('TButton',padding=(12,9),font=('Helvetica',10,'bold'))
        style.configure('Accent.TButton',background='#1267c9',foreground='white',borderwidth=0)
        style.map('Accent.TButton',background=[('active','#0d54a9'),('disabled','#aebdcc')])
        style.configure('TEntry',padding=7);style.configure('TCombobox',padding=6)
        style.configure('Title.TLabel',font=('Helvetica',18,'bold'),foreground='#13223e')
        style.configure('Sub.TLabel',font=('Helvetica',10),foreground='#52647c')
        style.configure('TLabelframe',background='#f5f8fc',borderwidth=1)
        style.configure('TLabelframe.Label',font=('Helvetica',11,'bold'),foreground='#19345a')
        style.configure('Horizontal.TProgressbar',troughcolor='#dbe6f3',background='#1267c9',thickness=16)

    def _make_shell(self):
        # Canvas navigation ensures explicitly readable text on macOS Aqua:
        # native Tk buttons can ignore fg/activeforeground in dark appearance.
        self.sidebar=tk.Frame(self,bg=SIDEBAR_BG,width=252)
        self.sidebar.pack(side='left',fill='y')
        self.sidebar.pack_propagate(False)
        brand=tk.Frame(self.sidebar,bg=SIDEBAR_BG)
        brand.pack(fill='x',padx=20,pady=(24,16))
        try:
            from PIL import Image,ImageTk
            icon=Image.open(asset('multi-backup.png')).resize((54,54),Image.Resampling.LANCZOS)
            self.icon_image=ImageTk.PhotoImage(icon)
            tk.Label(brand,image=self.icon_image,bg=SIDEBAR_BG,bd=0).pack(side='left',padx=(0,10))
        except Exception:
            pass
        brand_copy=tk.Frame(brand,bg=SIDEBAR_BG)
        brand_copy.pack(side='left',fill='x',expand=True)
        tk.Label(brand_copy,text='MULTI BACKUP',font=('Helvetica Neue',14,'bold'),
                 fg='#FFFFFF',bg=SIDEBAR_BG,anchor='w').pack(fill='x')
        tk.Label(brand_copy,text='YOUR PRIVATE VAULT',font=('Helvetica Neue',9),
                 fg='#C0D4ED',bg=SIDEBAR_BG,anchor='w').pack(fill='x',pady=(5,0))
        tk.Frame(self.sidebar,height=1,bg=SIDEBAR_SEPARATOR).pack(fill='x',padx=18,pady=(0,12))

        self.nav={}
        for name,icon in NAV_ITEMS:
            if name in ('Dashboard','Encryption'):
                section='WORKSPACE' if name=='Dashboard' else 'PREFERENCES'
                tk.Label(self.sidebar,text=section,font=('Helvetica Neue',9,'bold'),
                         fg=SIDEBAR_HEADING,bg=SIDEBAR_BG,
                         anchor='w').pack(fill='x',padx=25,
                                          pady=(10 if name=='Dashboard' else 19,6))
            btn=SidebarNavItem(self.sidebar,name,icon,lambda n=name:self._show(n))
            btn.pack(fill='x',padx=13,pady=2)
            self.nav[name]=btn
        foot=tk.Frame(self.sidebar,bg=SIDEBAR_BG)
        foot.pack(side='bottom',fill='x',padx=18,pady=(6,17))
        tk.Frame(foot,height=1,bg=SIDEBAR_SEPARATOR).pack(fill='x',pady=(0,13))
        tk.Label(foot,text=VERSION+'   ·   EUPL-1.2',
                 font=('Helvetica Neue',9),fg='#B4CAE4',
                 bg=SIDEBAR_BG).pack(anchor='w')
        self.content=tk.Frame(self,bg='#f5f8fc');self.content.pack(fill='both',expand=True)
        toolbar=ttk.Frame(self.content)
        toolbar.pack(fill='x',padx=(16,18),pady=(7,5))
        self.menu_button=PremiumButton(toolbar,text='Hide navigation',
                                       command=self._toggle_sidebar)
        self.menu_button.pack(side='left')
        ttk.Label(toolbar,text='PRIVATE BACKUP WORKSPACE',
                  style='Sub.TLabel').pack(side='right')
        self.hero=tk.Canvas(self.content,height=185,bg='#183855',highlightthickness=0)
        self.hero.pack(fill='x');self.hero.bind('<Configure>',self._draw_hero)
        body=tk.Frame(self.content,bg='#f5f8fc')
        body.pack(fill='both',expand=True)
        self.page_canvas=tk.Canvas(body,bg='#f5f8fc',highlightthickness=0,borderwidth=0)
        page_scroll=ttk.Scrollbar(body,orient='vertical',command=self.page_canvas.yview)
        self.page_canvas.configure(yscrollcommand=page_scroll.set)
        page_scroll.pack(side='right',fill='y')
        self.page_canvas.pack(side='left',fill='both',expand=True)
        self.page_host=ttk.Frame(self.page_canvas)
        self.page_window=self.page_canvas.create_window((0,0),window=self.page_host,anchor='nw')
        self.page_host.bind('<Configure>',lambda _ev:self._sync_page_scroll())
        self.page_canvas.bind('<Configure>',self._resize_page_host)
        self.pages={}
        for name in self.nav:
            page=ttk.Frame(self.page_host)
            self.pages[name]=page
        self._build_dashboard(self.pages['Dashboard'])
        self._build_backup(self.pages['Backup'])
        self._build_archives(self.pages['Archives'])
        self._build_restore(self.pages['Restore'])
        self._build_plugins(self.pages['Plugins'])
        self._build_encryption(self.pages['Encryption'])
        self._build_updates(self.pages['Updates'])
        self._build_settings(self.pages['Settings'])

    def _toggle_sidebar(self):
        if self._locked:return
        self.sidebar_visible=not self.sidebar_visible
        if self.sidebar_visible:
            self.sidebar.pack(side='left',fill='y',before=self.content)
        else:
            self.sidebar.pack_forget()
        self.menu_button.set_text('Hide navigation' if self.sidebar_visible
                                  else 'Show navigation')

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
                messagebox.showerror('Access denied','Too many incorrect password attempts.')
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

    def _prepare_identity(self):
        if self._session_secret is not None:
            return False
        if vault_path().exists():
            raise VaultError('Encryption key is locked. Restart and unlock the application.')
        if private_key_path().exists():
            raise VaultError('A previous key must be protected on application startup.')
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
            from PIL import Image,ImageTk,ImageEnhance
            img=Image.open(asset('hero.jpg')).convert('RGB').resize((w,h),Image.Resampling.LANCZOS)
            img=ImageEnhance.Brightness(img).enhance(.69)
            self.hero_image=ImageTk.PhotoImage(img)
            canvas.create_image(0,0,anchor='nw',image=self.hero_image)
        except Exception: pass
        canvas.create_text(35,57,text='Multi Backup',font=('Helvetica',28,'bold'),fill='white',anchor='w')
        canvas.create_text(35,92,text='Your data, protected across platforms',font=('Helvetica',13),fill='#eaf5ff',anchor='w')
        canvas.create_text(35,133,text='Encrypted backups   /   GitHub + Dropbox plugins   /   Resumable snapshots',
                           font=('Helvetica',10),fill='#dbeeff',anchor='w')

    def _show(self,name):
        if self._locked or name not in self.pages:
            return
        for key,page in self.pages.items():
            page.pack_forget()
            self.nav[key].set_active(key==name)
        self.pages[name].pack(fill='both',expand=True,padx=24,pady=18)
        self.page_canvas.yview_moveto(0)
        self._sync_page_scroll()
        if name=='Archives':self._refresh()
        if name=='Restore':self._refresh_restore()

    def _heading(self,parent,title,sub=''):
        ttk.Label(parent,text=title,style='Title.TLabel').pack(anchor='w',pady=(0,5))
        if sub:ttk.Label(parent,text=sub,style='Sub.TLabel').pack(anchor='w',pady=(0,17))

    def _build_dashboard(self,page):
        self._heading(page,'Overview','Your secure multi-backend backup workspace')
        cards=ttk.Frame(page);cards.pack(fill='x',pady=5)
        for title,subtitle,symbol in [('BACKENDS','GitHub ready · Dropbox optional','⬡'),
                                      ('ENCRYPTION','age · user-controlled identities','◇'),
                                      ('UPDATES','Stable by default','↻')]:
            frame=tk.Frame(cards,bg='white',highlightbackground='#d9e3f0',highlightthickness=1)
            frame.pack(side='left',fill='both',expand=True,padx=6,ipady=14)
            tk.Label(frame,text=symbol,font=('Helvetica',24),fg='#1267c9',bg='white').pack(anchor='w',padx=15)
            tk.Label(frame,text=title,font=('Helvetica',11,'bold'),fg='#142c4b',bg='white').pack(anchor='w',padx=15)
            tk.Label(frame,text=subtitle,font=('Helvetica',9),fg='#4e6480',bg='white').pack(anchor='w',padx=15)
        ttk.Label(page,text='Start with a provider and choose what to protect.',style='Title.TLabel').pack(anchor='w',pady=(35,14))
        action=ttk.Frame(page);action.pack(anchor='w')
        PremiumButton(action,text='Create backup',style='Accent.TButton',command=lambda:self._show('Backup')).pack(side='left')
        PremiumButton(action,text='Browse archives',command=lambda:self._show('Archives')).pack(side='left',padx=14)
        PremiumButton(action,text='Configure plugins',command=lambda:self._show('Plugins')).pack(side='left')
        ttk.Label(page,text='Latest update status',style='Sub.TLabel').pack(anchor='w',pady=(38,8))
        ttk.Label(page,textvariable=self.update_status,wraplength=650).pack(anchor='w')

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
            ('Start a fresh cycle',self.restart),
            ('Skip CI logs',self.skiplogs),
        ]:
            ttk.Checkbutton(options,text=label,variable=var).pack(anchor='w',pady=2)

        self.commands_frame=ttk.Frame(page)
        self.commands_frame.pack(fill='x',pady=(8,6))
        self.start_button=PremiumButton(self.commands_frame,text='Start / resume backup',
                                        style='Accent.TButton',command=self._start)
        self.start_button.pack(side='left')
        self.stop_button=PremiumButton(self.commands_frame,text='Interrupt safely',
                                       command=self._stop,state='disabled')
        self.stop_button.pack(side='left',padx=12)
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
                        ('Extract',self._extract),('Checkout Git',self._checkout),('Mount read-only',self._mount)]:
            PremiumButton(row,text=name,command=fn).pack(side='left',padx=(0,7))
        self.archives=tk.Listbox(page,height=9,background='white',border=0,selectbackground='#1267c9',font=('Courier',10))
        self.archives.pack(fill='both',expand=True,pady=(12,5))
        ttk.Label(page,text='Encrypted archive members').pack(anchor='w')
        self.members=tk.Listbox(page,height=9,background='white',border=0,selectbackground='#1267c9',font=('Courier',10))
        self.members.pack(fill='both',expand=True,pady=7)

    def _build_plugins(self,page):
        self._heading(page,'Backup engines','Each provider is an independent plugin with its own capabilities.')
        self.plugin_vars={}
        for backend,detail in [('github','Git repositories, releases, CI metadata and artifacts; optional cleanup'),
                                ('dropbox','Incremental file revisions; read-only remote operations; no purge')]:
            frame=tk.Frame(page,bg='white',highlightbackground='#dae4f1',highlightthickness=1)
            frame.pack(fill='x',pady=10,ipady=18)
            tk.Label(frame,text=backend.upper(),font=('Helvetica',14,'bold'),fg='#123459',bg='white').pack(anchor='w',padx=16)
            tk.Label(frame,text=detail,font=('Helvetica',10),fg='#4d6986',bg='white').pack(anchor='w',padx=16)
            var=tk.BooleanVar(value=backend in self.cfg['enabled_backends']);self.plugin_vars[backend]=var
            check=ttk.Checkbutton(frame,text='Enabled',variable=var,
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
        self.key_info=tk.Text(page,height=5,wrap='word',background='#FFFFFF',
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
        self._heading(page,'Native application updates','Discover signed release channels; download only when SHA-256 is available.')
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

    def _start(self):
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
        if not self._save_preferences():return
        try:
            created=self._prepare_identity()
        except (OSError,ValueError,RuntimeError) as exc:
            messagebox.showerror('Backup encryption unavailable',str(exc))
            return
        if created:
            messagebox.showwarning(
                'Protect your recovery key',
                'Your private age key was generated automatically at:\n\n'+
                self.identity.get()+'\n\nKeep a separate secure copy. Without this '
                'file, encrypted backups cannot be restored.')
        args=self._cli()+['backup','--backend',backend,
                          '--output-dir',self.root_dir.get(),
                          '--max-runs-per-repo',self.backup_ci_runs.get(),
                          '--keep-ci-runs',self.keep.get(),
                          '--metadata-workers',self.workers.get(),
                          '--recipient',self.recipient.get(),
                          '--json-progress']
        if self.identity.get():args+=['--identity',self.identity.get()]
        if backend=='github':
            args+=['--github-user',chosen]
            if self.cleanup.get():
                args+=['--cleanup-ci-artifacts']
                if self.dryrun.get():args+=['--cleanup-ci-dry-run']
            if self.skiplogs.get():args+=['--skip-ci-logs']
            if self.restart.get():args+=['--restart']
        elif self.dropbox_path.get():
            args+=['--dropbox-path',self.dropbox_path.get()]
        self.progress.set_percent(0)
        self.start_button['state']='disabled'
        self.stop_button['state']='normal'
        self.status.set('Starting encrypted backup…')
        try:
            kw={'creationflags':subprocess.CREATE_NEW_PROCESS_GROUP} if os.name=='nt' else {}
            self.proc=subprocess.Popen(args,stdout=subprocess.PIPE,stderr=subprocess.PIPE,
                                       text=True,bufsize=1,**kw)
        except (OSError,RuntimeError) as exc:
            self.start_button['state']='normal'
            self.stop_button['state']='disabled'
            messagebox.showerror('Start failed',str(exc))
            return
        for kind,stream in [('out',self.proc.stdout),('err',self.proc.stderr)]:
            threading.Thread(target=self._read,args=(kind,stream),daemon=True).start()
        proc=self.proc
        threading.Thread(target=lambda:self.events.put(('finished',proc.wait())),
                         daemon=True).start()

    def _read(self,kind,stream):
        for line in iter(stream.readline,''):self.events.put((kind,line.rstrip()))

    def _stop(self):
        if self.proc and self.proc.poll() is None:
            try:self.proc.send_signal(signal.CTRL_BREAK_EVENT if os.name=='nt' else signal.SIGINT)
            except Exception:self.proc.terminate()
            self.status.set('Stopping safely; checkpoints remain available…')

    def _poll(self):
        try:
            for _ in range(100):
                kind,payload=self.events.get_nowait()
                if kind=='finished':
                    self.start_button['state']='normal';self.stop_button['state']='disabled'
                    self.status.set(f'Backup finished: exit {payload}')
                    self._refresh()
                elif kind=='err':
                    try:
                        obj=json.loads(payload)
                        if obj.get('type')=='progress':
                            self.progress.set_percent(obj.get('percent',0))
                            self.status.set(obj.get('message',''))
                        else:self._log(payload)
                    except ValueError:self._log(payload)
                elif kind=='github-login':
                    login,force=payload
                    if force or not self.github_user.get().strip():
                        self.github_user.set(login)
                    self.github_status.set('Connected to GitHub as '+login)
                elif kind=='github-error':
                    self.github_status.set('GitHub not connected: '+str(payload))
                elif kind=='update':
                    self.update_status.set(payload)
                else:self._log(payload)
        except queue.Empty:pass
        self.after(130,self._poll)

    def _log(self,line):
        self.text['state']='normal';self.text.insert('end',line+'\n');self.text.see('end');self.text['state']='disabled'

    def _refresh(self):
        self.archive_index=catalog(Path(self.root_dir.get()))
        self.archives.delete(0,'end');self.members.delete(0,'end')
        for item in self.archive_index:self.archives.insert('end',f'[{item["kind"]}] {item["relative"]}')

    def _selected(self):
        sel=self.archives.curselection()
        if not sel:messagebox.showinfo('Select archive','Choose an archive first.');return None
        if not self.identity.get():messagebox.showwarning('Key required','Select the age private identity in the Backup page.');return None
        return Path(self.archive_index[sel[0]]['path'])

    def _background(self,fn):
        def work():
            try:self.events.put(('out',str(fn())))
            except Exception as exc:self.events.put(('out','ERROR: '+str(exc)))
        threading.Thread(target=work,daemon=True).start()

    def _view(self):
        a=self._selected()
        if a:
            try:
                values=list_members(a,Path(self.identity.get()))
                self.members.delete(0,'end')
                for item in values:self.members.insert('end',item['path'])
                self.selected_archive=a
            except Exception as exc:messagebox.showerror('Archive error',str(exc))

    def _show_member(self):
        if not self.selected_archive or not self.members.curselection():
            messagebox.showinfo('Select file','Select an archive member first');return
        name=self.members.get(self.members.curselection()[0])
        try:content=read_member(self.selected_archive,Path(self.identity.get()),name)
        except Exception as exc:messagebox.showerror('Archive error',str(exc));return
        view=tk.Toplevel(self);view.title(name);view.geometry('900x600')
        body=tk.Text(view,wrap='none');body.pack(fill='both',expand=True)
        body.insert('1.0',content);body['state']='disabled'

    def _extract(self):
        a=self._selected()
        if a:
            dst=filedialog.askdirectory(title='Extract into private directory')
            if dst:self._background(lambda:extract_archive(a,Path(self.identity.get()),Path(dst)))

    def _checkout(self):
        a=self._selected()
        if a:
            dst=filedialog.asksaveasfilename(title='Create new Git checkout directory')
            if dst:self._background(lambda:checkout_git(a,Path(self.identity.get()),Path(dst)))

    def _mount(self):
        a=self._selected()
        if not a:return
        point=simpledialog.askstring('Mount archive','Select mountpoint (Windows: unused drive letter such as X:)',parent=self)
        if point:self._background(lambda:mount_archive(a,Path(self.identity.get()),point,Path(self.root_dir.get())/'.state'))

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
