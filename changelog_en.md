Version 0.3.0 - July 16, 2026

- Sonarpad is now also available in French, Spanish, Portuguese, Czech and Polish, in addition to Italian and English.

- Added a separate News language setting. This setting is independent of the interface language and allows Sonarpad to use news sources and services tailored to the selected language.

- Added the Weather feature, which lets you search for a city and check current conditions, temperature, precipitation, wind and humidity, as well as forecasts for today, tomorrow or another day.

- Added the Movies in theaters section, with films currently showing, upcoming releases, plot summaries, release dates and, when available, links to trailers.

- Added an accessible calendar to the Tools menu. You can select any date, check holidays, the saint and quote of the day, create reminders and add appointments directly to macOS Calendar.

- Added the Route search feature, which can calculate walking, cycling, driving or wheelchair-accessible routes. You can choose the fastest or shortest route and view the distance, estimated duration and detailed directions.

- Added the Convert media feature, which supports converting audio and video files to several formats, including MP3, M4A, M4B, MP4, AVI, MOV, Opus, OGG, FLAC, WAV and AIFF. You can also create a video from an audio file and an image.

- Added the Speech dictionary. You can define words or expressions that the speech synthesizer should replace while reading, making it possible to correct pronunciations, abbreviations and particular names.

- Expanded the Articles section with Recent articles and Share commands, allowing you to quickly return to recently read content and share articles through the services available on macOS.

- Added Add a news source to the Sonarpad community and Sonarpad community news sources to the Articles menu. You can submit an RSS feed or news website and import sources shared by other users. Sources are added and displayed according to the selected News language.

- Improved news source management. Changing the News language now loads the appropriate default sources without removing sources added personally by the user.

- Expanded radio search with browsing by language, country and city, including complete and localized country names.

- Added the ability to submit a radio station to the Sonarpad community by specifying its name, stream address, language and genre.

- Added radio recording and scheduled radio recording. These actions are available both in search results and favorites, and recordings are saved directly as MP3 files.

- Added a list of recently opened text documents to the File menu, making it easier to reopen them quickly.

- Added Read-only mode, which is useful for reading a document without accidentally modifying it.

- Added Book contents for EPUB files that include a table of contents. You can select a chapter and move directly to it.

- Added the option to choose between high-quality Microsoft voices and macOS system voices.

- Added an option to ignore pauses caused by empty lines while reading.

- Added a setting for choosing how many seconds to move forward or backward during media playback.

- Improved the accessibility of windows, menus and controls, with more consistent handling of focus, Enter, Escape and keyboard shortcuts.

- Improved the localization of messages, buttons and confirmation dialogs in all supported languages.

- Fixed numerous issues affecting stability, media playback, scheduled radio recordings, source management and compilation on macOS.

- Special thanks to Leonardo Graziano, Luca Maianti and the Italian Tecnologia Accessibile group for their continued support and ongoing beta testing.

Changelog

Version 0.2.9 - May 1, 2026
- Extended YouTube features to Intel Macs and Catalina.
- Greatly improved YouTube search speed.
- Improved YouTube results handling, placing channels and playlists at the top.
- Added the ability to add and remove channels and playlists from favorites.
- Added a Voice preview button in the options.
- Added a Select all button when removing sources.
- Added a progress bar for Wikipedia search.

Version 0.2.8 - April 29, 2026
- Added the Tools menu with two new items: Search and import from Wikipedia and Play streaming audio.
- Search and import from Wikipedia lets you search for and import articles, read them, and save them as audiobooks.
- Play streaming audio can play streaming content, such as YouTube.
- In the streaming search box, you can type any content: the program will search for it and can also open channels and playlists.
- YouTube search is not enabled on Intel Macs due to incompatibility.
- Special thanks to Leonardo Graziano for his continuous support.
- For radio results, added a button to go directly to the selected results page without repeatedly using Go to next page.
- Extended the automatic bookmark feature to text files as well.

Version 0.2.7 - April 28, 2026
- Improved support for text files with diacritics and non-UTF-8 encodings (including support for Chinese characters and other international languages).
- Fixed an issue where typing a comma in a text field incorrectly opened the settings.
- Improved reading speed: long articles are now read faster, and the pause after paragraphs has been removed.
- Added the ability to open JPG files and similar formats with Sonarpad, allowing OCR to be performed on articles sent as images or photos.
- Added the ability to set Sonarpad as the default program.
- Sonarpad can now open not only text files, but also audio and video files, using the MPV player.
- Added an automatic bookmark option: when closing any file, podcast, or media content, Sonarpad will reopen it from the exact position where it was left.
- Radio stations are no longer opened in Safari; they are now played directly through Sonarpad's player.
- Starting with this version, the app is signed and no longer requires any user authorization, making installation simpler.
- Added automatic program updates: Sonarpad now checks, downloads, and updates itself automatically.

Version 0.2.6
- Fixed a wx/macOS bug that could show an error at startup and stabilized the related menus.
- Fixed the Cmd+, shortcut for the Settings menu even when focus is on the editor or other controls.
- When saving an audiobook, focus is now placed correctly on the text field and filenames containing a dot are no longer truncated.
- Added support for Lire OPML files with folder grouping: folders open as submenus and individual sources open in a dedicated dialog.
- Article source reordering now supports the new folder organization system with Open Folder, Root Folder, Move to Folder, and Move Out of Folders controls.

Version 0.2.5
- New custom save dialogs for text and audiobooks on macOS.
- Filename fields now correctly accept Cmd+V, Cmd+A, and standard editing shortcuts.
- The app now remembers the last folder and format used for text and audiobook saves.
- Added audiobook saving in M4A and WAV format.
- Added the Radio menu with language-based search, add to favorites, manual station entry, and favorite editing and reordering.
- Improved article sources added as websites: feed discovery from the page and comments-feed fix.
- macOS release workflow updated to include the Catalina artifact as well.

Version 0.2.4
- Major macOS PDF OCR improvements with the move to pdfium and stronger fallbacks.
- Added M4B export on macOS and improved text saving.
- Improved article source handling and protected refresh when a source returns zero items.
- Improved Edge TTS chunking and retry behavior.
- Added and refined the Catalina build and packaging pipeline.

Version 0.2.2
- Improved macOS PDF loading with clearer feedback and an explicit completion dialog.
- Added alphabetical sorting for article sources.
- Improved PDF text repair and localization.

Version 0.2.1
- Stabilized macOS shortcuts and menu actions for start, pause, stop, and save.
- Improved external podcast episode opening on macOS.
- Fixed macOS settings persistence.
- Hardened Intel/macOS build workflows and Xcode selection.

Version 0.2.0
- First macOS release of Sonarpad Per Mac.
- Text reading, articles, and podcast support with speech synthesis.
- macOS PDF OCR support, update downloads, and dedicated DMG packages.
- Hierarchical podcast categories and the first macOS shortcut work.
