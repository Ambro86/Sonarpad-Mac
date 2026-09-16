from pathlib import Path

ROOT = Path(__file__).resolve().parent
RUST = (ROOT / "src" / "audio_description_bridge.rs").read_text(encoding="utf-8")
SERVICE = (ROOT / "bridge" / "audio_description_runtime" / "audio_describer" / "core" / "sonarpad_service.py").read_text(encoding="utf-8")
SPEC = (ROOT / "bridge" / "audio_description_bridge_macos.spec").read_text(encoding="utf-8")


def test_rust_worker_exports_bundled_certifi_for_frozen_python():
    assert 'join("_internal")' in RUST
    assert 'join("certifi")' in RUST
    assert 'join("cacert.pem")' in RUST
    assert '.env("SSL_CERT_FILE", &ca_bundle)' in RUST
    assert '.env("REQUESTS_CA_BUNDLE", &ca_bundle)' in RUST
    assert '.env("CURL_CA_BUNDLE", &ca_bundle)' in RUST


def test_python_transport_uses_verified_certifi_context():
    assert "ssl.create_default_context()" in SERVICE
    assert "import certifi" in SERVICE
    assert "context.load_verify_locations(cafile=ca_file)" in SERVICE
    assert "context=self._ssl_context" in SERVICE
    assert "_create_unverified_context" not in SERVICE
    assert "CERT_NONE" not in SERVICE


def test_macos_bridge_bundle_already_contains_certifi():
    assert 'collect_data_files("certifi")' in SPEC
    assert '"certifi"' in SPEC


def test_ssl_context_keeps_verification_enabled():
    import ssl
    import sys

    runtime = ROOT / "bridge" / "audio_description_runtime"
    sys.path.insert(0, str(runtime))
    try:
        from audio_describer.core.sonarpad_service import _verified_ssl_context
        context = _verified_ssl_context()
    finally:
        sys.path.pop(0)
    assert context.verify_mode == ssl.CERT_REQUIRED
    assert context.check_hostname is True
