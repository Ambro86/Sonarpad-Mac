Lista zmian

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
