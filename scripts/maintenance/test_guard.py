#!/usr/bin/env python3
"""Exercise stock-service start inhibition without accessing hardware."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    source = Path(__file__).with_name("guard-init.sh")
    with tempfile.TemporaryDirectory(prefix="m64-init-guard-") as temporary:
        root = Path(temporary)
        original = root / "backup/etc/init.d"
        original.mkdir(parents=True)
        calls = root / "calls"
        for service in ["btminer", "system-monitor", "remote-daemon", "api", "keyd"]:
            delegate = original / service
            delegate.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$M64_TEST_CALLS"\n')
            delegate.chmod(0o755)
            for name in [service, "S89" + service, "K90" + service]:
                guard = root / name
                shutil.copy2(source, guard)
                guard.chmod(0o755)
                environment = dict(os.environ, M64_MAINT_BASE=str(root), M64_TEST_CALLS=str(calls))
                for action in ["start", "restart", "reload", "boot", "enable", "start_service"]:
                    result = subprocess.run([str(guard), action], env=environment, capture_output=True)
                    assert result.returncode != 0, (name, action)
                    assert not calls.exists(), (name, action, "original start was reached")
                # Read-only help is delegated to the saved script.
                result = subprocess.run([str(guard), "help"], env=environment, capture_output=True)
                assert result.returncode == 0, result.stderr
                assert calls.read_text() == "help\n"
                calls.unlink()
    print("Service guards: direct/S/K paths block all startup actions; help delegation passes")


if __name__ == "__main__":
    main()
