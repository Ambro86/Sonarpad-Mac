Changelog

Versione 0.5.0 - 12 settembre 2026

Audiodescrizioni, Strumenti e conversione batch

1. Portate su macOS le più recenti migliorie del motore di audiodescrizione della versione Windows, con maggiore robustezza del bridge Gemini, migliore gestione dei segmenti video problematici e controlli più affidabili durante analisi e riesportazione.

2. In “Crea audiodescrizione con IA” è ora possibile scegliere tra “Usa la mia chiave API Gemini” e “Usa il servizio Sonarpad AI”. Le due credenziali restano memorizzate separatamente: cambiare modalità non cancella né la chiave personale né il codice Sonarpad AI.

3. Quando si usa Sonarpad AI viene mostrato anche il credito attuale in un campo di sola lettura, con i controlli per mostrare il codice e richiederne uno nuovo.

4. Se il file contiene più tracce audio, Sonarpad chiede quale traccia analizzare prima di creare l’audiodescrizione. La scelta viene conservata nel progetto ed è rispettata anche nelle operazioni successive.

5. Aggiunta l’opzione per riconoscere i testi importanti presenti sullo schermo e tenerne conto nella generazione delle descrizioni.

6. È ora possibile scegliere facoltativamente di creare anche un video finale con l’audiodescrizione, oltre al normale output audio. Migliorata inoltre la gestione dei contenitori e dei timestamp nei casi in cui sia necessario un formato alternativo.

7. Migliorato il ducking dell’audio originale: l’abbassamento e il ripristino del volume attorno alla voce narrante sono più graduali, con pre-duck e rilascio più morbidi.

8. In “Modifica progetto audiodescrittivo” è stato aggiunto “Rianalizza segmento”. Il comando lavora sulla descrizione selezionata e usa automaticamente l’ultima modalità IA scelta nelle impostazioni globali, come nella versione Windows.

9. L’editor dei progetti permette ora di modificare più descrizioni una dopo l’altra senza applicarle subito. Le modifiche restano in memoria passando tra i segmenti o usando la ricerca; premendo “Applica” vengono controllate e applicate tutte insieme. Se anche una sola descrizione non entra nel silenzio disponibile, non viene applicata nessuna modifica e Sonarpad riporta alla descrizione da correggere.

10. Corretto il percorso di salvataggio quando si crea una seconda audiodescrizione senza chiudere la finestra: scegliendo un nuovo file sorgente viene ora proposto il percorso relativo al nuovo file e non quello usato in precedenza.

11. Aggiunta nelle impostazioni l’opzione “Raggruppa il menu Strumenti per categoria”. Quando è attiva, Strumenti viene organizzato nelle categorie “Lettura e contenuti”, “Multimedia” e “Utilità”; disattivandola torna il menu piatto.

12. Nell’Enciclopedia Treccani è stato rimosso dall’interfaccia accessibile il controllo dei risultati vuoto che compariva prima di effettuare una ricerca. Il controllo viene mostrato solo quando esistono risultati selezionabili.

13. Aggiunto “Converti cartella…” in Strumenti > Multimedia. È possibile convertire in massa un’intera cartella usando gli stessi formati e parametri di “Converti media”, ad esempio convertendo molti file WMA in MP3 con una sola operazione.

14. La conversione di cartelle mostra l’avanzamento file per file, propone la sottocartella “Convertiti”, mantiene i nomi di base dei file, riepiloga eventuali errori e protegge da sovrascritture accidentali o collisioni tra nomi di destinazione.

15. Aggiunta un’impostazione, attiva per impostazione predefinita, che durante l’avanzamento o il riavvolgimento nei contenuti multimediali annuncia sia il tempo corrente sia la durata totale in forma naturale, ad esempio “1 minuto e 10 secondi di 1 ora, 10 minuti e 10 secondi”. Disattivandola, Sonarpad continua ad annunciare soltanto il tempo corrente come nelle versioni precedenti.

16. Durante la riproduzione, Option+I annuncia in qualsiasi momento soltanto la durata totale del contenuto in forma naturale. Il comando funziona indipendentemente dall’impostazione che aggiunge la durata agli annunci di avanzamento e riavvolgimento; per le dirette viene annunciato che il contenuto è in diretta.

Versione 0.4.0 - 3 settembre 2026

Audiodescrizione con IA — nuova funzione principale

- Aggiunto direttamente nel menu Strumenti il modulo “Crea audiodescrizione con IA”. Sonarpad analizza l’audio per individuare gli spazi liberi dai dialoghi, genera le descrizioni con Gemini e usa i motori vocali già presenti nel programma, evitando di parlare sopra le battute.

- Migliorata la sincronizzazione tra ciò che accade nel video e le descrizioni, con controlli automatici sui tempi generati da Gemini.

- “Attiva pause estese” è deselezionata per impostazione predefinita. Può essere attivata nei contenuti con molti dialoghi o poco spazio disponibile per permettere l’inserimento di descrizioni altrimenti troppo lunghe.

- È possibile provare a riconoscere i personaggi e usare i loro nomi. I cataloghi dei personaggi possono essere mantenuti tra gli episodi di una serie per migliorare la continuità.

- È possibile salvare il progetto, modificare in seguito le descrizioni e riesportare senza dover rigenerare tutto con Gemini.

- Se il lavoro viene interrotto, Sonarpad conserva i progressi e permette di continuare l’audiodescrizione. In caso di quota Gemini esaurita è possibile attendere, cambiare modello oppure interrompere senza perdere il lavoro già completato.

- La finestra permette di scegliere lingua, livello di dettaglio, modello Gemini, motore e voce e ricorda le preferenze utilizzate. Il modulo è disponibile nelle lingue supportate da Sonarpad per Mac.

- Durante la generazione l’interfaccia mostra avanzamento, stato corrente e pulsante Annulla; al termine l’MP3 può essere aperto direttamente nel player interno.

- Migliorata la compatibilità con i video MKV: Sonarpad gestisce in modo più affidabile timestamp irregolari o mancanti e, quando possibile, ignora i pacchetti corrotti senza interrompere l’audiodescrizione.

- Corretto un problema che poteva far fallire l’esportazione finale in MP3 con video contenenti audio multicanale, ad esempio Dolby 5.1. Sonarpad converte automaticamente l’audio multicanale in stereo quando necessario per la codifica MP3.

- Quando il video contiene più tracce audio, prima di iniziare Sonarpad chiede quale traccia utilizzare. La casella combinata accessibile si può cambiare con le frecce; OK avvia l’audiodescrizione con la traccia scelta, mentre Annulla chiude la finestra e riporta il focus all’editor di Sonarpad.

- Aggiunta la casella “Mostra chiave API” accanto alla chiave Gemini. La chiave resta nascosta per impostazione predefinita e viene mostrata solo temporaneamente quando la casella è attiva; riaprendo la finestra torna sempre nascosta.

Audiodescrizioni e contenuti online

- “Crea audiodescrizione con IA” è disponibile anche per i video aperti tramite streaming e per i contenuti on demand compatibili di RaiPlay, Rai Audiodescrizioni e La7 Play.

YouTube e streaming

- Migliorata sensibilmente l’esperienza di YouTube, velocizzando la ricerca e la navigazione e ripristinandone il corretto funzionamento.

- Le opzioni di qualità video sono ora localizzate: al posto del valore tecnico “best” viene mostrata una voce comprensibile nella lingua dell’interfaccia, ad esempio “Migliore” in italiano.

- Sonarpad ricorda l’ultimo formato scelto in Salva media. Se, ad esempio, si sceglie MP4, alla successiva apertura MP4 rimane preselezionato.

La7 Play

- Aggiunto il modulo La7 Play, che permette di cercare e riascoltare i programmi già andati in onda.

Ringraziamenti

- Un grande ringraziamento a Leonardo Graziano e Tiziano Ferraro, che hanno testato a fondo la funzione Audiodescrizione con IA e Sonarpad in generale, contribuendo in modo prezioso al suo miglioramento.

- Un grande ringraziamento anche al gruppo Tecnologia Accessibile per il supporto, i test e i suggerimenti.

Versione 0.3.1 - 16 luglio 2026

- Corretto un problema che impediva l’avvio di Sonarpad quando il menu Radio conteneva preferiti, a causa di identificatori di menu non validi in wxWidgets.

- Sonarpad è ora disponibile anche in francese, spagnolo, portoghese, ceco e polacco, oltre che in italiano e inglese.

- Aggiunta un’impostazione separata per la Lingua notizie. Questa impostazione è indipendente dalla lingua dell’interfaccia e permette di utilizzare fonti e servizi dedicati alla lingua scelta.

- Aggiunta la Biblioteca digitale di BDCiechi. Gli utenti iscritti possono accedere con le proprie credenziali, consultare le ultime novità o il catalogo completo, cercare un libro, leggere un testo d’assaggio e salvarlo sul Mac per aprirlo con Sonarpad.

- Aggiunta la funzione Meteo, che permette di cercare una città e conoscere la situazione attuale, le temperature, le precipitazioni, il vento e l’umidità, oltre alle previsioni per oggi, domani o un altro giorno.

- Aggiunta la sezione Film al cinema, con i film attualmente nelle sale, le prossime uscite, la trama, la data di uscita e, quando disponibile, il collegamento al trailer.

- Aggiunto un calendario accessibile nel menu Strumenti. È possibile scegliere qualsiasi giorno, consultarne festività, santo e frase del giorno, creare promemoria e aggiungere appuntamenti direttamente al Calendario di macOS.

- Aggiunta la funzione Ricerca percorsi, che permette di calcolare un itinerario a piedi, in bicicletta, in automobile o in sedia a rotelle, scegliendo il percorso più veloce o quello più corto. Sonarpad mostra distanza, durata e indicazioni dettagliate.

- Aggiunta la funzione Converti media, per convertire file audio e video in numerosi formati, tra cui MP3, M4A, M4B, MP4, AVI, MOV, Opus, OGG, FLAC, WAV e AIFF. È inoltre possibile creare un video partendo da un file audio e da un’immagine.

- Aggiunto il Dizionario vocale. È possibile indicare parole o espressioni che la sintesi vocale deve sostituire durante la lettura, così da correggere pronunce errate, abbreviazioni o nomi particolari.

- Aggiunta la ricerca nel vocabolario e nell’enciclopedia Treccani. È possibile cercare una voce, leggerne l’anteprima e importare nell’editor l’intero contenuto oppure soltanto una delle sue sezioni. La funzione è disponibile con l’interfaccia italiana.

- Aggiunta la ricerca accessibile nelle Pagine Bianche e nelle Pagine Gialle. La funzione è disponibile con l’interfaccia italiana.

- Ampliata la sezione Articoli con i pulsanti Articoli recenti e Condividi, per ritrovare rapidamente gli ultimi contenuti letti e condividerli tramite i servizi disponibili su macOS.

- Aggiunte nel menu Articoli le funzioni Aggiungi testata alla comunità di Sonarpad e Testate della comunità di Sonarpad. È possibile proporre un feed RSS o il sito di una testata e importare le fonti condivise dagli altri utenti. Le testate vengono aggiunte e mostrate in base alla Lingua notizie selezionata.

- Migliorata la gestione delle fonti notizie: cambiando la Lingua notizie vengono caricate le fonti predefinite appropriate, senza eliminare le fonti personali aggiunte dall’utente.

- Ampliata la ricerca delle radio con la consultazione per lingua, nazione e città e con i nomi completi e localizzati dei paesi.

- Aggiunta la possibilità di inviare una radio alla comunità Sonarpad, specificandone nome, indirizzo, lingua e genere.

- Aggiunte la registrazione e la programmazione delle radio. Le nuove azioni sono disponibili sia nei risultati della ricerca sia nei preferiti, e le registrazioni vengono salvate direttamente in formato MP3. Dopo aver aperto una radio, è possibile avviare la registrazione anche premendo la lettera R.

- Ampliata la sezione TV con le funzioni Riproduci e registra e Programma registrazione. Le registrazioni programmate utilizzano un LaunchAgent di macOS e possono quindi avviarsi automaticamente anche quando Sonarpad è chiuso. Dopo aver aperto un canale TV, è possibile avviare la registrazione anche premendo la lettera R.

- Migliorata la sicurezza delle registrazioni TV: il flusso viene prima conservato nel formato originale e, al termine, Sonarpad prova a convertirlo automaticamente in MP4. Se la conversione non riesce, la registrazione originale non viene perduta.

- Migliorate la guida TV, l’indicazione del programma ora in onda, il catalogo dei canali e la compatibilità della riproduzione su macOS.

- Aggiunto nel menu File l’elenco dei documenti di testo aperti recentemente, per riaprirli più velocemente.

- Aggiunta la Modalità sola lettura, utile per consultare un documento evitando modifiche accidentali.

- Aggiunto il Sommario del libro per i file EPUB che contengono un indice. È possibile scegliere un capitolo e raggiungerlo direttamente.

- Aggiunta la possibilità di scegliere tra le voci Microsoft ad alta qualità e le voci di sistema di macOS.

- Aggiunta l’opzione per ignorare durante la lettura le pause causate dalle righe vuote.

- Aggiunta nelle impostazioni la possibilità di scegliere di quanti secondi avanzare o tornare indietro durante la riproduzione dei contenuti multimediali.

- Migliorata l’accessibilità delle finestre, dei menu e dei controlli, con una gestione più coerente del focus, del tasto Invio, del tasto Esc e delle scorciatoie da tastiera.

- Migliorata la localizzazione dei messaggi, dei pulsanti e delle finestre di conferma in tutte le lingue supportate.

- Corretto un problema per cui i file multimediali non venivano visualizzati nella finestra video del player.

- Risolti numerosi problemi relativi alla stabilità, alla riproduzione dei contenuti, alle registrazioni programmate, alla gestione delle fonti e alla compilazione su macOS.

- Si ringraziano Leonardo Graziano, Luca Maianti e il gruppo italiano Tecnologia Accessibile per il supporto continuo e il costante beta testing.


Versione 0.2.9 - 1 maggio 2026
- Estese le funzionalità YouTube anche per i Mac Intel e Catalina.
- Velocizzata enormemente la ricerca YouTube.
- Migliore gestione dei risultati YouTube, con canali e playlist inseriti all'inizio.
- Aggiunta la possibilità di aggiungere e rimuovere canali e playlist dai preferiti.
- Aggiunto nelle opzioni il pulsante Anteprima voce.
- Aggiunto in Rimuovi fonti il pulsante Seleziona tutto.
- Aggiunta una barra di progresso per la ricerca da Wikipedia.
- Aggiunto il canale TV Videolina.
- Spostate le voci del menu per le funzionalità aggiuntive in Strumenti, per allineare Sonarpad alla versione per Windows.
- Corretto il comportamento per cui a volte nelle TV non venivano mostrati i programmi ora in onda.
- Aggiunti numerosi canali TV, organizzando la finestra in categorie per una maggiore facilità di consultazione. È stato anche aggiunto un campo di ricerca che mostra i risultati della TV desiderata.

Versione 0.2.8 - 29 aprile 2026
- Aggiunto il menu Strumenti con due nuove voci: Cerca e importa da Wikipedia e Riproduci audio da streaming.
- Cerca e importa da Wikipedia permette di cercare e importare articoli, leggerli e salvarli come audiolibri.
- Riproduci audio da streaming permette di riprodurre contenuti in streaming, ad esempio da YouTube.
- Nella casella di ricerca dello streaming si può digitare qualunque contenuto: il programma lo cercherà e potrà aprire anche canali e playlist.
- La ricerca da YouTube non è abilitata sui Mac Intel per motivi di incompatibilità.
- Si ringrazia per il supporto continuo Leonardo Graziano.
- Per le radio è stato aggiunto un pulsante per andare direttamente alla pagina selezionata nei risultati, senza dover usare ogni volta Vai alla pagina successiva.
- Esteso il segnalibro automatico anche ai file di testo.
- Corretto un problema per cui a volte le audiodescrizioni non venivano salvate per problemi di timeout.
- Aggiunta la possibilità di impostare delle TV preferite.
- Nella lista dei canali TV è stata aggiunta l'indicazione del programma ora in onda.
- Inserita una guida TV completa, consultabile dal giorno precedente fino a cinque giorni dopo la data corrente.

Versione 0.2.7 - 28 aprile 2026
- Migliorato il supporto per i file con diacritici e con codifiche diverse da UTF-8 (incluso il supporto per caratteri cinesi e altre lingue internazionali).
- Corretto il problema per cui la virgola, digitata in un campo di testo, apriva erroneamente le impostazioni.
- Migliorata la rapidità di lettura: ora anche gli articoli lunghi vengono letti più velocemente ed è stata rimossa la pausa dopo i paragrafi.
- Aggiunta la possibilità di aprire con Sonarpad anche file JPG e formati simili, così da poter eseguire l'OCR anche sugli articoli inviati come immagini o fotografie.
- Aggiunta la possibilità di impostare Sonarpad come programma predefinito.
- Da ora Sonarpad può aprire non solo file di testo, ma anche file audio e video, utilizzando il player MPV.
- Aggiunta nelle opzioni la funzione di segnalibro automatico: se si chiude un file, un podcast o un qualsiasi contenuto multimediale, questo verrà riaperto dall'esatta posizione in cui era stato lasciato.
- Da ora le radio non vengono più aperte in Safari, ma vengono riprodotte direttamente tramite il player di Sonarpad.
- Da questa versione l'app è firmata e non richiede più alcuna autorizzazione da parte dell'utente, rendendo l'installazione più semplice.
- Inserito un aggiornamento automatico del programma che controlla, scarica e aggiorna automaticamente Sonarpad.
- Inseriti i moduli aggiuntivi di RaiPlay, Rai Audiodescrizioni, RaiPlay Sound e canali TV. Per utilizzarli sarà necessario richiedere un codice all'autore.
- Per ottenere il codice, seguire la procedura indicata dal programma e inviare la mail generata, assicurandosi che sia effettivamente presente nella posta inviata. Se la procedura viene eseguita correttamente, il codice verrà ricevuto entro circa un minuto.
- Il codice va inserito aprendo le impostazioni con Command + , e spostandosi con VO + freccia destra fino al campo Codice Sonarpad per funzionalità aggiuntive.
- Nota: se aprendo una funzionalità aggiuntiva, ad esempio RaiPlay, compare un errore, significa probabilmente che il codice non è stato copiato integralmente.
- Nei moduli Rai sono state aggiunte la ricerca e la consultazione dei contenuti, che vengono riprodotti tramite il player di Sonarpad.

Versione 0.2.6
- Corretto un bug di wx/macOS per cui all'avvio poteva comparire un errore e sono stati stabilizzati i menu collegati.
- Corretta la scorciatoia Cmd+, per il menu Impostazioni anche quando il focus si trova su editor e controlli.
- Quando si salva un audiolibro il focus viene ora posizionato correttamente sul campo di testo e i nomi file con il punto non vengono più tagliati.
- Aggiunto supporto agli OPML di Lire con divisione in cartelle: le cartelle si aprono come sottomenu e le singole fonti in una finestrella dedicata.
- Il riordino delle fonti articoli gestisce ora il nuovo sistema di cartelle con pulsanti Apri cartella, Cartella principale, Sposta in cartella e Sposta fuori dalle cartelle.

Versione 0.2.5
- Nuove finestre di salvataggio personalizzate per testo e audiolibri su macOS.
- I campi nome file ora accettano correttamente Cmd+V, Cmd+A e gli altri comandi di editing.
- Il programma ricorda ultima cartella e ultimo formato usati per salvataggio testo e audiolibri.
- Aggiunto il salvataggio audiolibri anche in formato M4A e WAV.
- Aggiunto il menu Radio con ricerca per lingua, aggiunta ai preferiti, aggiunta manuale di una stazione e modifica e riordino dei preferiti.
- Migliorata la gestione delle fonti articoli inserite come siti: scoperta del feed dalla pagina e correzione del feed commenti.
- Workflow release macOS aggiornato per includere anche l'artifact Catalina.

Versione 0.2.4
- Miglioramenti importanti all'OCR PDF su macOS con passaggio a pdfium e fallback più robusti.
- Aggiunto export M4B su macOS e affinato il salvataggio testo.
- Migliorata la gestione delle fonti articoli e la protezione del refresh quando una fonte restituisce zero elementi.
- Ottimizzata la sintesi Edge TTS con chunking e retry più affidabili.
- Aggiunta e poi raffinata la pipeline Catalina per build e packaging macOS.

Versione 0.2.2
- Migliorato il caricamento dei PDF su macOS con feedback più chiaro e dialogo finale esplicito.
- Ordinamento alfabetico delle fonti articoli.
- Riparazioni al testo PDF e miglioramenti generali di localizzazione.

Versione 0.2.1
- Stabilizzati shortcut e menu macOS per avvio, pausa, stop e salvataggio.
- Migliorata l'apertura esterna degli episodi podcast su macOS.
- Corretta la persistenza delle impostazioni macOS.
- Rafforzate le workflow di build Intel/macOS e la gestione di Xcode.

Versione 0.2.0
- Prima release macOS di Sonarpad Per Mac.
- Supporto lettura testo, articoli e podcast con sintesi vocale.
- Supporto PDF OCR su macOS, download aggiornamenti e pacchetti DMG dedicati.
- Categorie podcast gerarchiche e primi shortcut globali/macOS.
