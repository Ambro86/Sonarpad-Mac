Seznam změn

Verze 0.5.0 - 12. září 2026

Audiopopis, Nástroje a dávkový převod

1. Do macOS byla přenesena nejnovější vylepšení enginu audiopopisu z verze pro Windows, včetně odolnějšího propojení s Gemini, lepšího zpracování problematických video segmentů a spolehlivějších kontrol během analýzy a opětovného exportu.

2. Ve funkci „Vytvořit audiopopis s AI“ lze nyní zvolit „Použít můj API klíč Gemini“ nebo „Použít Sonarpad AI“. Obě přihlašovací údaje se ukládají odděleně, takže změna režimu nesmaže osobní klíč ani kód Sonarpad AI.

3. Při použití Sonarpad AI se aktuální kredit zobrazuje v poli pouze pro čtení spolu s ovládacími prvky pro zobrazení kódu a vyžádání nového.

4. Pokud soubor obsahuje více zvukových stop, Sonarpad se před vytvořením audiopopisu zeptá, kterou stopu má analyzovat. Vybraná stopa se uloží do projektu a použije se i při dalších operacích.

5. Byla přidána možnost rozpoznávat důležité texty zobrazené na obrazovce a zohlednit je při vytváření popisů.

6. Vytváření audiopopisu může nyní volitelně vytvořit také výsledné video obsahující audiopopis, vedle běžného zvukového výstupu. Zlepšena byla také práce s kontejnery a časovými značkami, pokud je nutný alternativní výstupní formát.

7. Bylo vylepšeno ducking původního zvuku: zeslabení a návrat hlasitosti kolem vyprávění jsou plynulejší, s jemnějším pre-duckem a uvolněním.

8. Do editoru projektu audiopopisu byla přidána funkce „Znovu analyzovat segment“. Pracuje s aktuálně vybraným popisem a automaticky používá naposledy zvolený globální režim AI stejně jako ve Windows.

9. Editor projektu nyní umí uchovat čekající změny několika popisů současně. Koncepty zůstávají zachovány při přechodu mezi segmenty i při vyhledávání; po volbě „Použít“ jsou všechny společně zkontrolovány a použity. Pokud se byť jediný popis nevejde do dostupného ticha, nepoužije se žádná změna a Sonarpad se vrátí k popisu, který je třeba opravit.

10. Opravena cesta pro uložení při vytváření druhého audiopopisu bez zavření okna: po výběru nového zdrojového souboru se nyní vytvoří cesta pro nový soubor místo zachování předchozí.

11. Do nastavení byla přidána volba „Seskupit nabídku Nástroje podle kategorií“. Je-li zapnutá, Nástroje jsou rozděleny na „Čtení a obsah“, „Multimédia“ a „Utility“; po vypnutí se obnoví plochá nabídka.

12. V encyklopedii Treccani byl z přístupného rozhraní odstraněn prázdný ovládací prvek výsledků, který se zobrazoval ještě před vyhledáním. Nyní se zobrazí pouze tehdy, když existují výsledky k výběru.

13. Do Nástroje > Multimédia byla přidána funkce „Převést složku…“. Celou složku lze dávkově převést pomocí stejných formátů a nastavení jako ve funkci „Převést média“, například více souborů WMA do MP3 v jediné operaci.

14. Převod složky zobrazuje průběh po jednotlivých souborech, navrhuje podsložku „Převedeno“, zachovává základní názvy souborů, shrnuje případné chyby a chrání před nechtěným přepsáním a kolizemi cílových názvů.

15. Bylo přidáno nastavení, které je ve výchozím stavu zapnuté a při posunu vpřed nebo vzad v médiích oznamuje jak aktuální pozici, tak celkovou délku v přirozeném formátu, například „1 minuta a 10 sekund z 1 hodiny, 10 minut a 10 sekund“. Po vypnutí Sonarpad nadále oznamuje pouze aktuální pozici stejně jako v předchozích verzích.

16. Během přehrávání klávesová zkratka Option+I kdykoli oznámí pouze celkovou délku média v přirozeném formátu. Zkratka funguje nezávisle na nastavení, které přidává celkovou délku k oznámení při posunu; u živého vysílání Sonarpad oznámí, že jde o živý přenos.

Verze 0.4.0 - 3. září 2026

Audiopopis s AI — nová hlavní funkce

- Do nabídky Nástroje byla přímo přidána funkce „Vytvořit audiopopis s AI“. Sonarpad analyzuje zvuk, vyhledá místa bez dialogů, vytvoří popisy pomocí Gemini a použije již dostupné hlasové moduly, aniž by mluvil přes dialogy.

- Byla zlepšena synchronizace mezi děním ve videu a popisy a časy vytvořené Gemini jsou automaticky kontrolovány.

- „Povolit rozšířené pauzy“ je ve výchozím nastavení vypnuto. Lze je zapnout u obsahu s mnoha dialogy nebo malým volným prostorem, aby bylo možné vložit delší popisy.

- Sonarpad se může pokusit rozpoznat postavy a používat jejich jména. Katalogy postav lze zachovat mezi epizodami seriálu pro lepší kontinuitu.

- Projekt lze uložit, později upravit popisy a znovu exportovat bez nutnosti vše znovu generovat pomocí Gemini.

- Pokud je proces přerušen, Sonarpad zachová průběh a umožní v audiopopisu pokračovat. Při vyčerpání kvóty Gemini lze čekat, změnit model nebo práci ukončit bez ztráty již dokončené části.

- V okně lze zvolit jazyk, úroveň podrobnosti, model Gemini, hlasový modul a hlas a použité nastavení se pamatuje. Modul je dostupný v jazycích podporovaných Sonarpadem pro Mac.

- Během generování rozhraní zobrazuje průběh, aktuální stav a Zrušit; po dokončení lze MP3 otevřít přímo v interním přehrávači.

- Zlepšena kompatibilita s videi MKV: Sonarpad spolehlivěji zpracovává nepravidelné nebo chybějící časové značky a, pokud je to možné, přeskočí poškozené pakety bez přerušení audiopopisu.

- Opraven problém, který mohl způsobit selhání závěrečného exportu do MP3 u videí s vícekanálovým zvukem, například Dolby 5.1. Sonarpad nyní automaticky převádí vícekanálový zvuk na stereo, pokud je to nutné pro kódování MP3.

- Pokud video obsahuje více zvukových stop, Sonarpad se před zpracováním zeptá, kterou stopu použít. Přístupný rozbalovací seznam lze měnit šipkami; OK spustí audiopopis s vybranou stopou, zatímco Zrušit zavře okno a vrátí fokus do editoru Sonarpadu.

- Vedle klíče Gemini bylo přidáno zaškrtávací políčko „Zobrazit klíč API“. Klíč je ve výchozím nastavení skrytý a zobrazí se pouze dočasně po zapnutí této volby; po opětovném otevření okna je znovu skrytý.

YouTube a streamování

- Výrazně vylepšeno používání YouTube: vyhledávání a navigace jsou rychlejší a bylo obnoveno správné fungování.

- Volby kvality videa jsou nyní přeložené: místo technické hodnoty „best“ Sonarpad zobrazuje srozumitelný popisek v jazyce rozhraní.

- Sonarpad si pamatuje poslední formát zvolený v Uložit médium. Pokud je například zvoleno MP4, při příštím otevření dialogu zůstane MP4 předvybráno.


Poděkování

- Velké poděkování patří Leonardu Grazianovi a Tizianu Ferrarovi, kteří důkladně testovali funkci Audiopopisu s AI i Sonarpad obecně a významně přispěli k jeho zlepšování.

- Velké poděkování také skupině Tecnologia Accessibile za podporu, testování a podněty.

Verze 0.3.1 - 16. července 2026

- Byl opraven problém, který bránil spuštění Sonarpadu, pokud nabídka Rádio obsahovala oblíbené stanice, kvůli neplatným identifikátorům nabídek wxWidgets.

- Sonarpad je nyní kromě italštiny a angličtiny dostupný také ve francouzštině, španělštině, portugalštině, češtině a polštině.

- Bylo přidáno samostatné nastavení Jazyka zpráv. Toto nastavení je nezávislé na jazyku rozhraní a umožňuje Sonarpadu používat zdroje a služby přizpůsobené vybranému jazyku.

- Byla přidána funkce Počasí, která umožňuje vyhledat město a zjistit aktuální podmínky, teplotu, srážky, vítr a vlhkost, stejně jako předpověď na dnešek, zítřek nebo jiný den.

- Byla přidána sekce Filmy v kinech s aktuálně promítanými filmy, připravovanými premiérami, popisy děje, daty uvedení a, pokud jsou k dispozici, odkazy na upoutávky.

- Do nabídky Nástroje byl přidán přístupný kalendář. Je možné vybrat libovolné datum, zjistit svátky, světce a citát dne, vytvářet připomenutí a přidávat události přímo do Kalendáře macOS.

- Byla přidána funkce Vyhledávání tras, která umožňuje vypočítat pěší, cyklistické, automobilové nebo bezbariérové trasy. Lze zvolit nejrychlejší nebo nejkratší trasu a zobrazit vzdálenost, odhadovanou dobu a podrobné pokyny.

- Byla přidána funkce Převod médií, která podporuje převod zvukových a obrazových souborů do několika formátů, včetně MP3, M4A, M4B, MP4, AVI, MOV, Opus, OGG, FLAC, WAV a AIFF. Je také možné vytvořit video ze zvukového souboru a obrázku.

- Byl přidán Slovník řeči. Je možné určit slova nebo výrazy, které má hlasový syntetizér při čtení nahrazovat, a tím opravit výslovnost, zkratky a specifická jména.

- Sekce Články byla rozšířena o příkazy Nedávné články a Sdílet, které umožňují rychle se vrátit k naposledy čtenému obsahu a sdílet články prostřednictvím služeb dostupných v macOS.

- Do nabídky Články byly přidány položky Přidat zdroj zpráv do komunity Sonarpad a Zdroje zpráv komunity Sonarpad. Je možné odeslat kanál RSS nebo zpravodajský web a importovat zdroje sdílené ostatními uživateli. Zdroje se přidávají a zobrazují podle vybraného Jazyka zpráv.

- Byla vylepšena správa zdrojů zpráv. Změna Jazyka zpráv nyní načte odpovídající výchozí zdroje, aniž by odstranila zdroje přidané samotným uživatelem.

- Vyhledávání rádií bylo rozšířeno o procházení podle jazyka, země a města, včetně úplných a lokalizovaných názvů zemí.

- Byla přidána možnost odeslat rozhlasovou stanici do komunity Sonarpad zadáním jejího názvu, adresy streamu, jazyka a žánru.

- Bylo přidáno nahrávání rádia a plánované nahrávání rádia. Tyto akce jsou dostupné ve výsledcích vyhledávání i v oblíbených a nahrávky se ukládají přímo jako soubory MP3. Po otevření rozhlasové stanice lze nahrávání spustit také stisknutím klávesy R.

- Do nabídky Soubor byl přidán seznam nedávno otevřených textových dokumentů, aby je bylo možné rychleji znovu otevřít.

- Byl přidán režim Pouze pro čtení, který je užitečný pro čtení dokumentu bez rizika nechtěných úprav.

- Byl přidán Obsah knihy pro soubory EPUB, které obsahují obsah. Je možné vybrat kapitolu a přejít přímo na ni.

- Byla přidána možnost volby mezi vysoce kvalitními hlasy Microsoft a systémovými hlasy macOS.

- Byla přidána možnost ignorovat při čtení pauzy způsobené prázdnými řádky.

- Bylo přidáno nastavení pro volbu počtu sekund, o které se má při přehrávání médií posunout vpřed nebo zpět.

- Byla vylepšena přístupnost oken, nabídek a ovládacích prvků, včetně konzistentnější práce s fokusem, klávesami Enter a Escape a klávesovými zkratkami.

- Byla vylepšena lokalizace zpráv, tlačítek a potvrzovacích dialogů ve všech podporovaných jazycích.

- Byl opraven problém, kvůli kterému se multimediální soubory nezobrazovaly v okně videa přehrávače.

- Byla opravena řada problémů ovlivňujících stabilitu, přehrávání médií, plánovaná nahrávání rádia, správu zdrojů a kompilaci v macOS.

- Zvláštní poděkování za trvalou podporu a pravidelné beta testování získávají Leonardo Graziano, Luca Maianti a italská skupina Tecnologia Accessibile.

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
