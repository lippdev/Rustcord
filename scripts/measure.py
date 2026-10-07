#!/usr/bin/env python3
"""Linux graphical smoke + idle RSS/CPU. Run from repo root with DISPLAY set."""
import json
import os
import pathlib
import re
import subprocess
import time

binary = pathlib.Path("target/release/rustcord")
artifact = pathlib.Path("artifacts")
artifact.mkdir(exist_ok=True)
smoke = subprocess.run([str(binary), "--smoke-test"], capture_output=True, text=True, timeout=15, check=True)
print(smoke.stdout.strip())
match = re.search(r"startup_ms=([0-9.]+) rss_bytes=Some\(([0-9]+)\) ui_cpu_ms=([0-9.]+)", smoke.stdout)
result = {"binary_bytes": binary.stat().st_size, "environment": "Linux; display/renderer supplied by caller"}
if match:
    result.update(startup_ms=float(match[1]), smoke_rss_bytes=int(match[2]), smoke_ui_cpu_ms=float(match[3]))
p = subprocess.Popen([str(binary)], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
try:
    time.sleep(1.5)
    def ticks():
        fields = pathlib.Path(f"/proc/{p.pid}/stat").read_text().split(")", 1)[1].split()
        return int(fields[11]) + int(fields[12])
    before = ticks()
    started = time.monotonic()
    time.sleep(3)
    elapsed = time.monotonic() - started
    result["idle_cpu_percent_one_core"] = (ticks() - before) / os.sysconf("SC_CLK_TCK") / elapsed * 100
    status = pathlib.Path(f"/proc/{p.pid}/status").read_text()
    result["idle_rss_bytes"] = int(re.search(r"VmRSS:\s+(\d+)", status)[1]) * 1024
finally:
    p.terminate()
    _, stderr = p.communicate(timeout=5)
    if stderr: print(stderr)
artifact.joinpath("metrics.json").write_text(json.dumps(result, indent=2)+"\n")
print(json.dumps(result, indent=2))
