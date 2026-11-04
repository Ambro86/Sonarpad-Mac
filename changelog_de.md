Änderungsprotokoll

Version 0.5.3 – 14. Oktober 2026

- Cloud-Synchronisierung zwischen Windows, Mac und Mobilgeräten für Radio- und TV-Favoriten, eigene Nachrichtenquellen und Ordner, Podcasts und SonarTube-Favoriten hinzugefügt. Verbindung über einen temporären Code. Artikel-Favoriten bleiben erhalten, soweit unterstützt. Manuelle Synchronisierung mit Löschungen und Diagnoseprotokollen. Auch kompatible Moduleinstellungen werden synchronisiert, darunter Suchtyp, Wetterort und Schnittmodus. Separate Steuerung für Sammlungen und Einstellungen; Einstellungen sind auf 32 KB begrenzt.

1. SonarTube ist der neue Name von „Audio aus Streams abspielen“ und bietet die vertrauten Funktionen. Bei der YouTube-Suche kann zwischen Alle, Videos, Playlists, Kanäle, Kurse und Podcasts gewählt werden. Die Auswahl bleibt gespeichert und beeinflusst das direkte Öffnen von Links nicht.

2. Schnellere Audio- und Videotranskription auf kompatiblen Macs durch eine neue, für die CPU optimierte Engine. Apple-Silicon-Macs verwenden bei nativer Ausführung Metal-Beschleunigung mit automatischem CPU-Fallback. Intel-Macs und Rosetta verwenden die CPU. Sprach- und Modellauswahl, Zeitmarken, Fortschritt und Abbruch bleiben erhalten. Unter Catalina und vor macOS 13.3 bleibt die bisherige Engine erhalten, die bei Problemen auch als automatischer Fallback dient. Das neue Modell wird beim ersten Einsatz heruntergeladen und anschließend wiederverwendet.

3. Der Radiokatalog wurde erweitert und bietet jetzt mehr Sender, Ergebnisseiten und eine zuverlässigere Erkennung von Duplikaten.

4. „Mediendateien schneiden“ wurde hinzugefügt: Audio und Video zuschneiden, Schnitte vorhören, Dateien aufteilen, Abschnitte entfernen oder wiederherstellen und die verbleibenden Teile verbinden. Videos lassen sich drehen und durch zusätzliche Tonspuren ergänzen, mit getrennter Lautstärke für Originalton und neue Spur sowie optionaler Wiederholung. Gelöschte Teile werden beim Navigieren und bei der Vorschau übersprungen und als gelöscht angesagt. Ein Befehlsbereich ermöglicht Tasteneingaben ohne Option/Befehl; hinzu kamen F1-Hilfe, Tastenkürzel auf Schaltflächen und gesprochene Teilauswahl. Zeiten werden in Stunden, Minuten und Sekunden angegeben; Schnittgrenzen lassen sich direkt anpassen. Zusätzliche Spurregler erscheinen nur bei Bedarf. Export in verschiedene Audio- und Videoformate mit vorgeschlagenem Namen und lokalisiertem Zusatz; Bestätigung beim Schließen ungesicherter Arbeit oder Unterbrechen eines Exports. Projekte können als .sonarcut gespeichert und wieder geöffnet werden, ohne das Ausgangsmaterial erneut zu komprimieren. Optionale GoldWave-Tasten ([ für Anfang, ] für Ende, Entf zum Entfernen einer Auswahl auch im erweiterten Modus) und „Markierungen löschen“ (Option+0 bzw. 0 im Befehlsbereich) erhalten die Schnitte und die Wiedergabeposition. GoldWave- und Überschreibungseinstellungen bleiben gespeichert. Nach Bestätigung kann das Original beim Export ersetzt werden; sowohl vom Original als auch von vorhandenen Zieldateien wird eine Sicherung erstellt.

5. Die Audiodeskription enthält jetzt die jüngsten Verbesserungen der Windows-Version: Eine standardmäßig deaktivierte Option erlaubt Beschreibungen während nichtsprachlicher Lautäußerungen ausschließlich nach KI-Prüfung; Dialoge bleiben geschützt. Tonhöhe und Filmlautstärke sind einstellbar und werden im Projekt gespeichert; beim Ändern der Originallautstärke gibt es eine gemischte Vorschau. Die Wiederaufnahme unterbrochener Aufträge wurde verbessert: bereits abgeschlossene Analysen, geprüfte Lautäußerungen und visuelle Zeitmarken bleiben erhalten. Ist das vorbereitete Video inkompatibel, wird der Zwischenstand bewahrt, statt die Analyse neu zu beginnen.

6. „Diagnose senden“ wurde dem Hilfemenü hinzugefügt. Protokoll und Bericht können mit Namen und optionaler Antwort-E-Mail an den Entwickler gesendet werden. Die Protokolldatei muss nicht mehr manuell im Ordner Application Support gesucht werden.

7. „Mediendateien zusammenfügen“ unter Werkzeuge → Multimedia (Option+Umschalt+Y): Audio- oder Videodateien in der gewünschten Reihenfolge verbinden, Dateien hinzufügen, entfernen und umsortieren, Zielformat auswählen sowie Fortschritt anzeigen und abbrechen. Auch mehrere Videos lassen sich zu einer Audiodatei verbinden. Beschriftungen wurden an „Ordner konvertieren“ angepasst, mit getrenntem Feld und Auswahlschaltfläche für den Quellordner sowie lokalisierten Bestätigungsdialogen.

8. „Hörbücher stapelweise erstellen“ unter Werkzeuge → Lesen und Inhalte: Dateien oder einen Ordner hinzufügen, Aufträge entfernen, Zielordner sowie MP3, M4B, M4A oder WAV wählen. Verwendet die eingestellte Stimme und Spracheinstellungen, erstellt ein Hörbuch pro Dokument und unterstützt Unterordner je Buch. Vorhandene Dateien werden durch eine Nummer im Dateinamen geschützt. Mit echtem Fortschritt, Abbruch, Ergebnisübersicht und Fortsetzung nach Fehlern einzelner Dateien. Microsoft-Sprachcaching und automatische Wiederholungsversuche werden wiederverwendet.

9. „Podcast aufnehmen“ unter Werkzeuge → Multimedia: Mikrofon, Systemaudio oder beides aufnehmen, Mikrofon auswählen, Lautstärken getrennt einstellen, ohne künstliche Stille pausieren und fortsetzen sowie in MP3, M4A oder WAV speichern. Mit Aufnahmedauer, VoiceOver-Ansagen, prozentualem Speicherfortschritt und lokalisierten Bestätigungen vor Abbruch. Schlägt das Speichern fehl, bleiben die Spuren erhalten. Systemaudio erfordert macOS 13 oder neuer; Mikrofonaufnahmen sind auch unter Catalina und anderen älteren unterstützten Versionen verfügbar.

10. Mehrere Dokumente können gleichzeitig geöffnet werden. Wechsel mit Befehl+< und Befehl+> oder über das Fenstermenü. Im Menü Datei gibt es „Neues Dokument“ und „Dokument schließen“. Jeder Text behält eigene Änderungen und Leseposition; beim Beenden wird für sämtliche Dokumente geprüft, ob ungespeicherte Änderungen vorliegen.

11. SonarMusic wurde zu Multimedia hinzugefügt: Suche nach Titeln, Alben, Künstlern, Playlists und Musikvideos; Vorschläge erscheinen erst während der Eingabe. Ergebnisse können durchsucht und seitenweise angezeigt werden. Titel lassen sich mit den vorhandenen Werkzeugen abspielen, speichern, transkribieren und per KI audiodeskribieren.

12. YouTube-Videos und SonarMusic-Titel lassen sich schneller öffnen; bei Fehlern wird automatisch eine alternative Methode versucht.

13. Audiodeskriptionen: Wenn die Erkennung von Bildschirmtext aktiviert ist, werden auch von Gemini erkannte Abspänne vorgelesen.

14. Audiodeskriptionen: Die zeitliche Abstimmung wurde korrigiert. Nach langen Abschnitten ohne geeignete Sprechpause beschreibt Gemini nicht mehr fortlaufend frühere statt aktueller Szenen.

15. Audiodeskriptionen: Passt ein Satz nicht in die verfügbare Pause, versucht Sonarpad eine erneute Sprachsynthese mit 10 % oder bei Bedarf 15 % höherer Geschwindigkeit und prüft die tatsächliche Dauer. Die Geschwindigkeit gilt ausschließlich für diesen Satz und bleibt bei Bearbeitung, Vorschau und Export gespeichert; alle anderen Sätze bleiben unverändert.

16. Audiodeskriptionen: Die kürzeste zulässige Pause beträgt jetzt 1,5 Sekunden. Dadurch können auch kurze, bisher ungenutzte Pausen für weitere Beschreibungen verwendet werden.

17. Audiodeskriptionen: Zuvor wegen Platzmangels ausgeschlossene Sätze werden anhand ihrer realen Synthesedauer erneut geprüft und bei Bedarf mit 10 %, maximal 15 % höherer Geschwindigkeit wiederhergestellt. Vorhandene Beschreibungen und Dialoge bleiben geschützt; die individuelle Geschwindigkeit wird im Projekt gespeichert.

18. Vollständige deutsche Übersetzung von Sonarpad für macOS hinzugefügt: Benutzeroberfläche, Menüs, Kalender, Audiodeskription, Nachrichtenquellen, Multimediawerkzeuge und Änderungsprotokoll. Deutsch kann in den Spracheinstellungen ausgewählt werden.

Version 0.5.2 – 12. Oktober 2026

Edge-Stimmen, Audiodeskription und Zuverlässigkeit

1. Neue Edge-Verbindung: Sonarpad versucht zuerst den neueren Microsoft-Edge-Dienst, der in Tests erfolgreich Audio erzeugt hat. Ist er nicht verfügbar, wird automatisch die bisherige Verbindung verwendet, ohne die ausgewählte Stimme zu ändern.

2. Weniger Unterbrechungen: Bei HTTP 403, abgebrochenen Verbindungen oder vorübergehend abgewiesenen Anfragen wird die Wiederherstellung automatisch versucht. Bei HTTP 429 wartet Sonarpad länger und beachtet die von Microsoft vorgegebene Wartezeit. Stopp und Abbrechen bleiben auch während langer Vorgänge verfügbar.

3. Ganze Sätze ohne Neustart: Eine Ausgabe wird erst nach der Endbestätigung `turn.end` angenommen. Leere, abgeschnittene oder beschädigte Audiosegmente werden verworfen und nur der betroffene Abschnitt erneut erzeugt. Fertige Hörbuchblöcke bleiben erhalten; die Audiodeskription verwendet weiterhin bis zu acht parallele Synthesen. Zusätzliche Prüfungen betreffen litauische Stimmen, Änderungen im Aussprachewörterbuch und Textlängen.

4. Bessere Diagnose: Im Protokoll ist jetzt ersichtlich, ob der neue Edge-Dienst oder die ältere Ersatzverbindung tatsächlich Audio geliefert hat. Eine geöffnete Verbindung zählt nicht mehr fälschlich als erfolgreiche Synthese.

5. Einfachere Stimmenvorschau: Eine neue Stimmenvorschau beendet die vorherige. Beim Schließen der Einstellungen wird die aktuelle Vorschau gestoppt. Wiederherstellung der Sprachausgabe, Audioprüfung vor dem Abspielen und Wiederverwendung fertiger Blöcke wurden verbessert.

6. Zuverlässigere Audiodeskriptionen: Ungewöhnliche Gemini-Antworten und abgelaufene Sitzungen werden besser behandelt. Läuft bereits eine Sonarpad-AI-Erstellung, wartet die Anwendung auf dieselbe Anfrage statt eine zweite auszulösen. Abbrechen erzeugt keine irreführenden Fehlermeldungen mehr.

7. Wiederherstellung schwieriger Medien: Bei Problemen mit Videozeitmarken werden nach Fehlschlag des normalen Ablaufs kleine Reparaturen versucht. Problematische Dateien lassen sich für die Analyse vorbereiten, ohne die ursprüngliche Dialogspur zu verlieren. Beschädigtes Audio wird nach Möglichkeit mit FFmpeg repariert; intakte Dateien durchlaufen den normalen Weg.

8. Weitere Möglichkeit bei fehlenden Sprechpausen: Wenn keine Beschreibungen hineinpassen, kann Sonarpad einen kurzen Versuch anbieten. Erst nach gesonderter ausdrücklicher Zustimmung darf die letzte Alternative über Dialoge sprechen und die Originallautstärke dabei automatisch verringern. Der normale Analyseablauf bleibt unverändert.

Version 0.5.0 – 22. September 2026

Audiodeskription, Werkzeuge und Stapelkonvertierung

1. Die aktuellen Verbesserungen der Windows-Audiodeskriptionsengine wurden auf macOS übertragen: robusterer Gemini-Bridge-Dienst, bessere Verarbeitung problematischer Videosegmente und zuverlässigere Prüfungen bei Analyse und erneutem Export.

2. Für „Audiodeskription mit KI erstellen“ stehen jetzt „Eigenen Gemini-API-Schlüssel verwenden“ und „Sonarpad AI verwenden“ zur Auswahl. Beide Zugangsdaten werden getrennt gespeichert; beim Wechsel geht weder der private API-Schlüssel noch der Sonarpad-AI-Code verloren.

3. Bei Sonarpad AI wird das aktuelle Guthaben in einem schreibgeschützten Feld angezeigt. Der Code lässt sich einblenden und ein neuer Code anfordern.

4. Bei Dateien mit mehreren Tonspuren wird vor dem Erstellen der Audiodeskription die zu analysierende Spur ausgewählt. Die Auswahl wird im Projekt gespeichert und für spätere Vorgänge wiederverwendet.

5. Eine neue Option erkennt wichtigen eingeblendeten Text und bezieht ihn in die Beschreibungen ein.

6. Neben der normalen Audioausgabe kann bei der Audiodeskription optional auch ein fertiges Video mit der Beschreibung erstellt werden. Container- und Zeitmarkenverarbeitung bei alternativen Ausgabeformaten wurden verbessert.

7. Sanftere automatische Lautstärkeabsenkung der Originaltonspur (Ducking): Absenken und Wiederanheben rund um die Erzählung erfolgen flüssiger.

8. Der Projekteditor bietet jetzt „Segment erneut analysieren“ für die ausgewählte Beschreibung. Dabei wird automatisch die zuletzt verwendete globale KI-Zugriffsart wie unter Windows benutzt.

9. Mehrere Beschreibungen können im Projekteditor gleichzeitig als noch nicht angewendete Änderungen vorliegen. Entwürfe bleiben beim Navigieren und Suchen erhalten. „Anwenden“ prüft und übernimmt alle zusammen. Passt eine Beschreibung nicht in die Sprechpause, wird keine Änderung übernommen und die betreffende Beschreibung zur Korrektur ausgewählt.

10. Beim Erstellen einer zweiten Audiodeskription ohne Schließen des Fensters wird der Speicherpfad nun korrekt für die neu ausgewählte Quelldatei erzeugt und nicht vom vorherigen Projekt übernommen.

11. Die Einstellung „Werkzeugmenü nach Kategorien gruppieren“ wurde hinzugefügt. Aktiviert erscheinen die Gruppen „Lesen und Inhalte“, „Multimedia“ und „Dienstprogramme“; deaktiviert wird die flache Menüliste angezeigt.

12. In der Treccani-Enzyklopädie erscheint das zuvor vor einer Suche sichtbare leere Ergebnisfeld nicht mehr in der barrierefreien Oberfläche. Es wird erst bei auswählbaren Ergebnissen eingeblendet.

13. „Ordner konvertieren“ unter Werkzeuge → Multimedia: Ein ganzer Ordner lässt sich mit denselben Formaten und Optionen wie einzelne Medien konvertieren, beispielsweise mehrere WMA-Dateien in MP3.

14. Die Ordnerkonvertierung zeigt den Fortschritt pro Datei, schlägt einen Unterordner „Konvertiert“ vor, erhält die ursprünglichen Dateinamen, fasst Fehler zusammen und schützt vor Überschreibungen und Namenskollisionen.

15. Eine standardmäßig aktive Einstellung gibt beim Vor- und Zurückspulen die aktuelle Position und die Gesamtdauer in natürlicher Sprache an, etwa „1 Minute 10 Sekunden von 1 Stunde 10 Minuten 10 Sekunden“. Wird sie deaktiviert, spricht Sonarpad wie bisher nur die aktuelle Position.

16. Während der Wiedergabe nennt Option+I ausschließlich die Gesamtdauer des Mediums in natürlicher Sprache, unabhängig von der vorherigen Einstellung. Live-Streams werden als live angesagt.

17. Im Fenster „Audiodeskription mit KI erstellen“ befinden sich Engine und Stimme nun in einer separaten Ansicht „Stimme anpassen“. Hier können Engine, Stimme, Geschwindigkeit, Lautstärke und Testwiedergabe gewählt werden. Die Optionen werden für Audiodeskriptionen gespeichert. Ohne Aufruf gelten weiterhin die allgemeinen Geschwindigkeits- und Lautstärkeeinstellungen.

18. Bei Streamingmedien erscheint jetzt die Dauer jedes Videos neben dem Titel. „Ordner konvertieren“ hat klarere Beschriftungen für Quell- und Zielordner. „Konvertierung stoppen“ beendet den laufenden FFmpeg-Prozess sofort, löscht unvollständige Ausgaben und startet keine weiteren Dateien.

19. Ein zusätzlicher Schutz behebt Probleme bei Mehrkanalquellen wie 5.1, 6.1 oder 7.1. Die normale Verarbeitung bleibt unverändert; nur wenn internes WAV-Audio unlesbar ist, ein unerwartetes Format hat oder PCM-Frames versetzt sind, wird dieser Schritt automatisch als 48-kHz-Stereo neu erstellt und wiederholt.

20. Beim Start einer Einzeldatei- oder Ordnerkonvertierung meldet VoiceOver „Konvertierung gestartet“, ohne dass der Fortschrittsbalken angesteuert werden muss.

21. RSS-Quellen funktionieren zuverlässiger: Liefert der Originalfeed eines Anbieters Fehler oder keine Artikel, versucht Sonarpad automatisch einen auf dieselbe Website und die eingestellte Nachrichtensprache beschränkten Google-News-Feed. Der Originalfeed bleibt gespeichert und hat Vorrang; der Fallback funktioniert auch für Il Giornale und verbreitete technische Feedanbieter.

22. Behoben: Beim Schließen eines geänderten Dokuments unter macOS konnte nach „Nicht sichern“ die Speicherabfrage ein zweites Mal erscheinen. Die Bestätigung gilt jetzt für das aktuelle Schließereignis und wird nur einmal abgefragt.

23. „Nachrichtenquellen der Community“ zeigt alle Quellen der ausgewählten Sprache, auch bereits in der Bibliothek vorhandene. Importierte Quellen tragen die Kennzeichnung „Bereits importiert“. Beim erneuten Import wird nach Ersetzen gefragt; derselbe Eintrag wird aktualisiert, ohne Duplikate zu erzeugen, und bleibt in seinem bisherigen Ordner.

24. „Zu Datum springen“ ist jetzt bei Podcasts und RaiPlay Sound verfügbar, wie in der mobilen Version. Bei Podcasts erscheint der Befehl nur, wenn echte Episodendaten verfügbar sind; die Auswahl zeigt alle Episoden dieses Tages, auch über die ersten 30 Menüeinträge hinaus. In RaiPlay Sound erscheint bei datierten Inhalten eine entsprechende Schaltfläche. Datumslisten enthalten nur tatsächlich vorhandene Daten, ohne überflüssige Beschriftungen, für eine übersichtliche VoiceOver-Navigation.

25. Die Medienwiedergabe unterstützt Pos1 und Ende: Pos1 bzw. Fn+Pfeil links auf MacBooks springt an den Dateianfang, Ende bzw. Fn+Pfeil rechts fünf Sekunden vor das Ende. Die Pfeiltasten allein behalten das eingestellte Sprungintervall.

Version 0.4.0 – 3. September 2026

KI-Audiodeskription – wichtige neue Funktion

- „Audiodeskription mit KI erstellen“ wurde direkt im Werkzeugmenü hinzugefügt. Sonarpad untersucht die Tonspur auf Pausen ohne Dialoge, erstellt mit Gemini Beschreibungen und spricht diese mit den vorhandenen Sonarpad-Stimmen, ohne Dialoge zu überdecken.
- Verbesserte Synchronisierung zwischen Videoereignissen und den erzeugten Beschreibungen einschließlich automatischer Prüfung der Gemini-Zeitmarken.
- „Erweiterte Pausen zulassen“ ist standardmäßig deaktiviert und kann bei dialogreichen Inhalten mit wenig freiem Platz aktiviert werden, damit längere Beschreibungen eingefügt werden können.
- Sonarpad kann Figuren erkennen und ihre Namen verwenden. Figurenkataloge lassen sich über mehrere Episoden einer Serie hinweg wiederverwenden.
- Projekte können gespeichert, später bearbeitet und erneut exportiert werden, ohne die komplette Gemini-Analyse zu wiederholen.
- Nach einer Unterbrechung bleibt der Fortschritt erhalten. Ist das Gemini-Kontingent erschöpft, kann man warten, das Modell wechseln oder den Vorgang ohne Verlust der abgeschlossenen Arbeit beenden.
- Das Fenster bietet Sprachwahl, Detailgrad, Gemini-Modell, Sprachengine und Stimme; die Auswahl wird gespeichert. Das Modul ist in den von Sonarpad für Mac unterstützten Sprachen verfügbar.
- Während der Erstellung zeigt die Oberfläche Fortschritt, Status und Abbrechen; das fertige MP3 kann direkt im internen Player geöffnet werden.
- Verbesserte MKV-Kompatibilität: unregelmäßige oder fehlende Zeitmarken werden robuster verarbeitet; beschädigte Pakete können nach Möglichkeit übersprungen werden.
- Behoben: MP3-Exporte konnten bei Videos mit Mehrkanalton wie Dolby 5.1 fehlschlagen. Sonarpad mischt Mehrkanalton bei Bedarf automatisch auf Stereo herunter.
- Bei mehreren Tonspuren kann die gewünschte Spur vorab über ein zugängliches Kombinationsfeld mit Pfeiltasten ausgewählt werden. OK beginnt die Beschreibung, Abbrechen schließt das Fenster und bringt den Fokus zurück in den Editor.
- Neben dem Gemini-Schlüssel gibt es „API-Schlüssel anzeigen“. Der Schlüssel bleibt standardmäßig verborgen und wird nur bei aktivierter Option eingeblendet; beim erneuten Öffnen ist er wieder verdeckt.

YouTube und Streaming

- Die YouTube-Suche und -Navigation wurde deutlich beschleunigt und ihre Funktion wiederhergestellt.
- Die Videoqualitäten besitzen jetzt verständliche, übersetzte Bezeichnungen statt des technischen Werts „best“.
- Sonarpad merkt sich das zuletzt beim Speichern von Medien verwendete Format, z. B. MP4.

Danksagungen

- Besonderer Dank gilt Leonardo Graziano und Tiziano Ferraro für ihre umfangreichen Tests von KI-Audiodeskription und Sonarpad und ihre wertvollen Verbesserungsvorschläge.
- Vielen Dank auch an die Gruppe Tecnologia Accessibile für Unterstützung, Tests und Anregungen.

Version 0.3.1 – 16. Juli 2026

- Ein Fehler mit ungültigen wxWidgets-Menükennungen beim Start mit Radiofavoriten wurde behoben.
- Sonarpad ist neben Italienisch und Englisch jetzt auch auf Französisch, Spanisch, Portugiesisch, Tschechisch und Polnisch verfügbar.
- Eine separate Einstellung für die Nachrichtensprache wurde ergänzt. Sie ist von der Oberflächensprache unabhängig und lädt passende Quellen und Dienste.
- Neu: Wetter mit Städtesuche, aktuellen Bedingungen, Temperatur, Niederschlag, Wind, Luftfeuchtigkeit sowie Vorhersagen für heute, morgen und beliebige Tage.
- Neu: Kinofilme mit aktuellen und kommenden Filmen, Inhaltsangaben, Startterminen und gegebenenfalls Trailerlinks.
- Neu: Barrierefreier Kalender im Werkzeugmenü mit Datumsauswahl, Feiertag, Tagesheiligem, Tageszitat und Erinnerungen, die direkt zum macOS-Kalender hinzugefügt werden können.
- Neu: Routensuche für Fußwege, Radfahren, Autofahrten und rollstuhlgerechte Wege; schnellste oder kürzeste Route, Entfernung, geschätzte Dauer und detaillierte Wegbeschreibung.
- Neu: Medienkonvertierung unter anderem in MP3, M4A, M4B, MP4, AVI, MOV, Opus, OGG, FLAC, WAV und AIFF. Aus Audio und einem Bild lässt sich auch ein Video erstellen.
- Neu: Aussprachewörterbuch für den Ersatz bestimmter Wörter und Ausdrücke bei der Sprachausgabe, etwa zur Korrektur von Namen, Abkürzungen oder Aussprache.
- Die Artikelansicht wurde um „Zuletzt gelesene Artikel“ und „Teilen“ erweitert, um gelesene Beiträge wiederzufinden und über macOS zu teilen.
- Im Artikelmenü stehen „Nachrichtenquelle an die Sonarpad-Community senden“ und „Nachrichtenquellen der Sonarpad-Community“. RSS-Feeds und Websites können eingereicht und Quellen anderer Nutzer importiert werden; die eingestellte Nachrichtensprache wird berücksichtigt.
- Die Verwaltung von Nachrichtenquellen wurde verbessert: Beim Sprachwechsel werden passende Standardquellen geladen, ohne persönlich hinzugefügte Quellen zu löschen.
- Erweiterte Radiosuche nach Sprache, Land und Stadt mit vollständigen, übersetzten Ländernamen.
- Radiosender können mit Name, Streamadresse, Sprache und Genre an die Sonarpad-Community gemeldet werden.
- Radioaufnahmen und zeitgesteuerte Radioaufnahmen sind jetzt in Suchergebnissen und Favoriten verfügbar und werden direkt als MP3 gespeichert. Bei geöffnetem Radiosender startet die Taste R ebenfalls die Aufnahme.
- Im Dateimenü gibt es eine Liste zuletzt geöffneter Textdokumente zum schnellen Wiederöffnen.
- Der schreibgeschützte Modus verhindert versehentliche Änderungen beim Lesen.
- Für EPUB-Dateien mit Inhaltsverzeichnis wurde ein Befehl zum direkten Sprung in ein Kapitel ergänzt.
- Zur Auswahl stehen hochwertige Microsoft-Stimmen und die Systemstimmen von macOS.
- Optional können durch Leerzeilen verursachte Sprechpausen ignoriert werden.
- Für Medien lässt sich das Vor- und Zurückspulintervall in Sekunden einstellen.
- Barrierefreiheit von Fenstern, Menüs und Steuerelementen sowie Fokus, Eingabe, Escape und Tastaturkürzel wurde verbessert.
- Meldungen, Schaltflächen und Bestätigungen wurden in allen unterstützten Sprachen besser lokalisiert.
- Ein Fehler bei der Anzeige von Medien in einem eigenen Videofenster wurde behoben.
- Zahlreiche Fehler bei Stabilität, Medienwiedergabe, geplanten Radioaufnahmen, Quellenverwaltung und macOS-Kompilierung wurden korrigiert.
- Besonderer Dank gilt Leonardo Graziano, Luca Maianti und der italienischen Gruppe Tecnologia Accessibile für ihre fortlaufenden Tests und Unterstützung.

Version 0.2.9 – 1. Mai 2026
- YouTube-Funktionen wurden auf Intel-Macs und Catalina ausgeweitet.
- Die YouTube-Suche wurde deutlich beschleunigt.
- Kanäle und Playlists werden in den Suchergebnissen bevorzugt angezeigt.
- Kanäle und Playlists können zu Favoriten hinzugefügt und aus ihnen entfernt werden.
- In den Optionen gibt es eine Schaltfläche für Stimmenvorschauen.
- Beim Entfernen von Quellen wurde „Alle auswählen“ ergänzt.
- Wikipedia-Suchen zeigen einen Fortschrittsbalken.

Version 0.2.8 – 29. April 2026
- Neues Werkzeugmenü mit „Wikipedia durchsuchen und importieren“ und „Streamingaudio abspielen“.
- Wikipedia-Artikel lassen sich suchen, importieren, lesen und als Hörbuch speichern.
- Streamingangebote wie YouTube können abgespielt werden.
- Das Streamingsuchfeld akzeptiert beliebige Inhalte und kann auch Kanäle und Playlists öffnen.
- Die YouTube-Suche ist auf Intel-Macs aufgrund von Inkompatibilität nicht aktiviert.
- Besonderer Dank an Leonardo Graziano für seine kontinuierliche Unterstützung.
- Bei Radiosuchergebnissen kann die Ergebnisseite jetzt direkt ausgewählt werden, ohne wiederholt „Nächste Seite“ zu verwenden.
- Automatische Lesezeichen stehen jetzt auch für Textdateien zur Verfügung.

Version 0.2.7 – 28. April 2026
- Bessere Unterstützung für Textdateien mit Akzenten und Nicht-UTF-8-Codierungen, einschließlich chinesischer Zeichen und anderer Sprachen.
- Behoben: Ein Komma in einem Textfeld öffnete fälschlich die Einstellungen.
- Schnellere Sprachausgabe bei langen Artikeln ohne zusätzliche Pause nach jedem Absatz.
- JPG-Dateien und ähnliche Formate können in Sonarpad geöffnet und per OCR aus Bildern oder Fotos ausgelesen werden.
- Sonarpad kann als Standardprogramm festgelegt werden.
- Neben Text können Audio- und Videodateien im MPV-Player geöffnet werden.
- Neue automatische Lesezeichen speichern die genaue Position beim Schließen von Dateien, Podcasts und Medien.
- Radiosender werden nicht mehr in Safari geöffnet, sondern im Sonarpad-Player abgespielt.
- Die Anwendung ist jetzt signiert und benötigt keine zusätzliche Benutzerfreigabe, wodurch die Installation einfacher wird.
- Automatische Aktualisierung: Sonarpad prüft neue Versionen, lädt sie herunter und installiert sie.

Version 0.2.6
- Ein wx/macOS-Fehler mit einer Startfehlermeldung wurde behoben; die zugehörigen Menüs wurden stabilisiert.
- Das Tastenkürzel Befehl+, für Einstellungen funktioniert jetzt auch im Editor und in anderen Steuerelementen.
- Beim Speichern eines Hörbuchs wird das Dateinamenfeld korrekt fokussiert; Punkte im Dateinamen führen nicht mehr zum Abschneiden des Namens.
- OPML-Dateien mit Ordnerstruktur werden unterstützt: Ordner erscheinen als Untermenüs, Quellen öffnen sich in eigenen Dialogen.
- Artikelquellen lassen sich mit „Ordner öffnen“, „Stammordner“, „In Ordner verschieben“ und „Aus Ordner heraus verschieben“ reorganisieren.

Version 0.2.5
- Neue eigene Speicherdialoge für Textdateien und Hörbücher unter macOS.
- Dateinamenfelder unterstützen Befehl+V, Befehl+A und weitere übliche Bearbeitungstasten.
- Zuletzt verwendeter Speicherordner und Format für Text und Hörbuch werden gemerkt.
- Hörbücher lassen sich auch in M4A und WAV speichern.
- Das Radiomenü bietet sprachabhängige Suche, Favoriten, manuelles Hinzufügen sowie Bearbeiten und Sortieren der Favoriten.
- Verbesserte Feed-Erkennung für als Websites hinzugefügte Artikelquellen und Korrektur von Kommentarfeeds.
- Der macOS-Releaseablauf erzeugt jetzt auch das Catalina-Paket.

Version 0.2.4
- Große Verbesserungen der PDF-Texterkennung unter macOS durch pdfium und robustere Ersatzverfahren.
- M4B-Export für macOS hinzugefügt und das Speichern von Text verbessert.
- Bessere Verwaltung von Artikelquellen und Schutz bei Aktualisierungen ohne Ergebnisse.
- Verbesserte Aufteilung und Wiederholungsversuche bei Edge-TTS-Audio.
- Erstellung und Paketierung für Catalina wurden hinzugefügt und verfeinert.

Version 0.2.2
- PDF-Dateien werden unter macOS mit deutlicheren Rückmeldungen und ausdrücklichem Abschlussdialog geladen.
- Alphabetische Sortierung für Artikelquellen hinzugefügt.
- PDF-Textkorrektur und Übersetzungen verbessert.

Version 0.2.1
- macOS-Tastaturkürzel und Menübefehle für Start, Pause, Stopp und Speichern stabilisiert.
- Das Öffnen externer Podcast-Episoden unter macOS verbessert.
- Speicherung von Einstellungen unter macOS korrigiert.
- Intel-/macOS-Builds und die Auswahl der Xcode-Version zuverlässiger gemacht.

Version 0.2.0
- Erste macOS-Version von Sonarpad für Mac.
- Textvorlesen, Artikel und Podcasts mit Sprachausgabe.
- OCR für PDFs unter macOS, Aktualisierungsdownloads und eigene DMG-Pakete.
- Hierarchische Podcastkategorien und erste macOS-Tastaturkürzel.
