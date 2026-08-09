# Sonarpad Audio Description Bridge — macOS

Worker Python headless del modulo **Crea audiodescrizione IA** per Sonarpad Mac. Esegue la segmentazione del parlato con il modello Pyannote ONNX incluso e la logica Gemini usata anche dal backend Windows.

Il worker viene costruito con PyInstaller in modalità onedir e distribuito direttamente dentro `Sonarpad.app/Contents/Resources/audio-description/audio_description_bridge`. L’utente finale non deve installare Python, ONNX Runtime o altri componenti.

Le dipendenze principali del worker sono bloccate nel build script. In particolare,
`cryptography` deve essere installato esclusivamente da una wheel binaria macOS:
una compilazione locale contro Homebrew OpenSSL può produrre un worker che si avvia
ma fallisce quando viene importato Gemini. Al termine della build viene eseguito il
binario PyInstaller con `--self-test`, che importa realmente Gemini e verifica il
modello ONNX senza effettuare richieste di rete.

Le versioni del client Gemini (`google-genai 2.12.1` e `google-api-core 2.32.0`)
corrispondono al worker Windows verificato come funzionante. macOS conserva
separatamente il pin compatibile di `cryptography`.

Protocollo stdout:

```text
STATUS:{json}
PROGRESS:0-100
QUOTA:{json}
RESULT:{json}
```

`QUOTA` viene emesso quando l’API richiede una decisione interattiva sulla quota o sul modello. La pipeline usa Pyannote per il parlato e Gemini API per le descrizioni, con audit di timestamp/copertura, recovery pass e continuità del catalogo personaggi.

Il worker non include e non avvia `ffmpeg` o `ffprobe`: Sonarpad usa il proprio FFmpeg incluso nel bundle per creare WAV/chunk e per il rendering finale. Anche TTS, ducking, scheduling e salvataggio progetto/catalogo rimangono nel processo Rust.

## Build macOS

```bash
PYTHON_BIN=python3 ./bridge/build_audio_description_bridge_macos.sh "$PWD/dist/audio-description-worker"
```

I workflow GitHub eseguono questo passaggio automaticamente per Apple Silicon, Intel e Catalina, copiano l’onedir nel bundle e firmano tutti i Mach-O del worker prima della firma/notarizzazione dell’app.

## Test

```bash
python3 bridge/run_audio_description_tests_macos.py
```

La suite Mac comprende i test portabili del motore e controlli specifici dell’host macOS: protocollo, timestamp, retry Gemini, priorità mandatory, bundle PyInstaller, menu wxDragon e workflow GitHub.
