"""German phase-two source-level regression checks (no macOS toolchain needed)."""
import json
import re
from pathlib import Path
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parent

EXPECTED_FEEDS = [
    ('Tagesschau', 'https://www.tagesschau.de/xml/rss2/'),
    ('ZDFheute', 'https://www.zdf.de/rss/zdf/nachrichten'),
    ('Deutschlandfunk Nachrichten', 'https://www.deutschlandfunk.de/nachrichten-100.xml'),
    ('Deutsche Welle Deutsch', 'https://rss.dw.com/xml/rss-de-all'),
    ('DER SPIEGEL', 'https://www.spiegel.de/schlagzeilen/index.rss'),
    ('ZEIT ONLINE', 'https://newsfeed.zeit.de/index'),
    ('Frankfurter Allgemeine Zeitung', 'https://www.faz.net/rss/aktuell/'),
    ('Süddeutsche Zeitung', 'https://rss.sueddeutsche.de/rss/Topthemen'),
    ('Neue Zürcher Zeitung', 'https://www.nzz.ch/recent.rss'),
    ('DER STANDARD', 'https://www.derstandard.at/rss'),
    ('heise online', 'https://www.heise.de/rss/heise-atom.xml'),
    ('Golem.de', 'https://rss.golem.de/rss.php?feed=RSS2.0'),
    ('Netzpolitik.org', 'https://netzpolitik.org/feed/'),
    ('Spektrum.de', 'https://www.spektrum.de/alias/rss/spektrum-de-rss-feed/996406'),
]


def test_14_original_user_supplied_german_feeds():
    lines = (ROOT/'i18n/feed_de.txt').read_text(encoding='utf8').splitlines()
    feeds = [tuple(map(str.strip, line.split(' | ', 1))) for line in lines]
    assert feeds == EXPECTED_FEEDS
    assert len(set(url for _, url in feeds)) == 14
    for name, url in feeds:
        assert name and urlsplit(url).scheme == 'https' and urlsplit(url).netloc


def test_german_feeds_selected_by_news_language():
    src = (ROOT/'src/articles.rs').read_text(encoding='utf8')
    assert 'const DEFAULT_DE_FEEDS: &str = include_str!("../i18n/feed_de.txt")' in src
    assert '"de" => default_german_sources(),' in src
    assert '"de" | "de-de" | "german" | "tedesco" | "deutsch" => "de".to_string()' in src
    assert 'DEFAULT_DE_FEEDS,' in src
    assert '"de" | "de-de" | "german" | "tedesco" | "deutsch" => ("de", "DE", "de")' in src
    assert 'fn german_news_sources_match_supplied_rss_catalog()' in src
    main = (ROOT/'src/main.rs').read_text(encoding='utf8')
    assert '"de" => ("de", "DE", "DE:de")' in main
    assert '["it", "fr", "es", "pt", "cs", "pl", "de"]' in main
    assert '"de" => ["Italienisch", "Französisch", "Spanisch", "Portugiesisch", "Tschechisch", "Polnisch", "Deutsch"]' in main
    assert '"de" => "german"' in main


def test_german_audio_description_catalog_is_complete():
    p = ROOT/'i18n'
    english = json.loads((p/'audio_description_en.json').read_text(encoding='utf8'))
    german = json.loads((p/'audio_description_de.json').read_text(encoding='utf8'))
    assert len(english) == len(german) == 240
    assert set(english) == set(german)
    for key, value in english.items():
        assert german[key].strip(), key
        assert sorted(re.findall(r'\{[a-z_]+\}',value)) == sorted(re.findall(r'\{[a-z_]+\}',german[key])),key
    assert german['audio_description.cancel'] == 'Abbrechen'
    assert german['audio_description.audio_track.title'] == 'Audiospur auswählen'
    src = (ROOT/'src/audio_description.rs').read_text(encoding='utf8')
    assert '"de" => (&DE, include_str!("../i18n/audio_description_de.json"))' in src
    assert '("de", include_str!("../i18n/audio_description_de.json"))' in src
    assert 'fn german_audio_description_catalog_has_all_keys_and_placeholders()' in src
