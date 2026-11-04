# Sonarpad per macOS – Revisione della lingua tedesca, fase 4 (parziale)

Questa fase modifica esclusivamente i sorgenti dell'archivio ZIP della fase 3 e **non** usa il bridge locale o modifica il repository remoto.

## Interventi effettuati

Il catalogo compilato `i18n/wxstd_de.mo` era già caricato da `src/main.rs`, ma aveva soltanto 21 traduzioni effettive per voci prevalentemente del menu nativo macOS. È stato ampliato a **177 voci tradotte** (oltre all'intestazione) con comandi comuni di finestre e selettori di file, fra cui Apri, Salva, Annulla, Sì, No, Chiudi, pulsanti, selettori di file e messaggi standard. È un catalogo GNU gettext `.mo` valido. La compilazione dell'interfaccia `.mo` è stata verificata mediante `gettext.GNUTranslations`, ma **non rappresenta l'intero catalogo ufficiale wxWidgets**.

Sono state collegate al tedesco tutte le 30 etichette del modulo SonarMusic. Nel modulo principale sono state completate 28 descrizioni meteorologiche e introdotte diramazioni tedesche in dialoghi di pianificazione delle registrazioni radio e TV, messaggi lettore MPV, notifiche del lettore, ricerca radio, guida TV, preferiti, categorie dei canali, dettagli di cinema, messaggi di anteprima delle voci, ricerca YouTube, lettura e gestione degli RSS e podcast e messaggi di altre finestre che prima restavano in inglese.

I filtri nelle finestre Apri e Salva ora traducono in tedesco alcune descrizioni dei tipi di file mantenendo invariati i pattern (ad esempio `*.*`), sia nel modulo principale sia in audiodescrizione, audiolibri in serie, unione media e trascrizione. Tradotti anche vari comandi, messaggi e dettagli dei servizi elenchi telefonici/Pagine Bianche-Gialle e la paginazione di BDCiechi.

I cataloghi delle fasi 1–3, le notizie, i santi, le citazioni del calendario e il changelog tedesco sono preservati.

## Verifiche

`python -m pytest -q test_german_phase1.py test_german_phase2.py test_german_phase3.py test_german_phase4.py test_interface_language_localization.py scripts/test_audio_description_localization.py`: 28 test superati e 8 sottotest aggiuntivi superati. I test della fase 4 verificano la validità del `.mo`, le etichette SonarMusic, i codici meteo, le varianti dei dialoghi, i filtri dei selettori file, le directory e la parità dei cataloghi principali. Archivio ZIP verificato con `testzip()`.

## Limiti e lavoro residuo

**La fase 4 rimane una revisione statica parziale, non una certificazione di localizzazione integrale di ogni finestra**. Non è disponibile qui il compilatore Rust/macOS né una sessione macOS con VoiceOver. Non è stato possibile importare l'intero catalogo tedesco ufficiale di wxWidgets: il `.mo` ampliato copre 177 voci note, non necessariamente tutti i messaggi e comandi dei dialoghi nativi. Inoltre, moduli meno frequenti, percorsi d'errore, output dinamici e funzionalità condizionate all'italiano possono conservare testi di origine. Serve una compilazione macOS e un'esplorazione sistematica di ciascuna funzione, sia con VoiceOver sia senza, per dichiarare completata la verifica al 100%. La nota sulla traduzione tedesca nel changelog 0.5.3 è mantenuta come richiesto dall'utente.
