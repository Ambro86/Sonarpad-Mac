Lista zmian

Wersja 0.5.0 - 12 września 2026

Audiodeskrypcja, Narzędzia i konwersja wsadowa

1. Na macOS przeniesiono najnowsze ulepszenia silnika audiodeskrypcji z wersji Windows, w tym bardziej odporny most Gemini, lepszą obsługę problematycznych segmentów wideo oraz pewniejsze kontrole podczas analizy i ponownego eksportu.

2. W „Utwórz audiodeskrypcję z AI” można teraz wybrać „Użyj mojego klucza API Gemini” albo „Użyj Sonarpad AI”. Oba poświadczenia są przechowywane oddzielnie, więc zmiana trybu nie usuwa ani prywatnego klucza, ani kodu Sonarpad AI.

3. Po wybraniu Sonarpad AI aktualny kredyt jest wyświetlany w polu tylko do odczytu, razem z elementami do pokazania kodu i poproszenia o nowy.

4. Jeśli plik zawiera kilka ścieżek audio, Sonarpad przed utworzeniem audiodeskrypcji pyta, którą ścieżkę przeanalizować. Wybór jest zapisywany w projekcie i używany także w późniejszych operacjach.

5. Dodano opcję rozpoznawania ważnych tekstów widocznych na ekranie i uwzględniania ich podczas tworzenia opisów.

6. Audiodeskrypcja może teraz opcjonalnie tworzyć końcowy plik wideo zawierający audiodeskrypcję, oprócz zwykłego wyjścia audio. Ulepszono też obsługę kontenerów i znaczników czasu, gdy potrzebny jest alternatywny format wyjściowy.

7. Ulepszono ducking oryginalnej ścieżki dźwiękowej: obniżanie i przywracanie głośności wokół narracji jest płynniejsze, z łagodniejszym pre-duckiem i wybrzmieniem.

8. Do edytora projektu audiodeskrypcji dodano „Ponownie przeanalizuj segment”. Polecenie działa na aktualnie wybranym opisie i automatycznie korzysta z ostatnio wybranego globalnego trybu AI, tak jak w wersji Windows.

9. Edytor projektu może teraz przechowywać oczekujące zmiany kilku opisów jednocześnie. Szkice pozostają dostępne podczas przechodzenia między segmentami i wyszukiwania; po wybraniu „Zastosuj” wszystkie są sprawdzane i stosowane razem. Jeśli choć jeden opis nie mieści się w dostępnym miejscu bez dialogu, żadna zmiana nie jest stosowana, a Sonarpad wraca do opisu wymagającego poprawy.

10. Naprawiono ścieżkę zapisu przy tworzeniu drugiej audiodeskrypcji bez zamykania okna: wybór nowego pliku źródłowego tworzy teraz ścieżkę dla nowego pliku zamiast zachowywać poprzednią.

11. Dodano ustawienie „Grupuj menu Narzędzia według kategorii”. Po włączeniu Narzędzia są podzielone na „Czytanie i treści”, „Multimedia” i „Narzędzia użytkowe”; po wyłączeniu wraca płaskie menu.

12. W encyklopedii Treccani usunięto z interfejsu dostępności pusty element wyników, który pojawiał się przed wykonaniem wyszukiwania. Jest on teraz pokazywany tylko wtedy, gdy istnieją wyniki do wyboru.

13. Dodano „Konwertuj folder…” w Narzędzia > Multimedia. Można wsadowo konwertować cały folder przy użyciu tych samych formatów i ustawień co w „Konwertuj multimedia”, na przykład wiele plików WMA do MP3 w jednej operacji.

14. Konwersja folderu pokazuje postęp dla każdego pliku, proponuje podfolder „Przekonwertowane”, zachowuje podstawowe nazwy plików, podsumowuje błędy i chroni przed przypadkowym nadpisaniem oraz kolizjami nazw docelowych.

15. Dodano ustawienie, domyślnie włączone, które podczas przewijania multimediów do przodu lub do tyłu ogłasza zarówno bieżącą pozycję, jak i całkowity czas trwania w naturalnej formie, na przykład „1 minuta i 10 sekund z 1 godziny, 10 minut i 10 sekund”. Po wyłączeniu Sonarpad nadal ogłasza tylko bieżącą pozycję, tak jak w poprzednich wersjach.

16. Podczas odtwarzania skrót Option+I w każdej chwili ogłasza wyłącznie całkowity czas trwania multimediów w naturalnej formie. Skrót działa niezależnie od ustawienia dodającego całkowity czas do komunikatów podczas przewijania; w przypadku transmisji na żywo Sonarpad informuje, że jest to transmisja na żywo.

17. W funkcji Utwórz audiodeskrypcję z AI silnik i głos nie zajmują już miejsca w głównym oknie. Nowy przycisk „Dostosuj głos” otwiera osobne okno z silnikiem, głosem, szybkością i głośnością oraz testem głosu; wybory są zapisywane dla audiodeskrypcji. Jeśli okno nigdy nie zostanie użyte, szybkość i głośność nadal dziedziczą ustawienia ogólne tak jak wcześniej.

18. „Odtwarzaj multimedia ze streamingu” pokazuje teraz przy filmie również czas trwania, oprócz tytułu. W „Konwertuj folder” etykiety są jaśniejsze: „Wybierz folder do konwersji” i „Folder docelowy”; podczas konwersji przycisk „Przerwij konwersję” natychmiast kończy aktywny proces FFmpeg, usuwa bieżący plik częściowy i nie uruchamia kolejnych plików.

19. Dodano zabezpieczenie awaryjne dla problematycznych źródeł wielokanałowych (na przykład 5.1, 6.1 lub 7.1): normalny przebieg pozostaje bez zmian i jest używany tak jak wcześniej; tylko jeśli wewnętrzny plik WAV jest nieczytelny, ma nieoczekiwany format albo zawiera niepełne ramki PCM, Sonarpad automatycznie odtwarza ten etap jako stereo 48 kHz i ponawia próbę, nie wpływając na pliki, które już działają poprawnie.

20. Po rozpoczęciu konwersji pojedynczego pliku lub folderu VoiceOver ogłasza teraz „Konwersja rozpoczęta”, dzięki czemu od razu wiadomo, że proces ruszył, bez przechodzenia do wskaźnika postępu.

21. Zwiększono niezawodność źródeł RSS: jeśli oryginalny kanał wydawcy nie działa lub nie zwraca artykułów, Sonarpad automatycznie próbuje kanału Google News ograniczonego do witryny tego samego wydawcy i w wybranym języku wiadomości. Oryginalny kanał pozostaje zapisany i ma pierwszeństwo; fallback obejmuje także Il Giornale i główne techniczne hosty kanałów.

22. Naprawiono problem w macOS, przez który po zamknięciu zmodyfikowanego dokumentu i wybraniu „Nie zapisuj” pytanie o zapis mogło pojawić się ponownie. Sonarpad zapamiętuje teraz potwierdzenie dla bieżącego zdarzenia zamykania i pyta tylko raz.


23. „Źródła społeczności” pokazują teraz zawsze wszystkie źródła dostępne dla wybranego języka, także te już znajdujące się w bibliotece. Zaimportowane źródła są oznaczone jako „Już zaimportowane”; po wybraniu takiego źródła Sonarpad pyta, czy je zastąpić. Zastąpienie aktualizuje ten sam wpis bez tworzenia duplikatu i zachowuje folder, w którym użytkownik go umieścił.

24. Dodano „Przejdź do daty” w Podcastach i RaiPlay Sound, zgodnie z działaniem wersji mobilnej. W Podcastach polecenie pojawia się na początku podmenu tylko wtedy, gdy kanał zawiera rzeczywiste daty; wybranie daty pokazuje pełną listę odcinków z tego dnia, również poza pierwszymi 30 pozycjami menu. RaiPlay Sound wyświetla kontekstowy przycisk „Przejdź do daty”, gdy dostępne są treści z datą. Selektory pokazują bezpośrednio tylko dostępne daty, bez zbędnych etykiet, co upraszcza nawigację VoiceOver.

Wersja 0.4.0 - 3 września 2026

Audiodeskrypcja z AI — nowa główna funkcja

- Bezpośrednio do menu Narzędzia dodano „Utwórz audiodeskrypcję z AI”. Sonarpad analizuje dźwięk, wyszukuje miejsca bez dialogów, generuje opisy za pomocą Gemini i używa dostępnych już silników mowy, nie mówiąc ponad dialogami.

- Ulepszono synchronizację między tym, co dzieje się w filmie, a opisami oraz dodano automatyczne sprawdzanie czasów generowanych przez Gemini.

- „Włącz rozszerzone pauzy” jest domyślnie wyłączone. Opcję można włączyć w materiałach z dużą liczbą dialogów lub niewielką ilością wolnego miejsca, aby umożliwić wstawianie dłuższych opisów.

- Sonarpad może próbować rozpoznawać postacie i używać ich imion. Katalogi postaci można zachowywać między odcinkami serialu, aby poprawić ciągłość.

- Projekt można zapisać, później poprawić opisy i ponownie wyeksportować bez ponownego generowania wszystkiego przez Gemini.

- Jeśli proces zostanie przerwany, Sonarpad zachowuje postęp i pozwala kontynuować audiodeskrypcję. Po wyczerpaniu limitu Gemini można poczekać, zmienić model lub przerwać bez utraty ukończonej pracy.

- Okno pozwala wybrać język, poziom szczegółowości, model Gemini, silnik mowy i głos oraz zapamiętuje używane ustawienia. Moduł jest dostępny w językach obsługiwanych przez Sonarpad dla Mac.

- Podczas generowania interfejs pokazuje postęp, bieżący stan i Anuluj; po zakończeniu MP3 można otworzyć bezpośrednio w wewnętrznym odtwarzaczu.

- Ulepszono zgodność z filmami MKV: Sonarpad lepiej obsługuje nieregularne lub brakujące znaczniki czasu i, gdy to możliwe, pomija uszkodzone pakiety bez przerywania audiodeskrypcji.

- Naprawiono problem, który mógł powodować błąd końcowego eksportu do MP3 w filmach z dźwiękiem wielokanałowym, na przykład Dolby 5.1. Sonarpad automatycznie miksuje teraz dźwięk wielokanałowy do stereo, gdy jest to potrzebne do kodowania MP3.

- Gdy film zawiera wiele ścieżek audio, Sonarpad przed rozpoczęciem pyta, której ścieżki użyć. Dostępne pole kombi można zmieniać strzałkami; OK uruchamia audiodeskrypcję z wybraną ścieżką, a Anuluj zamyka okno i przywraca fokus do edytora Sonarpad.

- Dodano pole wyboru „Pokaż klucz API” obok klucza Gemini. Klucz jest domyślnie ukryty i jest wyświetlany tylko tymczasowo po włączeniu tej opcji; po ponownym otwarciu okna znów pozostaje ukryty.

YouTube i strumieniowanie

- Znacznie poprawiono korzystanie z YouTube: przyspieszono wyszukiwanie i nawigację oraz przywrócono prawidłowe działanie.

- Opcje jakości wideo są teraz przetłumaczone: zamiast technicznej wartości „best” Sonarpad pokazuje czytelną etykietę w języku interfejsu.

- Sonarpad zapamiętuje ostatni format wybrany w Zapisz multimedia. Na przykład po wybraniu MP4, MP4 pozostaje wstępnie wybrane przy kolejnym otwarciu okna.


Podziękowania

- Wielkie podziękowania dla Leonardo Graziano i Tiziano Ferraro, którzy dokładnie przetestowali funkcję Audiodeskrypcji z AI oraz Sonarpad jako całość, wnosząc cenny wkład w jego ulepszanie.

- Wielkie podziękowania również dla grupy Tecnologia Accessibile za wsparcie, testy i sugestie.

Wersja 0.3.1 - 16 lipca 2026

- Naprawiono problem uniemożliwiający uruchomienie Sonarpada, gdy menu Radio zawierało ulubione stacje, spowodowany nieprawidłowymi identyfikatorami menu wxWidgets.

- Sonarpad jest teraz dostępny także w języku francuskim, hiszpańskim, portugalskim, czeskim i polskim, oprócz włoskiego i angielskiego.

- Dodano osobne ustawienie Język wiadomości. Jest ono niezależne od języka interfejsu i pozwala Sonarpadowi korzystać ze źródeł i usług dostosowanych do wybranego języka.

- Dodano funkcję Pogoda, która umożliwia wyszukanie miasta i sprawdzenie bieżących warunków, temperatury, opadów, wiatru i wilgotności, a także prognozy na dziś, jutro lub inny dzień.

- Dodano sekcję Filmy w kinach, zawierającą filmy aktualnie wyświetlane, nadchodzące premiery, opisy fabuły, daty premier oraz, gdy są dostępne, odnośniki do zwiastunów.

- Do menu Narzędzia dodano dostępny kalendarz. Można wybrać dowolną datę, sprawdzić święta, patrona i cytat dnia, tworzyć przypomnienia oraz dodawać wydarzenia bezpośrednio do Kalendarza macOS.

- Dodano funkcję Wyszukiwanie tras, która pozwala obliczać trasy piesze, rowerowe, samochodowe oraz dostępne dla osób poruszających się na wózkach. Można wybrać trasę najszybszą lub najkrótszą oraz sprawdzić odległość, przewidywany czas i szczegółowe wskazówki.

- Dodano funkcję Konwertuj multimedia, obsługującą konwersję plików audio i wideo do wielu formatów, w tym MP3, M4A, M4B, MP4, AVI, MOV, Opus, OGG, FLAC, WAV i AIFF. Można także utworzyć film z pliku audio i obrazu.

- Dodano Słownik mowy. Można określić słowa lub wyrażenia, które syntezator mowy ma zastępować podczas czytania, aby poprawić wymowę, skróty i szczególne nazwy.

- Rozszerzono sekcję Artykuły o polecenia Ostatnie artykuły i Udostępnij, które pozwalają szybko wrócić do ostatnio czytanych treści i udostępniać artykuły za pomocą usług dostępnych w macOS.

- Do menu Artykuły dodano polecenia Dodaj źródło wiadomości do społeczności Sonarpad oraz Źródła wiadomości społeczności Sonarpad. Można przesłać kanał RSS lub witrynę informacyjną i importować źródła udostępnione przez innych użytkowników. Źródła są dodawane i wyświetlane zgodnie z wybranym Językiem wiadomości.

- Ulepszono zarządzanie źródłami wiadomości. Zmiana Języka wiadomości wczytuje teraz odpowiednie źródła domyślne bez usuwania źródeł dodanych osobiście przez użytkownika.

- Rozszerzono wyszukiwanie radia o przeglądanie według języka, kraju i miasta, wraz z pełnymi i zlokalizowanymi nazwami państw.

- Dodano możliwość przesłania stacji radiowej do społeczności Sonarpad z podaniem jej nazwy, adresu strumienia, języka i gatunku.

- Dodano nagrywanie radia oraz planowanie nagrań radiowych. Funkcje te są dostępne zarówno w wynikach wyszukiwania, jak i w ulubionych, a nagrania są zapisywane bezpośrednio jako pliki MP3. Po otwarciu stacji radiowej nagrywanie można również rozpocząć, naciskając literę R.

- Do menu Plik dodano listę ostatnio otwieranych dokumentów tekstowych, aby można je było szybciej ponownie otworzyć.

- Dodano tryb Tylko do odczytu, przydatny do czytania dokumentu bez ryzyka przypadkowej modyfikacji.

- Dodano Spis treści książki dla plików EPUB zawierających spis treści. Można wybrać rozdział i przejść bezpośrednio do niego.

- Dodano możliwość wyboru między wysokiej jakości głosami Microsoft a głosami systemowymi macOS.

- Dodano opcję ignorowania podczas czytania pauz powodowanych przez puste wiersze.

- Dodano ustawienie pozwalające wybrać, o ile sekund przewijać multimedia do przodu lub do tyłu.

- Ulepszono dostępność okien, menu i elementów sterujących, zapewniając bardziej spójną obsługę fokusu, klawiszy Enter i Escape oraz skrótów klawiaturowych.

- Ulepszono lokalizację komunikatów, przycisków i okien potwierdzeń we wszystkich obsługiwanych językach.

- Naprawiono problem, przez który pliki multimedialne nie były wyświetlane w oknie wideo odtwarzacza.

- Naprawiono liczne problemy wpływające na stabilność, odtwarzanie multimediów, zaplanowane nagrania radiowe, zarządzanie źródłami i kompilację w macOS.

- Szczególne podziękowania kierujemy do Leonardo Graziano, Luca Maianti oraz włoskiej grupy Tecnologia Accessibile za stałe wsparcie i regularne testy beta.


Wersja 0.2.9 - 1 maja 2026
- Rozszerzono funkcje YouTube także na komputery Mac Intel i Catalina.
- Znacznie przyspieszono wyszukiwanie w YouTube.
- Ulepszono obsługę wyników YouTube, umieszczając kanały i playlisty na początku.
- Dodano możliwość dodawania i usuwania kanałów oraz playlist z ulubionych.
- Dodano przycisk Podgląd głosu w opcjach.
- Dodano przycisk Zaznacz wszystko podczas usuwania źródeł.
- Dodano pasek postępu dla wyszukiwania w Wikipedii.
- Dodano kanał TV Videolina.
- Pozycje menu funkcji dodatkowych przeniesiono do Narzędzi, aby ujednolicić Sonarpad z wersją dla Windows.
- Poprawiono zachowanie, przez które czasami w TV nie były wyświetlane programy aktualnie nadawane.
- Dodano liczne kanały TV, organizując okno w kategorie dla łatwiejszego przeglądania. Dodano także pole wyszukiwania pokazujące wyniki dla wybranej TV.

Wersja 0.2.8 - 29 kwietnia 2026
- Dodano menu Narzędzia z dwiema nowymi pozycjami: Szukaj i importuj z Wikipedii oraz Odtwórz audio ze strumienia.
- Szukaj i importuj z Wikipedii pozwala wyszukiwać i importować artykuły, czytać je oraz zapisywać jako audiobooki.
- Odtwórz audio ze strumienia pozwala odtwarzać treści strumieniowe, na przykład z YouTube.
- W polu wyszukiwania streamingu można wpisać dowolną treść: program ją wyszuka i może również otwierać kanały oraz playlisty.
- Wyszukiwanie YouTube nie jest włączone na Macach Intel z powodu niezgodności.
- Szczególne podziękowania dla Leonardo Graziano za stałe wsparcie.
- W wynikach radiowych dodano przycisk przejścia bezpośrednio do wybranej strony wyników, bez konieczności ciągłego używania opcji Przejdź do następnej strony.
- Rozszerzono automatyczną zakładkę także na pliki tekstowe.
- Poprawiono problem, przez który audiodeskrypcje czasami nie były zapisywane z powodu przekroczenia czasu oczekiwania.
- Dodano możliwość ustawiania ulubionych TV.
- Na liście kanałów TV dodano informację o programie aktualnie nadawanym.
- Dodano kompletny przewodnik TV, dostępny od poprzedniego dnia do pięciu dni po bieżącej dacie.

Wersja 0.2.7 - 28 kwietnia 2026
- Ulepszono obsługę plików ze znakami diakrytycznymi i kodowaniami innymi niż UTF-8, w tym znaków chińskich i innych języków międzynarodowych.
- Poprawiono problem, przez który wpisanie przecinka w polu tekstowym błędnie otwierało opcje.
- Poprawiono szybkość czytania: teraz także długie artykuły są czytane szybciej, a pauza po akapitach została usunięta.
- Dodano możliwość otwierania w Sonarpad plików JPG i podobnych formatów, aby wykonywać OCR także na artykułach przesłanych jako obrazy lub zdjęcia.
- Dodano możliwość ustawienia Sonarpad jako programu domyślnego.
- Od teraz Sonarpad może otwierać nie tylko pliki tekstowe, ale także pliki audio i wideo, używając odtwarzacza MPV.
- W opcjach dodano funkcję automatycznej zakładki: jeśli plik, podcast lub dowolna treść multimedialna zostanie zamknięta, zostanie ponownie otwarta dokładnie od miejsca, w którym ją pozostawiono.
- Stacje radiowe nie są już otwierane w Safari, lecz odtwarzane bezpośrednio przez odtwarzacz Sonarpad.
- Od tej wersji aplikacja jest podpisana i nie wymaga już żadnej autoryzacji użytkownika, co upraszcza instalację.
- Dodano automatyczną aktualizację programu, która sprawdza, pobiera i automatycznie aktualizuje Sonarpad.
- Dodano moduły dodatkowe RaiPlay, Audiodeskrypcje Rai, RaiPlay Sound i kanały TV. Aby z nich korzystać, trzeba poprosić autora o kod.
- Aby uzyskać kod, należy wykonać procedurę wskazaną przez program i wysłać wygenerowaną wiadomość e-mail, upewniając się, że rzeczywiście znajduje się w wysłanych. Jeśli procedura zostanie wykonana poprawnie, kod zostanie otrzymany w ciągu około minuty.
- Kod należy wprowadzić, otwierając opcje skrótem Command + , i przechodząc za pomocą VO + strzałka w prawo do pola Kod Sonarpad dla funkcji dodatkowych.
- Uwaga: jeśli podczas otwierania funkcji dodatkowej, na przykład RaiPlay, pojawi się błąd, prawdopodobnie oznacza to, że kod nie został skopiowany w całości.
- W modułach Rai dodano wyszukiwanie i przeglądanie treści, które są odtwarzane przez odtwarzacz Sonarpad.

Wersja 0.2.6
- Poprawiono błąd wx/macOS, który mógł powodować pojawienie się błędu przy uruchomieniu, i ustabilizowano powiązane menu.
- Poprawiono skrót Cmd+, dla menu Opcje także wtedy, gdy fokus znajduje się w edytorze lub na kontrolkach.
- Podczas zapisywania audiobooka fokus jest teraz poprawnie ustawiany na polu tekstowym, a nazwy plików z kropką nie są już ucinane.
- Dodano obsługę OPML z Lire z podziałem na foldery: foldery otwierają się jako podmenu, a pojedyncze źródła w dedykowanym oknie.
- Porządkowanie źródeł artykułów obsługuje teraz nowy system folderów z przyciskami Otwórz folder, Folder główny, Przenieś do folderu i Przenieś poza foldery.

Wersja 0.2.5
- Nowe niestandardowe okna zapisu tekstu i audiobooków w macOS.
- Pola nazw plików poprawnie obsługują teraz Cmd+V, Cmd+A i inne polecenia edycji.
- Program zapamiętuje ostatni folder i ostatni format użyty do zapisu tekstu i audiobooków.
- Dodano zapisywanie audiobooków także w formatach M4A i WAV.
- Dodano menu Radio z wyszukiwaniem według języka, dodawaniem do ulubionych, ręcznym dodawaniem stacji oraz edycją i zmianą kolejności ulubionych.
- Ulepszono obsługę źródeł artykułów dodanych jako strony: wykrywanie kanału z poziomu strony i korektę kanału komentarzy.
- Zaktualizowano proces wydania macOS, aby obejmował także artefakt Catalina.

Wersja 0.2.4
- Ważne ulepszenia OCR PDF na macOS dzięki przejściu na pdfium i bardziej niezawodnym mechanizmom awaryjnym.
- Dodano eksport M4B na macOS i dopracowano zapisywanie tekstu.
- Ulepszono obsługę źródeł artykułów i ochronę odświeżania, gdy źródło zwraca zero elementów.
- Zoptymalizowano syntezę Edge TTS dzięki bardziej niezawodnemu dzieleniu na fragmenty i ponownym próbom.
- Dodano i dopracowano potok Catalina dla kompilacji i pakietowania macOS.

Wersja 0.2.2
- Ulepszono ładowanie PDF na macOS dzięki czytelniejszym komunikatom i wyraźnemu końcowemu oknu dialogowemu.
- Alfabetyczne sortowanie źródeł artykułów.
- Naprawy tekstu PDF i ogólne ulepszenia lokalizacji.

Wersja 0.2.1
- Ustabilizowano skróty i menu macOS dla uruchamiania, pauzy, zatrzymania i zapisu.
- Ulepszono zewnętrzne otwieranie odcinków podcastów na macOS.
- Poprawiono trwałość ustawień macOS.
- Wzmocniono procesy kompilacji Intel/macOS i zarządzanie Xcode.

Wersja 0.2.0
- Pierwsze wydanie macOS Sonarpad dla Mac.
- Obsługa czytania tekstu, artykułów i podcastów z syntezą mowy.
- Obsługa OCR PDF na macOS, pobierania aktualizacji i dedykowanych pakietów DMG.
- Hierarchiczne kategorie podcastów i pierwsze globalne skróty/macOS.
