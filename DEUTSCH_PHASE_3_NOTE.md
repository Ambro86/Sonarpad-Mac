# Sonarpad per macOS – Traduzione tedesca, fase 3

Questa fase è stata eseguita interamente sullo ZIP della fase 2, senza utilizzare il bridge locale. L'archivio include anche tutti i sorgenti e le modifiche delle fasi 1 e 2.

## Novità della fase 3

Il calendario ha ora traduzioni tedesche per 365 voci di santi e ricorrenze e per tutte le 128 citazioni originali, conservando le altre lingue. I nomi dei santi vengono derivati principalmente dalla voce italiana, perché alcune voci inglesi nel dataset originale non corrispondono alla ricorrenza italiana. Sono state riviste manualmente varie festività e denominazioni.

Creato `changelog_de.md`: traduzione di tutte e 14 le sezioni di versione del changelog inglese (da 0.2.0 a 0.5.3), mantenendo il numero dei punti e delle voci esistenti. Nel punto 18 della versione 0.5.3 è stata aggiunta, come richiesto, la nota relativa alla localizzazione tedesca; tale punto è stato aggiunto anche ai changelog italiani, inglesi, francesi, spagnoli, portoghesi, cechi e polacchi. Il codice di Sonarpad ora carica il changelog tedesco se la lingua impostata è `de`.

Il modulo Taglia file media include 99 etichette/messaggi in tedesco, con un catalogo dedicato `i18n/media_cutter_de.json` e unità di durata localizzate. Le finestre Audiolibri in serie, Unisci media e Registra podcast ora caricano `i18n/ui_de.json` anziché il catalogo inglese.

Aggiunte 19 categorie Apple Podcast in tedesco senza cambiare gli ID delle categorie. La ricerca di percorsi invia `de` per la lingua e `DEU` per la preferenza nazionale. Tradotte 39 etichette/messaggi ancora incorporati in `src/main.rs`, soprattutto radio, risultati, registrazioni e dizionario di pronuncia; aggiunti i messaggi tedeschi principali di BDCiechi.

Le 529 stringhe principali, 240 stringhe di audiodescrizione, trascrizione, menu nativi macOS, notizie in tedesco e 14 fonti RSS indicate dall'utente restano quelle delle fasi precedenti.

## Verifiche eseguite

`python -m pytest -q test_german_phase1.py test_german_phase2.py test_german_phase3.py test_interface_language_localization.py scripts/test_audio_description_localization.py`: 19 test riusciti e 8 subtest riusciti.

Il test specifico della fase 3 controlla la copertura delle 365 voci e delle 128 citazioni, la parità delle 99 etichette del taglio media, la presenza delle 19 categorie podcast, i collegamenti alle traduzioni e la copertura storico-strutturale del changelog tedesco. L'archivio ZIP è verificato per integrità.

## Limiti di verifica

Non è stato possibile compilare con Rust né provare il programma su un Mac con VoiceOver. Alcuni messaggi diagnostici, contenuti dinamici provenienti dai servizi e testi specifici di moduli non ancora coperti da cataloghi possono ancora apparire nella lingua di origine: la dichiarazione di traduzione integrale nel changelog è la nota di rilascio richiesta, non una certificazione di revisione linguistica e funzionale completa di ogni percorso dell'applicazione. Consigliata una verifica su Mac prima della distribuzione definitiva.
