# Test macOS del modulo Crea audiodescrizione IA

La suite viene eseguita con:

```bash
python3 bridge/run_audio_description_tests_macos.py
```

Comprende i test portabili del worker e test strutturali del port Mac. Verifica in particolare protocollo worker/Rust, Pyannote ONNX, timestamp e chunk, retry Gemini, replay guard tra chunk, priorità delle descrizioni mandatory, localizzazioni Mac, profilo Chrome/CDP, nuova chat Gemini per chunk fisico e packaging/signing del worker nei workflow.

I test non usano rete né chiavi API e non avviano FFmpeg. La preparazione media e il bundle vengono verificati staticamente; la compilazione Rust e la firma/notarizzazione reale avvengono nei workflow macOS.
