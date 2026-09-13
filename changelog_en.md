Changelog

Version 0.5.0 - September 12, 2026

Audio Description, Tools and batch conversion

1. Brought the latest Windows audio-description engine improvements to macOS, with a more resilient Gemini bridge, better handling of problematic video segments, and more reliable checks during analysis and re-export.

2. “Create AI Audio Description” can now use either “Use my Gemini API key” or “Use Sonarpad AI”. The two credentials are stored independently, so switching modes never deletes the personal key or the Sonarpad AI code.

3. When Sonarpad AI is selected, the current credit is shown in a read-only field, together with controls to reveal the code and request a new one.

4. If a source file contains multiple audio tracks, Sonarpad now asks which track should be analyzed before creating the audio description. The selected track is stored in the project and is reused by later operations.

5. Added an option to recognize important on-screen text and take it into account when generating descriptions.

6. Audio-description creation can now optionally produce a final video containing the audio description in addition to the normal audio output. Container and timestamp handling has also been improved when an alternate output format is required.

7. Improved ducking of the original soundtrack: volume reduction and recovery around narration are now smoother, with gentler pre-duck and release behavior.

8. Added “Reanalyze segment” to the audio-description project editor. It works on the currently selected description and automatically uses the last global AI access mode, matching the Windows behavior.

9. The project editor can now keep edits to several descriptions pending at the same time. Drafts remain available while moving between segments or searching; pressing “Apply” validates and commits them together. If even one description does not fit the available silence, none of the edits are applied and Sonarpad returns to the description that needs correction.

10. Fixed the save path when creating a second audio description without closing the window: selecting a new source now generates a path for the new file instead of keeping the previous one.

11. Added the “Group the Tools menu by category” setting. When enabled, Tools is organized into “Reading and content”, “Multimedia”, and “Utilities”; disabling it restores the flat menu.

12. In the Treccani encyclopedia, the empty results control that appeared before a search has been removed from the accessible interface. It is now shown only when selectable results exist.

13. Added “Convert folder…” under Tools > Multimedia. An entire folder can be batch-converted using the same formats and settings as “Convert media”, for example converting many WMA files to MP3 in one operation.

14. Folder conversion shows per-file progress, suggests a “Converted” subfolder, preserves base filenames, summarizes failures, and protects against accidental overwrites and destination-name collisions.

15. Added a setting, enabled by default, that announces both the current position and the total duration in a natural format when seeking forward or backward through media, for example “1 minute 10 seconds of 1 hour 10 minutes 10 seconds”. When disabled, Sonarpad continues to announce only the current position as in previous versions.

16. During playback, Option+I now announces only the total duration of the current media in a natural format. The shortcut works independently of the setting that adds total duration to seek announcements; live streams are announced as live.

17. In Create audio description with AI, engine and voice no longer occupy the main window. The new “Adjust voice” button opens a dedicated window with engine, voice, speed and volume, plus voice testing; the choices are saved for audio descriptions. If the window is never used, speed and volume continue to inherit the general settings as before.

18. “Play streaming media” now shows each video’s duration together with its title. In “Convert folder”, labels are clearer with “Choose folder to convert” and “Destination folder”; while conversion is running, “Stop conversion” now terminates the active FFmpeg process immediately, removes the partial current output, and prevents subsequent files from starting.

19. Added a fallback safeguard for problematic multichannel sources (for example 5.1, 6.1 or 7.1): the normal pipeline remains unchanged and is used exactly as before; only if the internal WAV is unreadable, has an unexpected format, or contains misaligned PCM frames does Sonarpad automatically rebuild that step as 48 kHz stereo and retry, preventing finalization failures without affecting files that already work.

20. When a single-file or folder conversion starts, VoiceOver now announces “Conversion started”, so users immediately know that the process has begun without moving to the progress indicator.

21. Improved RSS source reliability: if a publisher’s original feed fails or returns no articles, Sonarpad automatically tries a Google News feed restricted to the same publisher site and using the selected News language. The original feed remains stored and keeps priority; the fallback also covers Il Giornale and common technical feed hosts.

22. Fixed an issue on macOS where closing a modified document and choosing “Don’t Save” could show the save prompt a second time. Sonarpad now remembers the confirmation for the current close event and asks only once.


23. “Community news sources” now always shows every source available for the selected language, including sources already present in the library. Already imported sources are marked “Already imported”; selecting one asks whether to replace it. Replacing updates the same entry without creating duplicates and preserves any folder where the user organized it.

24. Added “Go to date” to Podcasts and RaiPlay Sound, following the mobile behavior. In Podcasts the command appears at the top of a podcast submenu only when real feed dates are available; choosing a date shows the complete list of episodes from that day, including episodes beyond the first 30 shown in the menu. RaiPlay Sound now shows a contextual “Go to date” button when dated content is available. Date selectors contain only the available dates, without redundant labels, for cleaner VoiceOver navigation.

Version 0.4.0 - September 3, 2026

AI Audio Description — major new feature

- Added “Create AI Audio Description” directly to the Tools menu. Sonarpad analyzes the audio to find spaces without dialogue, generates descriptions with Gemini, and uses the speech engines already available in Sonarpad while avoiding spoken dialogue.

- Improved synchronization between what happens in the video and the generated descriptions, with automatic checks on Gemini timestamps.

- “Enable extended pauses” is disabled by default. It can be enabled for content with heavy dialogue or little available space so longer descriptions can still be inserted.

- Sonarpad can try to recognize characters and use their names. Character catalogs can be kept across episodes of a series to improve continuity.

- Projects can be saved, edited later, and exported again without generating everything again with Gemini.

- If the process is interrupted, Sonarpad keeps its progress and can continue the audio description. If the Gemini quota is exhausted, you can wait, switch model, or stop without losing completed work.

- The window lets you choose language, detail level, Gemini model, speech engine, and voice, and remembers the selected preferences. The module is available in the languages supported by Sonarpad for Mac.

- During generation the interface shows progress, current status, and Cancel; when finished, the MP3 can be opened directly in the internal player.

- Improved compatibility with MKV videos: Sonarpad handles irregular or missing timestamps more reliably and, when possible, skips corrupted packets without stopping the audio-description process.

- Fixed an issue that could cause final MP3 export to fail with videos containing multichannel audio such as Dolby 5.1. Sonarpad now automatically downmixes multichannel audio to stereo when required for MP3 encoding.

- When a video contains multiple audio tracks, Sonarpad asks which track to use before processing. The accessible combo box can be changed with the arrow keys; OK starts the audio description with the selected track, while Cancel closes the window and returns focus to the Sonarpad editor.

- Added a “Show API key” checkbox next to the Gemini key. The key remains hidden by default and is shown only temporarily while the checkbox is enabled; reopening the window always hides it again.

YouTube and streaming

- Significantly improved the YouTube experience by speeding up search and navigation and restoring proper operation.

- Video quality options are now localized: instead of the technical value “best”, Sonarpad shows a clear label in the interface language.

- Sonarpad remembers the last format selected in Save media. For example, if MP4 is selected, MP4 remains preselected the next time the dialog is opened.


Acknowledgements

- Special thanks to Leonardo Graziano and Tiziano Ferraro, who thoroughly tested AI Audio Description and Sonarpad in general, making a valuable contribution to its improvement.

- Special thanks also to the Tecnologia Accessibile group for its support, testing, and suggestions.

Version 0.3.1 - July 16, 2026

- Fixed an issue that prevented Sonarpad from starting when the Radio menu contained favorites, due to invalid wxWidgets menu identifiers.

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

- Added radio recording and scheduled radio recording. These actions are available both in search results and favorites, and recordings are saved directly as MP3 files. After opening a radio station, recording can also be started by pressing the letter R.

- Added a list of recently opened text documents to the File menu, making it easier to reopen them quickly.

- Added Read-only mode, which is useful for reading a document without accidentally modifying it.

- Added Book contents for EPUB files that include a table of contents. You can select a chapter and move directly to it.

- Added the option to choose between high-quality Microsoft voices and macOS system voices.

- Added an option to ignore pauses caused by empty lines while reading.

- Added a setting for choosing how many seconds to move forward or backward during media playback.

- Improved the accessibility of windows, menus and controls, with more consistent handling of focus, Enter, Escape and keyboard shortcuts.

- Improved the localization of messages, buttons and confirmation dialogs in all supported languages.

- Fixed an issue that prevented media files from being displayed in the player’s video window.

- Fixed numerous issues affecting stability, media playback, scheduled radio recordings, source management and compilation on macOS.

- Special thanks to Leonardo Graziano, Luca Maianti and the Italian Tecnologia Accessibile group for their continued support and ongoing beta testing.

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
