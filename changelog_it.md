Changelog

Versione 0.2.9 - 30 aprile 2026
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
- Aggiunti numerosi canali TV, organizzando la finestra in categorie, per maggior facilità di consultazione. E' stato anche aggiunto un campo di ricerca che mostra i risultati della tv desiderata.

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
