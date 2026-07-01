- Zkratka Nedávné články na Macu změněna na Command+spojovník.
- Standardní dialogy Ano/Ne nahrazeny lokalizovanými vlastními dialogy.
- V Digitální knihovně klávesa Enter ve vyhledávacím poli spustí hledání.
- Vyhledávání rádií: přidán filtr podle země s úplnými názvy zemí.

Seznam změn

Verze 0.2.9 - 1. května 2026
- Funkce YouTube byly rozšířeny také na Macy Intel a Catalina.
- Vyhledávání na YouTube bylo výrazně zrychleno.
- Byla vylepšena správa výsledků YouTube, kanály a playlisty se nyní zobrazují na začátku.
- Byla přidána možnost přidávat a odebírat kanály a playlisty z oblíbených.
- V možnostech bylo přidáno tlačítko Náhled hlasu.
- Při odebírání zdrojů bylo přidáno tlačítko Vybrat vše.
- Byl přidán ukazatel průběhu pro vyhledávání ve Wikipedii.
- Byl přidán televizní kanál Videolina.
- Položky nabídky pro doplňkové funkce byly přesunuty do Nástrojů, aby byl Sonarpad sladěn s verzí pro Windows.
- Bylo opraveno chování, kdy se někdy u TV nezobrazovaly právě vysílané pořady.
- Bylo přidáno mnoho televizních kanálů a okno bylo uspořádáno do kategorií pro snazší procházení. Bylo přidáno také vyhledávací pole, které zobrazuje výsledky požadované TV.

Verze 0.2.8 - 29. dubna 2026
- Byla přidána nabídka Nástroje se dvěma novými položkami: Vyhledat a importovat z Wikipedie a Přehrát streamované audio.
- Vyhledat a importovat z Wikipedie umožňuje vyhledávat a importovat články, číst je a ukládat jako audioknihy.
- Přehrát streamované audio umožňuje přehrávat streamovaný obsah, například z YouTube.
- Do vyhledávacího pole streamingu lze zadat jakýkoli obsah: program jej vyhledá a může otevřít také kanály a playlisty.
- Vyhledávání na YouTube není na Macích Intel povoleno z důvodu nekompatibility.
- Zvláštní poděkování patří Leonardu Grazianovi za trvalou podporu.
- U rádií bylo přidáno tlačítko pro přímý přechod na vybranou stránku výsledků, bez nutnosti opakovaně používat Přejít na další stránku.
- Automatická záložka byla rozšířena také na textové soubory.
- Byl opraven problém, kdy se audiopopisy někdy neuložily kvůli časovým limitům.
- Byla přidána možnost nastavit oblíbené TV.
- Do seznamu televizních kanálů byla přidána informace o právě vysílaném pořadu.
- Byl přidán kompletní televizní program, dostupný od předchozího dne až do pěti dnů po aktuálním datu.

Verze 0.2.7 - 28. dubna 2026
- Byla vylepšena podpora souborů s diakritikou a kódováním odlišným od UTF-8, včetně podpory čínských znaků a dalších mezinárodních jazyků.
- Byl opraven problém, kdy čárka zadaná do textového pole chybně otevřela možnosti.
- Byla zlepšena rychlost čtení: dlouhé články se nyní čtou rychleji a pauza po odstavcích byla odstraněna.
- Byla přidána možnost otevírat v Sonarpadu soubory JPG a podobné formáty, aby bylo možné provádět OCR také u článků poslaných jako obrázky nebo fotografie.
- Byla přidána možnost nastavit Sonarpad jako výchozí program.
- Sonarpad nyní může otevírat nejen textové soubory, ale také audio a video soubory pomocí přehrávače MPV.
- V možnostech byla přidána funkce automatické záložky: pokud zavřete soubor, podcast nebo jakýkoli multimediální obsah, bude znovu otevřen přesně v místě, kde jste skončili.
- Rádia se již neotevírají v Safari, ale přehrávají se přímo prostřednictvím přehrávače Sonarpad.
- Od této verze je aplikace podepsaná a již nevyžaduje žádné povolení uživatele, což zjednodušuje instalaci.
- Byla přidána automatická aktualizace programu, která automaticky kontroluje, stahuje a aktualizuje Sonarpad.
- Byly přidány doplňkové moduly RaiPlay, audiopopisy Rai, RaiPlay Sound a televizní kanály. Pro jejich používání je nutné vyžádat si kód od autora.
- Pro získání kódu postupujte podle pokynů programu a odešlete vygenerovaný e-mail; ujistěte se, že je skutečně ve složce odeslané pošty. Pokud je postup proveden správně, kód obdržíte přibližně do jedné minuty.
- Kód se zadává otevřením možností pomocí Command + , a přesunem pomocí VO + šipka doprava na pole Kód Sonarpad pro doplňkové funkce.
- Poznámka: pokud se při otevření doplňkové funkce, například RaiPlay, zobrazí chyba, pravděpodobně to znamená, že kód nebyl zkopírován celý.
- V modulech Rai bylo přidáno vyhledávání a procházení obsahu, který se přehrává pomocí přehrávače Sonarpad.

Verze 0.2.6
- Byla opravena chyba wx/macOS, která mohla při spuštění zobrazit chybu, a byly stabilizovány související nabídky.
- Byla opravena klávesová zkratka Cmd+, pro nabídku Možnosti i tehdy, když je fokus v editoru nebo na ovládacích prvcích.
- Při ukládání audioknihy je nyní fokus správně umístěn do textového pole a názvy souborů s tečkou se již nezkracují.
- Byla přidána podpora OPML z Lire s rozdělením do složek: složky se otevírají jako podnabídky a jednotlivé zdroje v samostatném okně.
- Přeskupování zdrojů článků nyní podporuje nový systém složek s tlačítky Otevřít složku, Hlavní složka, Přesunout do složky a Přesunout ze složek ven.

Verze 0.2.5
- Nová vlastní okna pro ukládání textu a audioknih v macOS.
- Pole názvu souboru nyní správně přijímají Cmd+V, Cmd+A a další editační příkazy.
- Program si pamatuje poslední složku a poslední formát použité pro ukládání textu a audioknih.
- Bylo přidáno ukládání audioknih také ve formátech M4A a WAV.
- Byla přidána nabídka Rádio s vyhledáváním podle jazyka, přidáním do oblíbených, ručním přidáním stanice a úpravou a řazením oblíbených.
- Byla vylepšena správa zdrojů článků přidaných jako weby: vyhledání feedu ze stránky a oprava feedu komentářů.
- Vydávací workflow macOS bylo aktualizováno tak, aby zahrnovalo také artefakt Catalina.

Verze 0.2.4
- Významná vylepšení OCR PDF v macOS díky přechodu na pdfium a robustnějším záložním mechanismům.
- Byl přidán export M4B v macOS a bylo doladěno ukládání textu.
- Byla zlepšena správa zdrojů článků a ochrana obnovování, když zdroj vrátí nula položek.
- Byla optimalizována syntéza Edge TTS pomocí spolehlivějšího dělení na bloky a opakování pokusů.
- Byla přidána a doladěna pipeline Catalina pro sestavení a balení macOS.

Verze 0.2.2
- Bylo vylepšeno načítání PDF v macOS s jasnější zpětnou vazbou a výslovným závěrečným dialogem.
- Abecední řazení zdrojů článků.
- Opravy textu PDF a obecná vylepšení lokalizace.

Verze 0.2.1
- Byly stabilizovány zkratky a nabídky macOS pro spuštění, pauzu, zastavení a uložení.
- Bylo vylepšeno externí otevírání epizod podcastů v macOS.
- Bylo opraveno ukládání nastavení macOS.
- Byla posílena workflow sestavení Intel/macOS a správa Xcode.

Verze 0.2.0
- První macOS verze Sonarpad pro Mac.
- Podpora čtení textu, článků a podcastů se syntézou řeči.
- Podpora OCR PDF v macOS, stahování aktualizací a vyhrazené balíčky DMG.
- Hierarchické kategorie podcastů a první globální/macOS zkratky.
- Bezpečnější nahrávání TV na Macu: živý stream se nahrává jako TS a po zastavení se jej Sonarpad pokusí automaticky převést do MP4; pokud převod selže, soubor TS zůstane zachován.
- Vyhledávání rádií podle země: názvy zemí se nyní lokalizují pomocí i18n-country-translations/CLDR místo ruční tabulky.
- Diagnostika TV na Macu: přidány rozšířené logy mpv, stav okna/videa, možnosti výstupu, výpisy logu mpv a diagnostické snímky pro zjištění příčiny prázdného okna videa.

- TV na Macu: katalog kanálů nyní používá pouze vzdálený katalog Sonarpad se stejnými hlavičkami a opravami La7/La7D jako mobilní verze. Přidán test spuštění mpv přes LaunchServices pro diagnostiku/opravu prázdného bílého okna videa.
