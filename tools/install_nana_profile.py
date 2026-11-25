#!/usr/bin/env python3
"""Install a recovered Nana profile beside a KiriKiri installation.

A Nana-protected XP3 stores its filename table and part of its content
transform behind values that the game derives at runtime from a short startup
string. CrySirror reads those values from three companion files placed next to
the game's XP3 archives:

* ``crysirror-nana.local.json`` -- directory, name-state words and content
  filter parameters (detected by ``crates/crysirror-launch/src/detect.rs``).
* ``content-table.bin`` -- the 4,096-byte control table.
* ``<executable stem>.key`` -- the startup string the table is derived from.

This script verifies a bundled profile against that derivation and copies it
into a game directory. It needs no third-party packages.

The bundled ``example`` profile is a self-consistent placeholder: it exercises
verification without carrying any game's recovered values. Profiles recovered
from a real installation live outside this repository.

Examples::

    python tools/install_nana_profile.py --list
    python tools/install_nana_profile.py --profile example --check
    python tools/install_nana_profile.py --profile example --game-dir "D:\\games\\loca-love"
    python tools/install_nana_profile.py --profile example --zip nana-transfer.zip
    python tools/install_nana_profile.py --derive "D:\\game\\localove3.key"
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import struct
import sys
import zipfile
from pathlib import Path

# crates/crysirror-formats-arc/src/xp3/cxdec/engine.rs:17
CONTROL_BLOCK_SIZE = 4096
PROFILE_FILENAME = "crysirror-nana.local.json"
TABLE_FILENAME = "content-table.bin"
MANIFEST_FILENAME = "profile.json"

_M64 = (1 << 64) - 1


def _rotl(value: int, shift: int) -> int:
    return ((value << shift) | (value >> (64 - shift))) & _M64


def siphash24(key: bytes, message: bytes) -> int:
    """SipHash-2-4, matching Rust's ``SipHasher24::new_with_key``."""
    k0, k1 = struct.unpack("<QQ", key)
    v0 = k0 ^ 0x736F6D6570736575
    v1 = k1 ^ 0x646F72616E646F6D
    v2 = k0 ^ 0x6C7967656E657261
    v3 = k1 ^ 0x7465646279746573

    def sip_round() -> None:
        nonlocal v0, v1, v2, v3
        v0 = (v0 + v1) & _M64
        v1 = _rotl(v1, 13) ^ v0
        v0 = _rotl(v0, 32)
        v2 = (v2 + v3) & _M64
        v3 = _rotl(v3, 16) ^ v2
        v0 = (v0 + v3) & _M64
        v3 = _rotl(v3, 21) ^ v0
        v2 = (v2 + v1) & _M64
        v1 = _rotl(v1, 17) ^ v2
        v2 = _rotl(v2, 32)

    total = len(message)
    body = total - total % 8
    for offset in range(0, body, 8):
        word = struct.unpack_from("<Q", message, offset)[0]
        v3 ^= word
        sip_round()
        sip_round()
        v0 ^= word

    tail = (total & 0xFF) << 56
    for index, byte in enumerate(message[body:]):
        tail |= byte << (8 * index)
    v3 ^= tail
    sip_round()
    sip_round()
    v0 ^= tail

    v2 ^= 0xFF
    for _ in range(4):
        sip_round()
    return v0 ^ v1 ^ v2 ^ v3


def read_startup_line(key_path: Path) -> str:
    """Return the first line of a native ``.key`` companion."""
    raw = key_path.read_bytes()
    if raw.startswith(b"\xff\xfe"):
        text = raw[2:].decode("utf-16-le")
    elif raw.startswith(b"\xef\xbb\xbf"):
        text = raw[3:].decode("utf-8")
    elif raw.startswith(b"\xfe\xff"):
        text = raw[2:].decode("utf-16-be")
    else:
        text = raw.decode("utf-8")
    lines = text.splitlines()
    return lines[0] if lines else text


def derive_setup(startup_line: str) -> dict:
    """Mirror ``nana_profile.rs::derive_setup`` for one startup line."""
    message = startup_line.encode("utf-16-le")
    control = hashlib.shake_256(message).digest(CONTROL_BLOCK_SIZE)
    digest = siphash24(control[:16], message)
    low = digest & 0xFFFFFFFF
    high = (digest >> 32) & 0xFFFFFFFF
    mask = (low & 0x1FF) + 0x100
    offset = ((low >> 9) & 0x7F) + ((low >> 16) & 0xFF) + 0x20
    name_state = [
        struct.unpack_from("<I", control, index * 4)[0] ^ (low if index % 2 == 0 else high)
        for index in range(6)
    ]
    return {
        "startup_line": startup_line,
        "control_block": control,
        "mask": mask,
        "offset": offset,
        "name_state": name_state,
    }


def sha256_of(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def profiles_root() -> Path:
    return Path(__file__).resolve().parent.parent / "profiles" / "nana"


def load_profile(name: str) -> tuple[Path, dict]:
    directory = profiles_root() / name
    manifest_path = directory / MANIFEST_FILENAME
    if not manifest_path.is_file():
        raise SystemExit(f"error: no profile manifest at {manifest_path}")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("files"):
        for entry in manifest["files"]:
            path = directory / entry["name"]
            if not path.is_file():
                raise SystemExit(f"error: manifest lists {entry['name']}, which is missing")
    return directory, manifest


def check_bundle(directory: Path, manifest: dict, verbose: bool = True) -> dict:
    """Verify hashes, then re-derive every value from the startup key."""
    failures: list[str] = []
    for entry in manifest["files"]:
        path = directory / entry["name"]
        size = path.stat().st_size
        digest = sha256_of(path)
        if size != entry["size"]:
            failures.append(f"{entry['name']}: size {size} != manifest {entry['size']}")
        if digest != entry["sha256"]:
            failures.append(f"{entry['name']}: sha256 {digest} != manifest {entry['sha256']}")
        elif verbose:
            print(f"  ok   {entry['name']}  {size} B  sha256 {digest[:16]}…")

    key_entry = next((e for e in manifest["files"] if e["role"] == "startup_key"), None)
    if key_entry is None:
        raise SystemExit("error: manifest declares no startup_key file")
    derived = derive_setup(read_startup_line(directory / key_entry["name"]))

    declared_table = (directory / TABLE_FILENAME).read_bytes()
    if declared_table != derived["control_block"]:
        failures.append(
            f"{TABLE_FILENAME}: does not match shake256(startup line) "
            f"({declared_table[:8].hex()} vs {derived['control_block'][:8].hex()})"
        )

    profile = json.loads((directory / PROFILE_FILENAME).read_text(encoding="utf-8"))
    content = profile.get("content", {})
    for label, derived_value, declared_value in (
        ("mask", derived["mask"], content.get("mask")),
        ("offset", derived["offset"], content.get("offset")),
        ("name_state", derived["name_state"], profile.get("name_state")),
    ):
        if derived_value != declared_value:
            failures.append(f"{label}: derived {derived_value} != declared {declared_value}")
        elif verbose:
            print(f"  ok   {label} = {derived_value}")

    if verbose:
        if failures:
            for line in failures:
                print(f"  FAIL {line}")
        else:
            print("  ok   control table, filter parameters and name state all derive from the key")
    return {"derived": derived, "failures": failures}


def destination_names(manifest: dict, executable: Path) -> dict:
    names = {}
    for entry in manifest["files"]:
        if entry["role"] == "startup_key":
            names[entry["name"]] = f"{executable.stem}.key"
        else:
            names[entry["name"]] = entry["name"]
    return names


def cmd_check(directory: Path, manifest: dict) -> int:
    print(f"bundle: {directory}")
    print(f"profile: {manifest.get('name')}  executable: {manifest.get('executable')}")
    result = check_bundle(directory, manifest)
    return 1 if result["failures"] else 0


def cmd_install(directory: Path, manifest: dict, game_dir: Path, force: bool) -> int:
    executable_name = manifest["executable"]
    executable = game_dir / executable_name
    if not executable.is_file():
        raise SystemExit(f"error: {executable} not found; point --game-dir at the installation")

    print(f"bundle: {directory}")
    print(f"target: {game_dir}  (runtime {executable_name})")
    result = check_bundle(directory, manifest)
    if result["failures"]:
        print("refusing to install: the bundle does not verify")
        return 1

    names = destination_names(manifest, executable)
    pending = []
    skipped = 0
    for source_name, target_name in names.items():
        source = directory / source_name
        target = game_dir / target_name
        if target.is_file() and target.read_bytes() != source.read_bytes():
            if not force:
                print(f"  skip {target_name}: already present with different content (use --force)")
                skipped += 1
                continue
            pending.append((source, target, "replaced"))
        elif target.is_file():
            print(f"  same {target_name}: already installed")
            continue
        else:
            pending.append((source, target, "written"))

    for source, target, action in pending:
        shutil.copyfile(source, target)
        print(f"  {action}: {target.name}  {target.stat().st_size} B")

    for target_name in names.values():
        target = game_dir / target_name
        if not target.is_file():
            print(f"  missing after install: {target_name}")
            return 1
    if skipped:
        print(f"incomplete: {skipped} companion(s) kept their existing content; rerun with --force to replace")
        return 2
    print(
        "installed; CrySirror detects the profile as the first KiriKiri candidate "
        f"for {executable_name} and the archive reader validates the key against the control table"
    )
    return 0


def cmd_zip(directory: Path, manifest: dict, archive: Path, executable_stem: str | None) -> int:
    result = check_bundle(directory, manifest, verbose=False)
    if result["failures"]:
        print("refusing to package: the bundle does not verify")
        return 1
    executable = Path(manifest["executable"])
    names = destination_names(manifest, Path(f"{executable_stem}.exe" if executable_stem else executable.name))
    archive.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as handle:
        for source_name, target_name in names.items():
            handle.write(directory / source_name, target_name)
    print(f"wrote {archive} ({archive.stat().st_size} B, {len(names)} entries)")
    for target_name in names.values():
        print(f"  {target_name}")
    return 0


def cmd_derive(key_path: Path) -> int:
    derived = derive_setup(read_startup_line(key_path))
    profile = {
        "name_state": derived["name_state"],
        "content": {
            "mask": derived["mask"],
            "offset": derived["offset"],
            "control_block_file": TABLE_FILENAME,
        },
    }
    print(f"startup line: {derived['startup_line']!r}")
    print(f"control[0:16]: {derived['control_block'][:16].hex()}")
    print(f"mask: {derived['mask']}  offset: {derived['offset']}")
    print(json.dumps(profile, indent=2))
    return 0


def cmd_list() -> int:
    root = profiles_root()
    if not root.is_dir():
        print(f"no bundled profiles at {root}")
        return 1
    for directory in sorted(p for p in root.iterdir() if p.is_dir()):
        manifest_path = directory / MANIFEST_FILENAME
        if not manifest_path.is_file():
            continue
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        total = sum(entry["size"] for entry in manifest.get("files", []))
        print(f"{directory.name:24} {total:6} B  {manifest.get('executable')}  {manifest.get('game')}")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--profile", default="example", help="bundled profile name (default: example)")
    parser.add_argument("--game-dir", type=Path, help="installation to install the profile into")
    parser.add_argument("--check", action="store_true", help="verify the bundle only")
    parser.add_argument("--force", action="store_true", help="replace companion files that differ")
    parser.add_argument("--zip", type=Path, metavar="ARCHIVE", help="write a transferable zip of the companion files")
    parser.add_argument("--stem", help="executable stem used to name the .key inside --zip")
    parser.add_argument("--derive", type=Path, metavar="KEY", help="print the values derived from a .key companion")
    parser.add_argument("--list", action="store_true", help="list bundled profiles")
    args = parser.parse_args(argv)

    if args.list:
        return cmd_list()
    if args.derive:
        return cmd_derive(args.derive)

    directory, manifest = load_profile(args.profile)
    if args.zip:
        return cmd_zip(directory, manifest, args.zip, args.stem)
    if args.game_dir:
        return cmd_install(directory, manifest, args.game_dir, args.force)
    if args.check:
        return cmd_check(directory, manifest)
    parser.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())
