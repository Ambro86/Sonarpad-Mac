"""Phase 4: German language coverage of native wxWidgets and secondary dialogs.

Static tests cannot replace a real macOS build and VoiceOver walkthrough.
"""
import gettext
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent
MAIN = (ROOT / 'src/main.rs').read_text(encoding='utf8')


def between(start: str, end: str, source: str = MAIN) -> str:
    assert start in source, start
    rest = source.split(start, 1)[1]
    assert end in rest, end
    return rest.split(end, 1)[0]


def test_wxstd_de_is_valid_extended_gnu_gettext_catalog():
    with (ROOT / 'i18n/wxstd_de.mo').open('rb') as stream:
        catalog = gettext.GNUTranslations(stream)
    assert len(catalog._catalog) >= 175
    for key, expected in {
        'Cancel':'Abbrechen','Open':'Öffnen','Close':'Schließen',
        'Save':'Speichern','Yes':'Ja','No':'Nein',
        'All files (*.*)|*.*':'Alle Dateien (*.*)|*.*',
        'macOS menu name\x04Window':'Fenster',
        'macOS menu item\x04Hide Others':'Andere ausblenden',
    }.items():
        assert catalog._catalog[key] == expected, key
    assert 'Language: de' in catalog._catalog['']
    assert '"de" => Some(include_bytes!("../i18n/wxstd_de.mo"))' in MAIN
    assert 'install_wx_translations(&initial_ui_language)' in MAIN


def test_app_bundle_localizations_keep_german():
    for path in (ROOT/'.github/workflows/macos-app-dmg.yml', ROOT/'.github/workflows/macos-app-dmg-catalina.yml'):
        content=path.read_text()
        assert 'de.lproj' in content
        assert 'string de' in content


def test_every_sonarmusic_action_has_a_german_label():
    data=json.loads((ROOT/'src/sonarmusic_labels.json').read_text())
    assert set(data['de']) == set(data['en'])
    for key in data['en']:
        assert data['de'][key].strip(), key
    assert data['de']['open_artist'] == 'Künstler öffnen'
    assert data['de']['use_suggestion'] == 'Vorschlag übernehmen'


def test_all_weather_codes_have_german_descriptions():
    code=between('fn weather_code_label(', 'fn ', MAIN)
    de=between('"de" => match code {', '"pl" => match code {', code)
    en=code.split('_ => match code {',1)[1]
    german_codes=set(re.findall(r'^\s*(\d+)\s*=>', de, re.M))
    english_codes=set(re.findall(r'^\s*(\d+)\s*=>', en, re.M))
    assert len(german_codes)==28
    assert german_codes==english_codes


def test_radio_tv_scheduling_and_media_voice_have_german_branches():
    for name,field in [('radio_schedule_labels','RadioScheduleLabels'),('tv_schedule_labels','TvScheduleLabels'),('mpv_voice_messages','MpvVoiceMessages')]:
        section=between(f'fn {name}(', '\nfn ', MAIN)
        german=between(f'"de" => {field} {{', f'_ => {field} {{', section)
        english=section.split(f'_ => {field} {{',1)[1].split('},',1)[0]
        german_fields=set(re.findall(r'^\s*([a-z_]+):',german,re.M))
        english_fields=set(re.findall(r'^\s*([a-z_]+):',english,re.M))
        assert german_fields==english_fields,(name,german_fields ^ english_fields)
    assert '"de" => "de-DE"' in MAIN


def test_tv_guide_search_and_recordings_are_german():
    for expected in ('TV-Programm des ausgewählten Senders','Gestern','Heute','Morgen',
                     'Rai-Sender','TV-Sender','Sender gefunden.','Keine Ergebnisse gefunden.',
                     'Aufnahmen…','Radiosender suchen','Radio- und TV-Aufnahmen',
                     'Für diese Sendung ist keine Inhaltsbeschreibung verfügbar.',
                     'Löschen fehlgeschlagen','Aus Favoriten entfernen'):
        if expected=='Keine Ergebnisse gefunden.':
            assert expected in (ROOT/'src/directories.rs').read_text()
        else:
            assert expected in MAIN,expected


def test_filesystem_dialog_labels_localized_for_german():
    assert 'fn localized_file_filter(filter: &str)' in MAIN
    for v in ['Alle Dateien|*.*', 'Unterstützte Dateien|', 'OPML oder TXT|', 'Medien|', 'Text|']:
        assert v in MAIN,v
    for path in ['src/audio_description.rs','src/media_join.rs','src/batch_audiobooks.rs','src/media_transcription.rs']:
        assert 'crate::localized_file_filter(' in (ROOT/path).read_text(),path


def test_secondary_business_directory_dialogs_use_german():
    content=(ROOT/'src/directories.rs').read_text()
    for v in ['Vorherige Seite','Nächste Seite','Suchergebnisse','Suche läuft','Details werden abgerufen',
              'Adresse:','Telefonnummern:','Webseiten:']:
        assert v in content,v
    bdciechi=(ROOT/'src/bdciechi.rs').read_text()
    assert 'Seite {} von {}' in bdciechi
    assert 'Seite 1' in bdciechi


def test_catalog_parity_across_four_phases():
    for group,length in [('ui',529),('audio_description',240),('media_transcription',27)]:
        de=json.loads((ROOT/f'i18n/{group}_de.json').read_text())
        en=json.loads((ROOT/f'i18n/{group}_en.json').read_text())
        assert len(de)==len(en)==length
        assert set(de)==set(en)
    assert len((ROOT/'i18n/feed_de.txt').read_text().strip().splitlines())==14
    assert (ROOT/'changelog_de.md').exists()
