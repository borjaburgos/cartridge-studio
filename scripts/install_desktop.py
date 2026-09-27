#!/usr/bin/env python3
"""Install a versioned native package, preserving this checkout's backup library."""
import os
from pathlib import Path
import subprocess
import sys
root = Path(__file__).resolve().parents[1]
installer = root/'tmp/dist/cartridge-studio/install.sh'
if not installer.is_file():
    raise SystemExit('Build first: python3 scripts/build_desktop.py')
env = os.environ.copy()
env['CARTRIDGE_STUDIO_DATA_DIR'] = str(root/'tmp')
raise SystemExit(subprocess.call([str(installer), *sys.argv[1:]], env=env))
