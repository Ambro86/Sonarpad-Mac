# Controllo di visibilità: servizi riservati all'italiano

Questo archivio deriva dalla fase 4 della traduzione tedesca. La selezione
``de`` non deve mostrare i servizi RaiPlay, RaiPlaySound, LA7, TV Italia,
Audiodescrizioni RAI e Audiodescrizioni Sonarpad. La medesima regola vale per
le altre lingue diverse da ``it``.

Le due modalità del menu Strumenti (suddivisa per categorie e piatta) già
applicavano la condizione `settings.ui_language == "it"`: sono state verificate
senza modificare queste condizioni. Audiodescrizioni Sonarpad conserva inoltre
il requisito `load_saved_rai_luce_code().is_some()`, cioè la presenza del
codice Sonarpad configurato. Il relativo dialogo ripete già i due controlli.

In questa revisione è stata aggiunta una salvaguardia al gestore dei comandi:
anche se gli identificatori di menu vengono inviati direttamente, questi
servizi non si aprono fuori dall'interfaccia italiana; il catalogo Sonarpad
richiede anche un codice configurato.

**Rimane disponibile in tedesco** la funzione generale di creazione delle
audiodescrizioni con IA (ID_TOOLS_AUDIO_DESCRIPTION), che è distinta dal
catalogo italiano delle Audiodescrizioni Sonarpad.

Verifiche: `python -m pytest -q test_german_phase1.py test_german_phase2.py
test_german_phase3.py test_german_phase4.py test_interface_language_localization.py
scripts/test_audio_description_localization.py test_italian_only_media_visibility.py`
(34 test superati, 27 sottotest). Non sono state effettuate compilazioni
macOS o prove delle finestre dal vivo.
