#!/usr/bin/env python3
"""Standalone diagnostics for Sonarpad's bundled mpv.app on macOS.

The script deliberately never fails just because mpv fails: its job is to collect
as much evidence as possible and write one deterministic text report.
"""

from __future__ import annotations

import argparse
import datetime as dt
import glob
import hashlib
import os
import pathlib
import plistlib
import re
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.parse
import zipfile
from dataclasses import dataclass
from typing import Iterable, Sequence


MAX_COMMAND_OUTPUT = 120_000


class Report:
    def __init__(self) -> None:
        self.lines: list[str] = []

    def add(self, text: str = "") -> None:
        self.lines.extend(str(text).splitlines() or [""])

    def section(self, title: str) -> None:
        self.add("")
        self.add("=" * 80)
        self.add(title)
        self.add("=" * 80)

    def command(self, argv: Sequence[str], result: "CommandResult") -> None:
        self.add(f"$ {' '.join(argv)}")
        self.add(f"exit={result.returncode} timed_out={str(result.timed_out).lower()}")
        if result.output:
            self.add(result.output)

    def write(self, path: pathlib.Path) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("\n".join(self.lines) + "\n", encoding="utf-8")


@dataclass
class CommandResult:
    returncode: int
    output: str
    timed_out: bool = False

    @property
    def ok(self) -> bool:
        return self.returncode == 0 and not self.timed_out


@dataclass
class ProbeResult:
    name: str
    passed: bool
    detail: str
    returncode: int | None = None


def clipped(text: str, limit: int = MAX_COMMAND_OUTPUT) -> str:
    if len(text) <= limit:
        return text.rstrip()
    head = text[: limit // 2]
    tail = text[-limit // 2 :]
    return (head + f"\n... <truncated {len(text) - limit} chars> ...\n" + tail).rstrip()


def run(argv: Sequence[str], timeout: float = 30.0, cwd: pathlib.Path | None = None) -> CommandResult:
    try:
        cp = subprocess.run(
            list(argv),
            cwd=str(cwd) if cwd else None,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            errors="replace",
            timeout=timeout,
            check=False,
        )
        return CommandResult(cp.returncode, clipped(cp.stdout or ""))
    except subprocess.TimeoutExpired as exc:
        out = exc.stdout or ""
        if isinstance(out, bytes):
            out = out.decode("utf-8", "replace")
        return CommandResult(124, clipped(out), timed_out=True)
    except Exception as exc:  # diagnostic code must keep going
        return CommandResult(125, f"{type(exc).__name__}: {exc}")


def sha256_file(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def download(url: str, dest: pathlib.Path, expected_sha: str, report: Report, label: str) -> bool:
    report.section(f"DOWNLOAD: {label}")
    dest.parent.mkdir(parents=True, exist_ok=True)
    result = run(
        [
            "curl",
            "--location",
            "--fail",
            "--show-error",
            "--http1.1",
            "--retry",
            "4",
            "--retry-all-errors",
            "--connect-timeout",
            "30",
            "--max-time",
            "600",
            url,
            "-o",
            str(dest),
        ],
        timeout=660,
    )
    report.command(["curl", url, "-o", str(dest)], result)
    if not result.ok or not dest.exists():
        report.add(f"DOWNLOAD_RESULT={label}:FAIL")
        return False

    actual = sha256_file(dest)
    report.add(f"sha256={actual}")
    if expected_sha:
        if actual.lower() != expected_sha.lower():
            report.add(f"EXPECTED_SHA256={expected_sha}")
            report.add(f"DOWNLOAD_RESULT={label}:FAIL_SHA256")
            return False
        report.add("SHA256_RESULT=PASS")
    else:
        report.add("SHA256_RESULT=NOT_PINNED")
    report.add(f"DOWNLOAD_RESULT={label}:PASS")
    return True


def extract_archive(archive: pathlib.Path, out_dir: pathlib.Path, report: Report, label: str) -> pathlib.Path | None:
    report.section(f"EXTRACT: {label}")
    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True)
    try:
        name = archive.name.lower()
        if name.endswith(".zip"):
            with zipfile.ZipFile(archive) as zf:
                zf.extractall(out_dir)
            nested = next(out_dir.rglob("mpv.tar.gz"), None)
            if nested:
                with tarfile.open(nested, "r:gz") as tf:
                    tf.extractall(out_dir)
        elif name.endswith((".tar.gz", ".tgz")):
            with tarfile.open(archive, "r:gz") as tf:
                tf.extractall(out_dir)
        else:
            report.add(f"Unsupported archive format: {archive}")
            return None
    except Exception as exc:
        report.add(f"EXTRACT_ERROR={type(exc).__name__}: {exc}")
        return None

    apps = sorted(p for p in out_dir.rglob("mpv.app") if p.is_dir())
    if not apps:
        report.add("EXTRACT_RESULT=FAIL_NO_MPV_APP")
        return None
    app = apps[0]
    binary = app / "Contents" / "MacOS" / "mpv"
    report.add(f"mpv_app={app}")
    report.add(f"mpv_binary={binary}")
    report.add(f"EXTRACT_RESULT={'PASS' if binary.exists() else 'FAIL_NO_BINARY'}")
    return app if binary.exists() else None


def version_tuple(value: str) -> tuple[int, ...]:
    return tuple(int(p) for p in re.findall(r"\d+", value))


def parse_minos(text: str) -> str | None:
    match = re.search(r"\bminos\s+([0-9]+(?:\.[0-9]+)*)", text)
    if match:
        return match.group(1)
    # Older LC_VERSION_MIN_MACOSX output from vtool.
    match = re.search(r"\bversion\s+([0-9]+(?:\.[0-9]+)*)", text)
    return match.group(1) if match else None


def macho_files(app: pathlib.Path) -> list[pathlib.Path]:
    items: list[pathlib.Path] = []
    for path in app.rglob("*"):
        if not path.is_file() or path.is_symlink():
            continue
        info = run(["file", "-b", str(path)], timeout=10)
        if "Mach-O" in info.output:
            items.append(path)
    return sorted(items)


def analyze_bundle(app: pathlib.Path, label: str, compat_target: str, report: Report) -> dict[str, object]:
    report.section(f"BUNDLE ANALYSIS: {label}")
    binary = app / "Contents" / "MacOS" / "mpv"
    state: dict[str, object] = {}

    for argv in (
        ["file", str(binary)],
        ["lipo", "-archs", str(binary)],
        ["vtool", "-show-build", str(binary)],
        ["otool", "-L", str(binary)],
        ["codesign", "--verify", "--deep", "--strict", "--verbose=4", str(app)],
        ["codesign", "-dv", "--verbose=4", str(app)],
        ["spctl", "--assess", "--type", "execute", "-vvv", str(app)],
        ["xattr", "-lr", str(app)],
        ["plutil", "-p", str(app / "Contents" / "Info.plist")],
    ):
        result = run(argv, timeout=45)
        report.command(argv, result)
        if argv[0] == "codesign" and "--verify" in argv:
            state["codesign_ok"] = result.ok
        elif argv[0] == "spctl":
            state["spctl_ok"] = result.ok

    report.add("\n-- Symlinks --")
    symlinks = []
    for path in sorted(app.rglob("*")):
        if path.is_symlink():
            try:
                target = os.readlink(path)
                resolved = path.resolve(strict=False)
                exists = resolved.exists()
                symlinks.append((path, target, exists))
                report.add(f"{path.relative_to(app)} -> {target} resolved_exists={exists}")
            except OSError as exc:
                report.add(f"{path.relative_to(app)} -> ERROR {exc}")
    if not symlinks:
        report.add("<none>")
    state["broken_symlink"] = any(not x[2] for x in symlinks)

    report.add("\n-- Mach-O deployment targets --")
    machos = macho_files(app)
    minos_rows: list[tuple[pathlib.Path, str]] = []
    for path in machos:
        result = run(["vtool", "-show-build", str(path)], timeout=15)
        minos = parse_minos(result.output)
        arch = run(["lipo", "-archs", str(path)], timeout=10).output.replace("\n", " ").strip()
        rel = path.relative_to(app)
        report.add(f"{rel} | arch={arch or '?'} | minos={minos or '?'}")
        if minos:
            minos_rows.append((path, minos))

    target_tuple = version_tuple(compat_target)
    offenders = [(p, v) for p, v in minos_rows if version_tuple(v) > target_tuple]
    if offenders:
        report.add(f"COMPATIBILITY_TARGET={compat_target}:FAIL")
        for p, v in offenders:
            report.add(f"INCOMPATIBLE_MINOS {p.relative_to(app)} minos={v} > {compat_target}")
        state["compat_ok"] = False
    elif minos_rows:
        report.add(f"COMPATIBILITY_TARGET={compat_target}:PASS")
        state["compat_ok"] = True
    else:
        report.add(f"COMPATIBILITY_TARGET={compat_target}:UNKNOWN_NO_MINOS")
        state["compat_ok"] = None

    if minos_rows:
        state["max_minos"] = max((v for _, v in minos_rows), key=version_tuple)
    else:
        state["max_minos"] = None
    state["macho_count"] = len(machos)
    return state


def create_sonarpad_packaging_simulation(source_app: pathlib.Path, work: pathlib.Path, report: Report) -> pathlib.Path | None:
    report.section("SONARPAD PACKAGING SIMULATION")
    parent = work / "SonarpadDiagnostic.app"
    if parent.exists():
        shutil.rmtree(parent)
    nested = parent / "Contents" / "Resources" / "mpv.app"
    nested.parent.mkdir(parents=True, exist_ok=True)
    try:
        shutil.copytree(source_app, nested, symlinks=True)
        info = nested / "Contents" / "Info.plist"
        with info.open("rb") as f:
            plist = plistlib.load(f)
        plist["CFBundleDevelopmentRegion"] = "it"
        plist["CFBundleLocalizations"] = ["en", "it", "fr", "es", "pt", "cs", "pl"]
        plist["CFBundleAllowMixedLocalizations"] = True
        with info.open("wb") as f:
            plistlib.dump(plist, f, sort_keys=False)

        parent_info = {
            "CFBundleExecutable": "SonarpadDiagnostic",
            "CFBundleIdentifier": "com.sonarpad.mpv-diagnostic",
            "CFBundleName": "SonarpadDiagnostic",
            "CFBundlePackageType": "APPL",
            "CFBundleVersion": "1",
            "CFBundleShortVersionString": "1.0",
        }
        (parent / "Contents" / "MacOS").mkdir(parents=True, exist_ok=True)
        with (parent / "Contents" / "Info.plist").open("wb") as f:
            plistlib.dump(parent_info, f)
        launcher = parent / "Contents" / "MacOS" / "SonarpadDiagnostic"
        launcher.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        launcher.chmod(0o755)

        result = run(["codesign", "--force", "--deep", "--sign", "-", str(parent)], timeout=60)
        report.command(["codesign", "--force", "--deep", "--sign", "-", str(parent)], result)
        verify = run(["codesign", "--verify", "--deep", "--strict", "--verbose=4", str(parent)], timeout=60)
        report.command(["codesign", "--verify", "--deep", "--strict", "--verbose=4", str(parent)], verify)
        if not result.ok:
            report.add("PACKAGING_SIMULATION=FAIL_SIGN")
            return None
        report.add("PACKAGING_SIMULATION=PASS")
        return nested
    except Exception as exc:
        report.add(f"PACKAGING_SIMULATION=ERROR {type(exc).__name__}: {exc}")
        return None


def format_returncode(rc: int | None) -> str:
    if rc is None:
        return "none"
    if rc < 0:
        try:
            return f"{rc} ({signal.Signals(-rc).name})"
        except ValueError:
            return str(rc)
    return str(rc)


def probe_simple(name: str, argv: Sequence[str], report: Report, timeout: float = 30) -> ProbeResult:
    result = run(argv, timeout=timeout)
    report.add(f"\n--- PROBE {name} ---")
    report.command(argv, result)
    passed = result.ok
    detail = "PASS" if passed else f"FAIL rc={format_returncode(result.returncode)} timeout={result.timed_out}"
    report.add(f"PROBE_RESULT {name}={detail}")
    return ProbeResult(name, passed, detail, result.returncode)


def probe_survival(name: str, argv: Sequence[str], report: Report, survive_seconds: float = 3.0) -> ProbeResult:
    report.add(f"\n--- PROBE {name} (must survive {survive_seconds:.1f}s) ---")
    report.add(f"$ {' '.join(argv)}")
    try:
        proc = subprocess.Popen(
            list(argv),
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            errors="replace",
        )
        deadline = time.monotonic() + survive_seconds
        while time.monotonic() < deadline and proc.poll() is None:
            time.sleep(0.1)
        if proc.poll() is None:
            passed = True
            detail = f"PASS alive_after={survive_seconds:.1f}s"
            proc.terminate()
            try:
                out, _ = proc.communicate(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
                out, _ = proc.communicate(timeout=3)
        else:
            passed = False
            out, _ = proc.communicate(timeout=3)
            detail = f"FAIL exited_early rc={format_returncode(proc.returncode)}"
        if out:
            report.add(clipped(out))
        report.add(f"PROBE_RESULT {name}={detail}")
        return ProbeResult(name, passed, detail, proc.returncode)
    except Exception as exc:
        detail = f"ERROR {type(exc).__name__}: {exc}"
        report.add(f"PROBE_RESULT {name}={detail}")
        return ProbeResult(name, False, detail, None)


def process_pids_for_binary(binary: pathlib.Path) -> set[int]:
    result = run(["ps", "-axo", "pid=,command="], timeout=10)
    pids: set[int] = set()
    needle = str(binary)
    for line in result.output.splitlines():
        if needle not in line:
            continue
        match = re.match(r"\s*(\d+)\s+", line)
        if match:
            pids.add(int(match.group(1)))
    return pids


def terminate_pids(pids: Iterable[int]) -> None:
    for pid in pids:
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    time.sleep(0.5)
    for pid in pids:
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


def probe_launchservices(name: str, app: pathlib.Path, report: Report) -> ProbeResult:
    binary = app / "Contents" / "MacOS" / "mpv"
    before = process_pids_for_binary(binary)
    argv = [
        "/usr/bin/open",
        "-n",
        str(app),
        "--args",
        "--no-config",
        "--idle=yes",
        "--force-window=yes",
        "--no-terminal",
    ]
    result = run(argv, timeout=15)
    report.add(f"\n--- PROBE {name} ---")
    report.command(argv, result)
    time.sleep(2.0)
    after = process_pids_for_binary(binary)
    new_pids = after - before
    passed = result.ok and bool(new_pids)
    detail = f"{'PASS' if passed else 'FAIL'} open_rc={result.returncode} new_mpvs={sorted(new_pids)}"
    report.add(f"PROBE_RESULT {name}={detail}")
    terminate_pids(new_pids)
    return ProbeResult(name, passed, detail, result.returncode)


def make_test_wav(path: pathlib.Path) -> None:
    import wave

    frames = b"\x00\x00" * 16_000
    with wave.open(str(path), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(16_000)
        wav.writeframes(frames)


def runtime_probes(app: pathlib.Path, label: str, report: Report, live_url: str, vod_url: str, network: bool) -> list[ProbeResult]:
    report.section(f"RUNTIME PROBES: {label}")
    binary = app / "Contents" / "MacOS" / "mpv"
    results: list[ProbeResult] = []
    results.append(probe_simple(f"{label}.version", [str(binary), "--version"], report, timeout=20))
    results.append(
        probe_survival(
            f"{label}.headless_idle",
            [str(binary), "--no-config", "--idle=yes", "--force-window=no", "--no-terminal", "--vo=null", "--ao=null"],
            report,
        )
    )
    results.append(
        probe_survival(
            f"{label}.cocoa_force_window",
            [str(binary), "--no-config", "--idle=yes", "--force-window=yes", "--no-terminal"],
            report,
        )
    )
    results.append(probe_launchservices(f"{label}.launchservices_bundle", app, report))

    with tempfile.TemporaryDirectory(prefix="sonarpad-mpv-media-") as td:
        wav = pathlib.Path(td) / "silence.wav"
        make_test_wav(wav)
        results.append(
            probe_simple(
                f"{label}.local_wav",
                [str(binary), "--no-config", "--vo=null", "--ao=null", "--length=0.5", str(wav)],
                report,
                timeout=20,
            )
        )

    if network:
        for suffix, url in (("la7_live", live_url), ("la7_vod", vod_url)):
            if not url:
                continue
            results.append(
                probe_simple(
                    f"{label}.{suffix}",
                    [str(binary), "--no-config", "--vo=null", "--ao=null", "--length=2", url],
                    report,
                    timeout=35,
                )
            )
    else:
        report.add("NETWORK_PROBES=DISABLED")
    return results


def crash_reports_since(started_at: float, report: Report) -> None:
    report.section("CRASH / SECURITY EVIDENCE")
    paths = []
    for pattern in (
        os.path.expanduser("~/Library/Logs/DiagnosticReports/mpv*"),
        os.path.expanduser("~/Library/Logs/DiagnosticReports/*mpv*"),
    ):
        for raw in glob.glob(pattern):
            path = pathlib.Path(raw)
            try:
                if path.is_file() and path.stat().st_mtime >= started_at - 5:
                    paths.append(path)
            except OSError:
                pass
    paths = sorted(set(paths), key=lambda p: p.stat().st_mtime)
    if not paths:
        report.add("CRASH_REPORTS=<none found>")
    for path in paths[-5:]:
        report.add(f"\n--- {path} ---")
        try:
            report.add(clipped(path.read_text(encoding="utf-8", errors="replace"), 80_000))
        except OSError as exc:
            report.add(f"READ_ERROR={exc}")

    predicate = '(process == "amfid") OR (process == "syspolicyd") OR (process == "runningboardd") OR (eventMessage CONTAINS[c] "mpv")'
    argv = ["/usr/bin/log", "show", "--last", "8m", "--style", "compact", "--predicate", predicate]
    result = run(argv, timeout=35)
    report.command(argv, result)


def summarize(
    scenario: str,
    target: str,
    candidate_state: dict[str, object],
    baseline_state: dict[str, object] | None,
    candidate_results: list[ProbeResult],
    packaged_results: list[ProbeResult],
    baseline_results: list[ProbeResult],
    runtime_enabled: bool,
    report: Report,
) -> None:
    report.section("FINAL DIAGNOSTIC SUMMARY")
    report.add(f"SCENARIO={scenario}")
    report.add(f"COMPATIBILITY_TARGET={target}")
    report.add(f"RUNTIME_TESTED={str(runtime_enabled).lower()}")
    report.add(f"CANDIDATE_MAX_MINOS={candidate_state.get('max_minos')}")
    report.add(f"CANDIDATE_COMPAT_OK={candidate_state.get('compat_ok')}")
    report.add(f"CANDIDATE_CODESIGN_OK={candidate_state.get('codesign_ok')}")
    report.add(f"CANDIDATE_SPCTL_OK={candidate_state.get('spctl_ok')}")
    if baseline_state:
        report.add(f"BASELINE_MAX_MINOS={baseline_state.get('max_minos')}")
        report.add(f"BASELINE_COMPAT_OK={baseline_state.get('compat_ok')}")

    for item in candidate_results + packaged_results + baseline_results:
        report.add(f"PROBE_SUMMARY {item.name}={'PASS' if item.passed else 'FAIL'} {item.detail}")

    if not runtime_enabled:
        report.add("CATALINA_RUNTIME_NOTE=GitHub-hosted Actions does not provide a macOS 10.15 runner; this scenario is a static Mach-O/deployment-target/signature compatibility check only.")

    diagnosis: list[str] = []
    if candidate_state.get("compat_ok") is False:
        diagnosis.append(
            f"HIGH_CONFIDENCE: candidate contains Mach-O binaries requiring a macOS newer than {target}; it cannot run on that target regardless of La7/HLS."
        )
    if candidate_state.get("codesign_ok") is False:
        diagnosis.append("HIGH_CONFIDENCE: candidate bundle fails codesign --verify --deep --strict.")

    by_name = {p.name: p for p in candidate_results}
    baseline_by_name = {p.name: p for p in baseline_results}
    packaged_by_name = {p.name: p for p in packaged_results}

    cand_version = by_name.get("candidate.version")
    base_version = baseline_by_name.get("baseline.version")
    if cand_version and not cand_version.passed and base_version and base_version.passed:
        diagnosis.append("HIGH_CONFIDENCE: candidate fails before media playback while the known-good baseline starts on the same runner; problem is in the candidate binary/bundle/runtime compatibility, not La7.")
    cand_headless = by_name.get("candidate.headless_idle")
    cand_gui = by_name.get("candidate.cocoa_force_window")
    base_gui = baseline_by_name.get("baseline.cocoa_force_window")
    if cand_headless and cand_headless.passed and cand_gui and not cand_gui.passed and base_gui and base_gui.passed:
        diagnosis.append("LIKELY: command-line core survives but Cocoa/window initialization fails only in candidate; inspect macOS frontend/Swift/video output changes.")
    raw_ls = by_name.get("candidate.launchservices_bundle")
    packaged_ls = packaged_by_name.get("packaged.launchservices_bundle")
    if raw_ls and raw_ls.passed and packaged_ls and not packaged_ls.passed:
        diagnosis.append("LIKELY: raw mpv.app launches but Sonarpad-style nested/ad-hoc-signed packaging does not; focus on nested code signing, Info.plist mutation or LaunchServices constraints.")

    local_probe = by_name.get("candidate.local_wav")
    live_probe = by_name.get("candidate.la7_live")
    vod_probe = by_name.get("candidate.la7_vod")
    if local_probe and local_probe.passed and any(p and not p.passed for p in (live_probe, vod_probe)):
        diagnosis.append("POSSIBLE: local media works but a La7 network probe fails; inspect HLS/network/FFmpeg path rather than process startup.")

    if not diagnosis:
        diagnosis.append("No single cause was proven automatically. Use the per-command output, crash reports and unified log above to compare candidate vs baseline.")
    for idx, text in enumerate(diagnosis, 1):
        report.add(f"DIAGNOSIS_{idx}={text}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--scenario", required=True)
    parser.add_argument("--candidate-url", required=True)
    parser.add_argument("--candidate-sha", default="")
    parser.add_argument("--baseline-url", default="")
    parser.add_argument("--baseline-sha", default="")
    parser.add_argument("--compat-target", required=True)
    parser.add_argument("--runtime", choices=("yes", "no"), default="yes")
    parser.add_argument("--network", choices=("yes", "no"), default="yes")
    parser.add_argument("--live-url", default="")
    parser.add_argument("--vod-url", default="")
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    report = Report()
    started_at = time.time()
    report.add("SONARPAD MPV DIAGNOSTIC REPORT")
    report.add(f"generated_utc={dt.datetime.now(dt.timezone.utc).isoformat()}")
    report.add(f"scenario={args.scenario}")
    report.add(f"candidate_url={args.candidate_url}")
    report.add(f"baseline_url={args.baseline_url or '<none>'}")
    report.add(f"compatibility_target={args.compat_target}")

    report.section("RUNNER IDENTITY")
    for argv in (
        ["sw_vers"],
        ["uname", "-a"],
        ["uname", "-m"],
        ["arch"],
        ["sysctl", "-n", "machdep.cpu.brand_string"],
        ["xcodebuild", "-version"],
        ["xcrun", "--sdk", "macosx", "--show-sdk-path"],
    ):
        report.command(argv, run(argv, timeout=20))

    root = pathlib.Path(tempfile.mkdtemp(prefix="sonarpad-mpv-diag-"))
    try:
        cand_archive_name = pathlib.Path(urllib.parse.urlparse(args.candidate_url).path).name or "candidate.zip"
        cand_archive = root / cand_archive_name
        candidate_app = None
        if download(args.candidate_url, cand_archive, args.candidate_sha, report, "candidate"):
            candidate_app = extract_archive(cand_archive, root / "candidate", report, "candidate")

        baseline_app = None
        if args.baseline_url:
            base_archive_name = pathlib.Path(urllib.parse.urlparse(args.baseline_url).path).name or "baseline.tar.gz"
            base_archive = root / base_archive_name
            if download(args.baseline_url, base_archive, args.baseline_sha, report, "baseline"):
                baseline_app = extract_archive(base_archive, root / "baseline", report, "baseline")

        candidate_state: dict[str, object] = {}
        baseline_state: dict[str, object] | None = None
        candidate_results: list[ProbeResult] = []
        packaged_results: list[ProbeResult] = []
        baseline_results: list[ProbeResult] = []

        if candidate_app:
            candidate_state = analyze_bundle(candidate_app, "candidate 0.41", args.compat_target, report)
        else:
            report.section("CANDIDATE UNAVAILABLE")
            report.add("Candidate could not be downloaded/extracted; runtime probes skipped.")

        if baseline_app:
            baseline_state = analyze_bundle(baseline_app, "known-good baseline", args.compat_target, report)

        runtime_enabled = args.runtime == "yes"
        network_enabled = args.network == "yes"
        if runtime_enabled and candidate_app:
            candidate_results = runtime_probes(candidate_app, "candidate", report, args.live_url, args.vod_url, network_enabled)
            nested = create_sonarpad_packaging_simulation(candidate_app, root / "packaging", report)
            if nested:
                packaged_results = runtime_probes(nested, "packaged", report, args.live_url, args.vod_url, network_enabled)
        if runtime_enabled and baseline_app:
            baseline_results = runtime_probes(baseline_app, "baseline", report, args.live_url, args.vod_url, network_enabled)

        crash_reports_since(started_at, report)
        summarize(
            args.scenario,
            args.compat_target,
            candidate_state,
            baseline_state,
            candidate_results,
            packaged_results,
            baseline_results,
            runtime_enabled,
            report,
        )
    except Exception as exc:
        report.section("UNEXPECTED DIAGNOSTIC SCRIPT ERROR")
        report.add(f"{type(exc).__name__}: {exc}")
    finally:
        try:
            shutil.rmtree(root)
        except OSError:
            pass

    output = pathlib.Path(args.output)
    report.write(output)
    print(f"Diagnostic report written to {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
