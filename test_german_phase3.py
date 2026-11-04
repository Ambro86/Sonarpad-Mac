"""German phase-three catalog / UI routing regression tests (Python-only)."""
import json
import re
from pathlib import Path

ROOT=Path(__file__).resolve().parent


def test_all_calendar_dates_and_quotes_have_german_text():
    data=json.loads((ROOT/'src/calendar_data.json').read_text(encoding='utf8'))
    dates=data['saints']
    assert len(dates)==365
    for month in range(1,13):
        for day in range(1,32):
            key=f'{day}-{month}'
            if key in dates:
                assert dates[key]['de'].strip(),key
                assert dates[key]['it'].strip(),key
    assert len(data['quotes']['de'])==len(data['quotes']['en'])==128
    assert all(quote.strip() for quote in data['quotes']['de'])
    assert data['saints']['25-12']['de'].startswith('Weihnachten')
    assert data['saints']['1-11']['de']=='Allerheiligen'
    assert data['saints']['2-2']['de'].startswith('Darstellung des Herrn')


def test_all_99_media_cutter_labels_translated_and_routed():
    cutter=(ROOT/'src/media_cutter.rs').read_text(encoding='utf8')
    german=json.loads((ROOT/'i18n/media_cutter_de.json').read_text(encoding='utf8'))
    labels=[line for line in cutter.splitlines() if ': t(l,' in line]
    assert len(labels)==99
    for label in labels:
        strings=re.findall(r'"(?:\\.|[^"\\])*"',label)
        assert len(strings)==7,label[:100]
        english=json.loads(strings[1])
        assert english in german, english
        assert german[english].strip()
    assert len(german)==98
    assert '"de" => german_cutter_label(en)' in cutter
    assert '"de" => ["Stunde", "Stunden", "Minute", "Minuten", "Sekunde", "Sekunden"]' in cutter


def test_secondary_features_select_german_catalogs():
    for module in ('batch_audiobooks.rs','media_join.rs','podcast_recording.rs'):
        src=(ROOT/'src'/module).read_text(encoding='utf8')
        assert '"de" => include_str!("../i18n/ui_de.json")' in src,module
    routes=(ROOT/'src/routes.rs').read_text(encoding='utf8')
    assert '"de" => "de"' in routes
    assert '"de" => "DEU"' in routes
    podcasts=(ROOT/'src/podcasts.rs').read_text(encoding='utf8')
    assert 'ui_language.eq_ignore_ascii_case("de")' in podcasts
    german_categories=podcasts.split('ui_language.eq_ignore_ascii_case("de")',1)[1].split('ui_language.eq_ignore_ascii_case("en")',1)[0]
    assert len(re.findall(r'id: \d+,',german_categories))==19
    assert 'name: "Nachrichten"' in german_categories
    main=(ROOT/'src/main.rs').read_text(encoding='utf8')
    for phrase in ('"de" => "Radiosender suchen"','"de" => "Wörterbucheinträge"','"de" => "Aufnahmen"'):
        assert phrase in main
    bdciechi=(ROOT/'src/bdciechi.rs').read_text(encoding='utf8')
    assert 'fn bdciechi_is_german_ui()' in bdciechi
    assert '"Netzwerkfehler: {}"' in bdciechi


def test_complete_historical_german_changelog_is_loaded():
    english=(ROOT/'changelog_en.md').read_text(encoding='utf8')
    german=(ROOT/'changelog_de.md').read_text(encoding='utf8')
    versions=['0.5.3','0.5.2','0.5.0','0.4.0','0.3.1','0.2.9','0.2.8','0.2.7','0.2.6','0.2.5','0.2.4','0.2.2','0.2.1','0.2.0']
    for v in versions:
        assert f'Version {v}' in german,v
    assert len(re.findall(r'^\d+\. ',german,re.M))==len(re.findall(r'^\d+\. ',english,re.M))
    assert len(re.findall(r'^- ',german,re.M))==len(re.findall(r'^- ',english,re.M))
    main=(ROOT/'src/main.rs').read_text(encoding='utf8')
    assert '"de" => include_str!("../changelog_de.md")' in main
    newest=german.split('Version 0.5.2',1)[0]
    assert '18. Vollständige deutsche Übersetzung' in newest
    for lang in ('it','en','fr','es','pt','cs','pl'):
        text=(ROOT/f'changelog_{lang}.md').read_text(encoding='utf8')
        first_section=text.split('0.5.2',1)[0]
        assert re.search(r'(?m)^18\. ',first_section),lang


def test_earlier_german_work_is_preserved():
    german=json.loads((ROOT/'i18n/ui_de.json').read_text(encoding='utf8'))
    assert len(german)==529
    descriptions=json.loads((ROOT/'i18n/audio_description_de.json').read_text(encoding='utf8'))
    assert len(descriptions)==240
    feeds=(ROOT/'i18n/feed_de.txt').read_text(encoding='utf8').splitlines()
    assert len(feeds)==14
    assert feeds[0].startswith('Tagesschau | https://www.tagesschau.de')
    assert feeds[-1].startswith('Spektrum.de | https://www.spektrum.de')
