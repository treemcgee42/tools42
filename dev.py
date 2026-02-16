#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python <3.11
    print("error: tomllib not available; use Python 3.11+", file=sys.stderr)
    sys.exit(1)


def eprint(msg: str) -> None:
    print(msg, file=sys.stderr)


def ensure_dir(path: Path) -> None:
    if not path.exists():
        print(f"create dir: {path}")
        path.mkdir(parents=True, exist_ok=True)


def read_toml(path: Path) -> dict:
    try:
        with path.open("rb") as f:
            return tomllib.load(f)
    except FileNotFoundError:
        eprint(f"build spec not found: {path}")
        sys.exit(1)
    except tomllib.TOMLDecodeError as exc:
        eprint(f"invalid build spec: {path}: {exc}")
        sys.exit(1)


def read_cargo_package_name(path: Path) -> str:
    if not path.exists():
        eprint(f"Cargo.toml not found: {path}")
        sys.exit(1)
    try:
        with path.open("rb") as f:
            cargo = tomllib.load(f)
    except tomllib.TOMLDecodeError as exc:
        eprint(f"invalid Cargo.toml: {path}: {exc}")
        sys.exit(1)

    package = cargo.get("package")
    if not isinstance(package, dict):
        eprint(f"invalid Cargo.toml: missing [package] in {path}")
        sys.exit(1)

    name = package.get("name")
    if not isinstance(name, str) or not name:
        eprint(f"invalid Cargo.toml: missing package.name in {path}")
        sys.exit(1)
    return name


def run_command(cmd: list[str], cwd: Path, env: dict[str, str]) -> None:
    print(f"run: {' '.join(cmd)}")
    try:
        subprocess.run(cmd, cwd=str(cwd), env=env, check=True)
    except FileNotFoundError:
        eprint(f"command not found: {cmd[0]}")
        sys.exit(1)
    except subprocess.CalledProcessError as exc:
        sys.exit(exc.returncode)


def build_rust(repo_root: Path, package: str, pkg_dir: Path) -> None:
    bld_root = repo_root / "bld" / package
    bin_root = repo_root / "bin"
    ensure_dir(bld_root)
    ensure_dir(bin_root)

    cargo_toml = pkg_dir / "Cargo.toml"
    bin_name = read_cargo_package_name(cargo_toml)

    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(bld_root)
    env["RUSTFLAGS"] = (
        f'{env.get("RUSTFLAGS", "")} --remap-path-prefix src={pkg_dir / "src"}'
    ).strip()
    cmd = ["cargo", "build", "--manifest-path", str(cargo_toml)]
    run_command(cmd, cwd=repo_root, env=env)

    exe_suffix = ".exe" if os.name == "nt" else ""
    built_bin = bld_root / "debug" / f"{bin_name}{exe_suffix}"
    if not built_bin.exists():
        eprint(f"built binary not found: {built_bin}")
        sys.exit(1)

    dest_bin = bin_root / f"{bin_name}{exe_suffix}"
    print(f"install: {built_bin} -> {dest_bin}")
    shutil.copy2(built_bin, dest_bin)
    if os.name != "nt":
        dest_bin.chmod(0o755)


def test_rust(repo_root: Path, package: str, pkg_dir: Path) -> None:
    bld_root = repo_root / "bld" / package
    ensure_dir(bld_root)

    cargo_toml = pkg_dir / "Cargo.toml"
    if not cargo_toml.exists():
        eprint(f"Cargo.toml not found: {cargo_toml}")
        sys.exit(1)

    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(bld_root)
    env["RUSTFLAGS"] = (
        f'{env.get("RUSTFLAGS", "")} --remap-path-prefix src={pkg_dir / "src"}'
    ).strip()
    cmd = ["cargo", "test", "--manifest-path", str(cargo_toml)]
    run_command(cmd, cwd=repo_root, env=env)


def cmd_build(args: argparse.Namespace) -> None:
    repo_root = Path(__file__).resolve().parent
    pkg_dir = repo_root / "src" / args.package

    if not pkg_dir.exists():
        eprint(f"package dir not found: {pkg_dir}")
        sys.exit(1)

    spec_path = pkg_dir / "tools42_build.toml"
    spec = read_toml(spec_path)

    pkg_type = spec.get("type")
    if pkg_type != "rust":
        eprint(f"unsupported package type: {pkg_type}")
        sys.exit(1)

    build_rust(repo_root, args.package, pkg_dir)


def cmd_test(args: argparse.Namespace) -> None:
    repo_root = Path(__file__).resolve().parent
    pkg_dir = repo_root / "src" / args.package

    if not pkg_dir.exists():
        eprint(f"package dir not found: {pkg_dir}")
        sys.exit(1)

    spec_path = pkg_dir / "tools42_build.toml"
    spec = read_toml(spec_path)

    pkg_type = spec.get("type")
    if pkg_type != "rust":
        eprint(f"unsupported package type: {pkg_type}")
        sys.exit(1)

    test_rust(repo_root, args.package, pkg_dir)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(prog="dev.py")
    sub = parser.add_subparsers(dest="command", required=True)

    build = sub.add_parser("build", help="build a package")
    build.add_argument("package", help="package name (subdir under src/)")
    build.set_defaults(func=cmd_build)

    test = sub.add_parser("test", help="test a package")
    test.add_argument("package", help="package name (subdir under src/)")
    test.set_defaults(func=cmd_test)

    args = parser.parse_args(argv)
    args.func(args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
