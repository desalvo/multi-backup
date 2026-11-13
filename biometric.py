"""Optional macOS Touch ID enrolment backed by device-only Keychain access control.

An enrolled biometric unlock credential never gets stored in Python preferences.
The vault password is stored under SecAccessControl.biometryCurrentSet, which
invalidates the item after enrolled fingers change. Password unlock always works.
"""
from __future__ import annotations

import os
import shutil
import subprocess
import sys


class BiometricError(RuntimeError):
    pass


def _helper():
    if sys.platform!='darwin':
        raise BiometricError('Device biometrics are supported only on macOS in this build')
    helper=shutil.which('multi-backup-biometric')
    if not helper:
        raise BiometricError('Secure macOS Touch ID helper is not installed')
    return helper


def _call(command, secret=None):
    tool=_helper()
    result=subprocess.run(
        [tool,command],
        input=secret.encode('utf-8') if secret is not None else None,
        capture_output=True,check=False,timeout=60)
    if result.returncode!=0:
        raise BiometricError(result.stderr.decode('utf-8','replace').strip()[:300] or
                             'Touch ID authentication unavailable')
    return result.stdout


def available():
    try:
        return _call('status').strip()==b'available'
    except (BiometricError,OSError,subprocess.TimeoutExpired):
        return False


def enable(password):
    if not available():
        raise BiometricError('Touch ID is unavailable or has not been enrolled in macOS')
    return _call('enroll',password).strip()==b'enrolled'


def unlock_password():
    return _call('unlock').decode('utf-8')


def disable():
    return _call('disable').strip()==b'disabled'
