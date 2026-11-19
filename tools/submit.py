#!/usr/bin/env python3
"""Compile the packaged `.hnmod` on a Starling build server and download
the built library.

Usage: python3 tools/submit.py --server URL [--version V]

The server address is not stored in this repo: pass --server, or set
HORIZON_BUILD_SERVER. The server speaks: POST /v1/build?version=V with
each mod as its length (4 bytes, big-endian) and its file;
GET /v1/job/ID for the state; GET /v1/job/ID/lib for the library. Prints
the job id and the built library's size and sha256, and saves it to target/.
"""
import hashlib
import json
import os
import struct
import sys
import time
import urllib.request
import urllib.error
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_VERSION = "0.8.0"


def get(server, path):
    with urllib.request.urlopen(server + path, timeout=30) as r:
        return json.loads(r.read())


def main():
    args = sys.argv[1:]
    server = os.environ.get("HORIZON_BUILD_SERVER")
    version = DEFAULT_VERSION
    while args:
        if args[0] == "--server":
            server, args = args[1], args[2:]
        elif args[0] == "--version":
            version, args = args[1], args[2:]
        else:
            raise SystemExit(f"unknown argument: {args[0]}")
    if not server:
        raise SystemExit("no build server: pass --server URL or set HORIZON_BUILD_SERVER")
    hnmod = sorted((ROOT / "target").glob("locator-heads-*.hnmod"))
    if not hnmod:
        raise SystemExit("no packaged .hnmod in target/: run tools/package.py first")
    data = hnmod[-1].read_bytes()
    print(f"server: {server}  version: {version}")
    print(f"mod: {hnmod[-1].name} ({len(data)} bytes, sha256 {hashlib.sha256(data).hexdigest()})")
    try:
        print("busy:", get(server, "/v1/busy"))
    except Exception as e:
        raise SystemExit(f"build server did not answer /v1/busy: {e}")
    body = struct.pack(">I", len(data)) + data
    req = urllib.request.Request(
        f"{server}/v1/build?version={version}", data=body, method="POST",
        headers={"Content-Type": "application/octet-stream"},
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            job = json.loads(r.read())["job"]
    except urllib.error.HTTPError as e:
        raise SystemExit(f"submit refused ({e.code}): {e.read().decode(errors='replace')}")
    print(f"job: {job}")
    deadline = time.time() + 15 * 60
    while True:
        status = get(server, f"/v1/job/{job}")
        state = status.get("state")
        print(f"state: {state}  {status}")
        if state == "done":
            break
        if state == "failed":
            raise SystemExit(f"build failed: {status.get('error')}")
        if time.time() > deadline:
            raise SystemExit("timed out after 15 minutes; the job may still be building")
        time.sleep(15)
    out = ROOT / "target" / "libhorizon_android.so"
    req = urllib.request.Request(f"{server}/v1/job/{job}/lib")
    with urllib.request.urlopen(req, timeout=300) as r:
        lib = r.read()
    out.write_bytes(lib)
    print(json.dumps({
        "job": job,
        "lib": str(out),
        "lib_size": len(lib),
        "lib_sha256": hashlib.sha256(lib).hexdigest(),
        "server_sha256": status.get("sha256"),
        "server_size": status.get("size"),
    }, indent=2))


if __name__ == "__main__":
    main()
