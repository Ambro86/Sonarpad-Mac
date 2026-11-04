# Sonarpad per macOS – Traduzione tedesca, fase 2 (PARZIALE)

Questa fase prosegue dalla fase 1, utilizzando soltanto lo ZIP dei sorgenti fornito dall’utente. Non è stato usato il bridge locale.

## Completato

Il catalogo `i18n/audio_description_de.json` traduce tutte le 240 voci del modulo "Crea audiodescrizioni con IA", comprese selezione file e tracce audio, modelli Gemini, impostazioni vocali, progetti, stato di avanzamento, crediti Sonarpad AI, messaggi di conferma ed errori. `src/audio_description.rs` carica il catalogo in tedesco quando la lingua dell’interfaccia è impostata su Deutsch.

Le impostazioni della sezione notizie consentono di selezionare Tedesco/Deutsch separatamente dalla lingua generale dell’interfaccia. Il catalogo `i18n/feed_de.txt` comprende esattamente le 14 fonti RSS tedesche fornite dall’utente, con titoli e URL preservati senza sostituzioni. `src/articles.rs` le seleziona come fonti predefinite per `news_language = "de"`; `src/main.rs` include l’opzione nella scelta della lingua e nella ricerca Google News localizzata. In caso di indisponibilità del feed originale, la logica preesistente di fallback Google News utilizza la lingua tedesca.

Sono inclusi test di regressione per la copertura delle 240 traduzioni dell’audiodescrizione, la parità dei segnaposto, l’esatta corrispondenza di tutti i 14 feed e il collegamento al selettore delle notizie. È stato aggiornato anche il test generico sui cataloghi di audiodescrizione, da sette a otto lingue.

Le modifiche della fase 1 sono mantenute integralmente: catalogo tedesco principale (529 voci), localizzazione del menu macOS, impostazioni, calendario e trascrizione multimediale.

## Verifiche

Test eseguiti con `python -m pytest -q test_german_phase1.py test_german_phase2.py test_interface_language_localization.py scripts/test_audio_description_localization.py`: 14 test superati, 8 verifiche aggiuntive. La compilazione Rust/macOS e le connessioni effettive ai feed in tempo reale non sono state verificate in questo ambiente (manca `rustc` e non è disponibile macOS).

## Ancora da completare nelle fasi successive

La traduzione di tutti i santi e delle citazioni dei 365 giorni del calendario (al momento si usa il testo inglese quando manca il tedesco), il changelog tedesco integrale e alcuni pannelli secondari con testi inseriti direttamente nel codice Rust. Occorre infine una prova funzionale su un Mac reale con VoiceOver.

**Questa è una versione PARZIALE di lavoro, non una traduzione tedesca completa e pronta per la pubblicazione.**
