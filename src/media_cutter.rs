use crate::{append_podcast_log, ffmpeg_executable_path, announce_voiceover_message};
use std::cell::{Cell, RefCell};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;
use wxdragon::prelude::*;
use wxdragon::timer::Timer;

const MIN_PART_SECONDS: f64 = 0.10;

#[derive(Clone, Copy, PartialEq, Eq)]
enum VideoRotation {
    None,
    Right,
    Left,
    Half,
}

#[derive(Clone, Debug)]
struct MediaPart {
    start: f64,
    end: f64,
    keep: bool,
}

impl MediaPart {
    fn duration(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }
}

#[derive(Clone, Debug)]
struct ProbeInfo {
    duration: f64,
    has_video: bool,
    has_audio: bool,
}

#[derive(Clone, Debug)]
struct AddedTrackSettings {
    path: PathBuf,
    original_volume: i32,
    new_volume: i32,
    loop_track: bool,
}

struct PreviewState {
    child: Option<Child>,
    playing: bool,
    base_position: f64,
    started_at: Option<Instant>,
    stop_at: Option<f64>,
}

impl PreviewState {
    fn new() -> Self {
        Self {
            child: None,
            playing: false,
            base_position: 0.0,
            started_at: None,
            stop_at: None,
        }
    }

    fn position(&self, duration: f64) -> f64 {
        let mut position = self.base_position;
        if self.playing {
            if let Some(started_at) = self.started_at {
                position += started_at.elapsed().as_secs_f64();
            }
        }
        position.clamp(0.0, duration.max(0.0))
    }

    fn stop_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.playing = false;
        self.started_at = None;
        self.stop_at = None;
    }

    fn pause(&mut self, duration: f64) {
        self.base_position = self.position(duration);
        self.stop_child();
    }
}

impl Drop for PreviewState {
    fn drop(&mut self) {
        self.stop_child();
    }
}

struct ExportProgress {
    percent: i32,
    message: String,
    finished: bool,
    result: Option<Result<PathBuf, String>>,
}

#[derive(Clone)]
struct ExportSnapshot {
    input: PathBuf,
    output: PathBuf,
    parts: Vec<MediaPart>,
    probe: ProbeInfo,
    rotation: VideoRotation,
    added_track: Option<AddedTrackSettings>,
}

#[derive(Clone, Copy)]
struct Labels {
    menu: &'static str,
    title: &'static str,
    open_file: &'static str,
    mode: &'static str,
    guided: &'static str,
    advanced: &'static str,
    movement: &'static str,
    back: &'static str,
    play: &'static str,
    pause: &'static str,
    forward: &'static str,
    set_start: &'static str,
    set_end: &'static str,
    apply_cut: &'static str,
    listen_cut: &'static str,
    modify_cut: &'static str,
    split_here: &'static str,
    part_singular: &'static str,
    listen: &'static str,
    modify: &'static str,
    delete: &'static str,
    restore: &'static str,
    rotation: &'static str,
    rotation_none: &'static str,
    rotation_right: &'static str,
    rotation_left: &'static str,
    rotation_half: &'static str,
    show_video: &'static str,
    hide_video: &'static str,
    add_track: &'static str,
    save: &'static str,
    cancel_processing: &'static str,
    close: &'static str,
    ready: &'static str,
    no_file: &'static str,
    invalid_media: &'static str,
    file_loaded: &'static str,
    start_set: &'static str,
    end_set: &'static str,
    cut_applied: &'static str,
    invalid_cut: &'static str,
    same_output: &'static str,
    pending_cut_save: &'static str,
    discard_pending_cut: &'static str,
    split_added: &'static str,
    part_deleted: &'static str,
    part_restored: &'static str,
    no_deleted_part: &'static str,
    precision: &'static str,
    start_back: &'static str,
    start_forward: &'static str,
    end_back: &'static str,
    end_forward: &'static str,
    adjusted: &'static str,
    choose_track: &'static str,
    original_volume: &'static str,
    new_volume: &'static str,
    loop_track: &'static str,
    track_added: &'static str,
    processing_started: &'static str,
    processing: &'static str,
    processing_cancelled: &'static str,
    saved: &'static str,
    save_failed: &'static str,
    nothing_to_save: &'static str,
    unsaved_title: &'static str,
    unsaved_message: &'static str,
    part_deleted_marker: &'static str,
}

fn t(lang: &str, it: &'static str, en: &'static str, fr: &'static str, es: &'static str, pt: &'static str, cs: &'static str, pl: &'static str) -> &'static str {
    match lang {
        "en" => en,
        "fr" => fr,
        "es" => es,
        "pt" => pt,
        "cs" => cs,
        "pl" => pl,
        _ => it,
    }
}

fn labels() -> Labels {
    let lang = crate::normalize_ui_language(&crate::Settings::load().ui_language);
    let l = lang.as_str();
    Labels {
        menu: t(l, "Taglia file media…", "Cut media file…", "Découper un fichier multimédia…", "Cortar archivo multimedia…", "Cortar ficheiro multimédia…", "Oříznout multimediální soubor…", "Przytnij plik multimedialny…"),
        title: t(l, "Taglia file media", "Cut media file", "Découper un fichier multimédia", "Cortar archivo multimedia", "Cortar ficheiro multimédia", "Oříznout multimediální soubor", "Przytnij plik multimedialny"),
        open_file: t(l, "Apri file media…", "Open media file…", "Ouvrir un fichier multimédia…", "Abrir archivo multimedia…", "Abrir ficheiro multimédia…", "Otevřít multimediální soubor…", "Otwórz plik multimedialny…"),
        mode: t(l, "Tipo di taglio", "Cut mode", "Mode de découpe", "Tipo de corte", "Modo de corte", "Režim střihu", "Tryb cięcia"),
        guided: t(l, "Taglio guidato", "Guided cut", "Découpe guidée", "Corte guiado", "Corte guiado", "Řízený střih", "Cięcie prowadzone"),
        advanced: t(l, "Taglio avanzato", "Advanced cut", "Découpe avancée", "Corte avanzado", "Corte avançado", "Pokročilý střih", "Cięcie zaawansowane"),
        movement: t(l, "Spostamento", "Movement", "Déplacement", "Desplazamiento", "Deslocamento", "Posun", "Przesunięcie"),
        back: t(l, "Indietro", "Back", "Reculer", "Atrás", "Recuar", "Zpět", "Wstecz"),
        play: t(l, "Riproduci", "Play", "Lire", "Reproducir", "Reproduzir", "Přehrát", "Odtwórz"),
        pause: t(l, "Pausa", "Pause", "Pause", "Pausa", "Pausa", "Pauza", "Pauza"),
        forward: t(l, "Avanti", "Forward", "Avancer", "Adelante", "Avançar", "Vpřed", "Dalej"),
        set_start: t(l, "Inizio taglio", "Set cut start", "Début de coupe", "Inicio del corte", "Início do corte", "Začátek střihu", "Początek cięcia"),
        set_end: t(l, "Fine taglio", "Set cut end", "Fin de coupe", "Fin del corte", "Fim do corte", "Konec střihu", "Koniec cięcia"),
        apply_cut: t(l, "Applica taglio", "Apply cut", "Appliquer la coupe", "Aplicar corte", "Aplicar corte", "Použít střih", "Zastosuj cięcie"),
        listen_cut: t(l, "Ascolta taglio", "Listen to cut", "Écouter la coupe", "Escuchar corte", "Ouvir corte", "Poslechnout střih", "Odsłuchaj cięcie"),
        modify_cut: t(l, "Modifica taglio…", "Adjust cut…", "Modifier la coupe…", "Modificar corte…", "Modificar corte…", "Upravit střih…", "Modyfikuj cięcie…"),
        split_here: t(l, "Dividi qui", "Split here", "Diviser ici", "Dividir aquí", "Dividir aqui", "Rozdělit zde", "Podziel tutaj"),
        part_singular: t(l, "Parte", "Part", "Partie", "Parte", "Parte", "Část", "Część"),
        listen: t(l, "Ascolta", "Listen", "Écouter", "Escuchar", "Ouvir", "Poslechnout", "Odsłuchaj"),
        modify: t(l, "Modifica…", "Adjust…", "Modifier…", "Modificar…", "Modificar…", "Upravit…", "Modyfikuj…"),
        delete: t(l, "Elimina", "Delete", "Supprimer", "Eliminar", "Eliminar", "Odstranit", "Usuń"),
        restore: t(l, "Ripristina parte eliminata", "Restore deleted part", "Restaurer la partie supprimée", "Restaurar parte eliminada", "Restaurar parte eliminada", "Obnovit odstraněnou část", "Przywróć usuniętą część"),
        rotation: t(l, "Rotazione video", "Video rotation", "Rotation vidéo", "Rotación de vídeo", "Rotação do vídeo", "Otočení videa", "Obrót wideo"),
        rotation_none: t(l, "Nessuna", "None", "Aucune", "Ninguna", "Nenhuma", "Žádné", "Brak"),
        rotation_right: t(l, "Destra", "Right", "Droite", "Derecha", "Direita", "Doprava", "W prawo"),
        rotation_left: t(l, "Sinistra", "Left", "Gauche", "Izquierda", "Esquerda", "Doleva", "W lewo"),
        rotation_half: "180°",
        show_video: t(l, "Mostra anteprima video", "Show video preview", "Afficher l’aperçu vidéo", "Mostrar vista previa de vídeo", "Mostrar pré-visualização do vídeo", "Zobrazit náhled videa", "Pokaż podgląd wideo"),
        hide_video: t(l, "Nascondi anteprima video", "Hide video preview", "Masquer l’aperçu vidéo", "Ocultar vista previa de vídeo", "Ocultar pré-visualização do vídeo", "Skrýt náhled videa", "Ukryj podgląd wideo"),
        add_track: t(l, "Aggiungi nuova traccia…", "Add new track…", "Ajouter une nouvelle piste…", "Añadir nueva pista…", "Adicionar nova faixa…", "Přidat novou stopu…", "Dodaj nową ścieżkę…"),
        save: t(l, "Salva…", "Save…", "Enregistrer…", "Guardar…", "Guardar…", "Uložit…", "Zapisz…"),
        cancel_processing: t(l, "Interrompi", "Stop", "Interrompre", "Interrumpir", "Interromper", "Přerušit", "Przerwij"),
        close: t(l, "Chiudi", "Close", "Fermer", "Cerrar", "Fechar", "Zavřít", "Zamknij"),
        ready: t(l, "Pronto", "Ready", "Prêt", "Listo", "Pronto", "Připraveno", "Gotowe"),
        no_file: t(l, "Apri prima un file media.", "Open a media file first.", "Ouvrez d’abord un fichier multimédia.", "Abre primero un archivo multimedia.", "Abra primeiro um ficheiro multimédia.", "Nejprve otevřete multimediální soubor.", "Najpierw otwórz plik multimedialny."),
        invalid_media: t(l, "Il file non contiene audio o video utilizzabile.", "The file contains no usable audio or video.", "Le fichier ne contient aucun audio ou vidéo utilisable.", "El archivo no contiene audio o vídeo utilizable.", "O ficheiro não contém áudio ou vídeo utilizável.", "Soubor neobsahuje použitelný zvuk ani video.", "Plik nie zawiera użytecznego audio ani wideo."),
        file_loaded: t(l, "File caricato", "File loaded", "Fichier chargé", "Archivo cargado", "Ficheiro carregado", "Soubor načten", "Plik wczytany"),
        start_set: t(l, "Inizio taglio impostato", "Cut start set", "Début de coupe défini", "Inicio del corte establecido", "Início do corte definido", "Začátek střihu nastaven", "Ustawiono początek cięcia"),
        end_set: t(l, "Fine taglio impostata", "Cut end set", "Fin de coupe définie", "Fin del corte establecido", "Fim do corte definido", "Konec střihu nastaven", "Ustawiono koniec cięcia"),
        cut_applied: t(l, "Taglio applicato", "Cut applied", "Coupe appliquée", "Corte aplicado", "Corte aplicado", "Střih použit", "Cięcie zastosowane"),
        invalid_cut: t(l, "I punti del taglio non sono validi.", "The cut points are not valid.", "Les points de coupe ne sont pas valides.", "Los puntos de corte no son válidos.", "Os pontos de corte não são válidos.", "Body střihu nejsou platné.", "Punkty cięcia są nieprawidłowe."),
        same_output: t(l, "Scegli un file di destinazione diverso dal file originale.", "Choose an output file different from the original file.", "Choisissez un fichier de destination différent du fichier d’origine.", "Elige un archivo de destino diferente del archivo original.", "Escolha um ficheiro de destino diferente do ficheiro original.", "Vyberte jiný cílový soubor než původní soubor.", "Wybierz plik docelowy inny niż plik oryginalny."),
        pending_cut_save: t(l, "Prima di salvare, completa il taglio guidato in corso.", "Before saving, complete or cancel the current guided cut.", "Avant d’enregistrer, terminez ou annulez la coupe guidée en cours.", "Antes de guardar, completa o cancela el corte guiado en curso.", "Antes de guardar, conclua ou cancele o corte guiado em curso.", "Před uložením dokončete nebo zrušte probíhající řízený střih.", "Przed zapisaniem zakończ lub anuluj bieżące cięcie prowadzone."),
        discard_pending_cut: t(l, "C’è un taglio guidato non ancora applicato. Vuoi scartarlo e passare al taglio avanzato?", "There is a guided cut that has not been applied yet. Discard it and switch to advanced cut?", "Une coupe guidée n’a pas encore été appliquée. Voulez-vous l’abandonner et passer à la coupe avancée ?", "Hay un corte guiado que aún no se ha aplicado. ¿Quieres descartarlo y pasar al corte avanzado?", "Existe um corte guiado ainda não aplicado. Pretende descartá-lo e mudar para o corte avançado?", "Existuje dosud nepoužitý řízený střih. Chcete jej zahodit a přejít na pokročilý střih?", "Istnieje niezatwierdzone cięcie prowadzone. Odrzucić je i przejść do cięcia zaawansowanego?"),
        split_added: t(l, "Divisione aggiunta", "Split added", "Division ajoutée", "División añadida", "Divisão adicionada", "Rozdělení přidáno", "Dodano podział"),
        part_deleted: t(l, "Parte eliminata", "Part deleted", "Partie supprimée", "Parte eliminada", "Parte eliminada", "Část odstraněna", "Część usunięta"),
        part_restored: t(l, "Parte ripristinata", "Part restored", "Partie restaurée", "Parte restaurada", "Parte restaurada", "Část obnovena", "Część przywrócona"),
        no_deleted_part: t(l, "Non ci sono parti eliminate da ripristinare.", "There are no deleted parts to restore.", "Aucune partie supprimée à restaurer.", "No hay partes eliminadas para restaurar.", "Não existem partes eliminadas para restaurar.", "Nejsou žádné odstraněné části k obnovení.", "Brak usuniętych części do przywrócenia."),
        precision: t(l, "Precisione", "Precision", "Précision", "Precisión", "Precisão", "Přesnost", "Precyzja"),
        start_back: t(l, "Inizio indietro", "Start back", "Début en arrière", "Inicio atrás", "Início para trás", "Začátek zpět", "Początek wstecz"),
        start_forward: t(l, "Inizio avanti", "Start forward", "Début en avant", "Inicio adelante", "Início para a frente", "Začátek vpřed", "Początek dalej"),
        end_back: t(l, "Fine indietro", "End back", "Fin en arrière", "Fin atrás", "Fim para trás", "Konec zpět", "Koniec wstecz"),
        end_forward: t(l, "Fine avanti", "End forward", "Fin en avant", "Fin adelante", "Fim para a frente", "Konec vpřed", "Koniec dalej"),
        adjusted: t(l, "Taglio aggiornato", "Cut updated", "Coupe mise à jour", "Corte actualizado", "Corte atualizado", "Střih upraven", "Cięcie zaktualizowane"),
        choose_track: t(l, "Scegli traccia audio…", "Choose audio track…", "Choisir une piste audio…", "Elegir pista de audio…", "Escolher faixa de áudio…", "Vybrat zvukovou stopu…", "Wybierz ścieżkę audio…"),
        original_volume: t(l, "Volume originale", "Original volume", "Volume original", "Volumen original", "Volume original", "Původní hlasitost", "Głośność oryginalna"),
        new_volume: t(l, "Volume nuova traccia", "New track volume", "Volume de la nouvelle piste", "Volumen de la nueva pista", "Volume da nova faixa", "Hlasitost nové stopy", "Głośność nowej ścieżki"),
        loop_track: t(l, "Ripeti la nuova traccia se termina prima", "Loop the new track if it ends first", "Répéter la nouvelle piste si elle se termine avant", "Repetir la nueva pista si termina antes", "Repetir a nova faixa se terminar antes", "Opakovat novou stopu, pokud skončí dříve", "Zapętl nową ścieżkę, jeśli skończy się wcześniej"),
        track_added: t(l, "Nuova traccia configurata", "New track configured", "Nouvelle piste configurée", "Nueva pista configurada", "Nova faixa configurada", "Nová stopa nastavena", "Nowa ścieżka skonfigurowana"),
        processing_started: t(l, "Elaborazione iniziata", "Processing started", "Traitement commencé", "Procesamiento iniciado", "Processamento iniciado", "Zpracování zahájeno", "Rozpoczęto przetwarzanie"),
        processing: t(l, "Elaborazione", "Processing", "Traitement", "Procesando", "A processar", "Zpracování", "Przetwarzanie"),
        processing_cancelled: t(l, "Elaborazione interrotta", "Processing stopped", "Traitement interrompu", "Procesamiento interrumpido", "Processamento interrompido", "Zpracování přerušeno", "Przetwarzanie przerwane"),
        saved: t(l, "File salvato", "File saved", "Fichier enregistré", "Archivo guardado", "Ficheiro guardado", "Soubor uložen", "Plik zapisany"),
        save_failed: t(l, "Salvataggio non riuscito", "Save failed", "Échec de l’enregistrement", "Error al guardar", "Falha ao guardar", "Uložení se nezdařilo", "Zapisywanie nie powiodło się"),
        nothing_to_save: t(l, "Non ci sono parti da salvare.", "There are no parts to save.", "Aucune partie à enregistrer.", "No hay partes para guardar.", "Não existem partes para guardar.", "Nejsou žádné části k uložení.", "Brak części do zapisania."),
        unsaved_title: t(l, "Modifiche non salvate", "Unsaved changes", "Modifications non enregistrées", "Cambios sin guardar", "Alterações não guardadas", "Neuložené změny", "Niezapisane zmiany"),
        unsaved_message: t(l, "Ci sono tagli non salvati. Vuoi chiudere comunque?", "There are unsaved cuts. Close anyway?", "Des coupes ne sont pas enregistrées. Fermer quand même ?", "Hay cortes sin guardar. ¿Cerrar de todos modos?", "Existem cortes não guardados. Fechar mesmo assim?", "Existují neuložené střihy. Přesto zavřít?", "Istnieją niezapisane cięcia. Zamknąć mimo to?"),
        part_deleted_marker: t(l, "eliminata", "deleted", "supprimée", "eliminada", "eliminada", "odstraněna", "usunięta"),
    }
}

pub fn menu_label() -> String {
    labels().menu.to_string()
}

fn format_time(seconds: f64) -> String {
    let total_ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let hours = total_ms / 3_600_000;
    let minutes = (total_ms % 3_600_000) / 60_000;
    let secs = (total_ms % 60_000) / 1000;
    let tenths = (total_ms % 1000) / 100;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}.{tenths}")
    } else {
        format!("{minutes}:{secs:02}.{tenths}")
    }
}

fn format_duration_natural(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        format!("{hours} h {minutes} min {secs} s")
    } else if minutes > 0 {
        format!("{minutes} min {secs} s")
    } else {
        format!("{secs} s")
    }
}

fn ffprobe_path() -> Option<PathBuf> {
    if let Some(ffmpeg) = ffmpeg_executable_path()
        && let Some(parent) = ffmpeg.parent()
    {
        let candidate = parent.join("ffprobe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let system = PathBuf::from("ffprobe");
    Command::new(&system)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()
        .filter(|status| status.success())
        .map(|_| system)
}

fn parse_ffmpeg_clock(value: &str) -> Option<f64> {
    let mut parts = value.trim().split(':');
    let hours = parts.next()?.parse::<f64>().ok()?;
    let minutes = parts.next()?.parse::<f64>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let total = hours * 3600.0 + minutes * 60.0 + seconds;
    (total.is_finite() && total > 0.0).then_some(total)
}

fn probe_media_with_ffmpeg(path: &Path) -> Result<ProbeInfo, String> {
    let ffmpeg = ffmpeg_executable_path().unwrap_or_else(|| PathBuf::from("ffmpeg"));
    let mut command = Command::new(&ffmpeg);
    if let Some(parent) = ffmpeg.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        command.current_dir(parent);
    }
    let output = command
        .args(["-hide_banner", "-i"])
        .arg(path)
        .args(["-t", "0", "-f", "null", "-"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| format!("FFmpeg: {err}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let duration = stderr
        .lines()
        .find_map(|line| {
            let marker = "Duration: ";
            let start = line.find(marker)? + marker.len();
            let rest = &line[start..];
            let end = rest.find(',').unwrap_or(rest.len());
            parse_ffmpeg_clock(&rest[..end])
        })
        .ok_or_else(|| "durata media non disponibile".to_string())?;
    let mut has_video = false;
    let mut has_audio = false;
    for line in stderr.lines().filter(|line| line.trim_start().starts_with("Stream #")) {
        if line.contains("Audio:") {
            has_audio = true;
        }
        if line.contains("Video:") && !line.to_ascii_lowercase().contains("attached pic") {
            has_video = true;
        }
    }
    if !has_audio && !has_video {
        return Err("nessuna traccia audio o video utilizzabile".to_string());
    }
    Ok(ProbeInfo {
        duration,
        has_video,
        has_audio,
    })
}

fn probe_media(path: &Path) -> Result<ProbeInfo, String> {
    if let Some(ffprobe) = ffprobe_path() {
        let ffprobe_result = (|| {
            let output = Command::new(&ffprobe)
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "format=duration",
                    "-show_entries",
                    "stream=codec_type:stream_disposition=attached_pic",
                    "-of",
                    "json",
                ])
                .arg(path)
                .output()
                .map_err(|err| format!("ffprobe: {err}"))?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
            }
            let value: serde_json::Value = serde_json::from_slice(&output.stdout)
                .map_err(|err| format!("ffprobe JSON: {err}"))?;
            let duration = value
                .get("format")
                .and_then(|v| v.get("duration"))
                .and_then(|v| {
                    v.as_str()
                        .and_then(|text| text.parse::<f64>().ok())
                        .or_else(|| v.as_f64())
                })
                .filter(|v| v.is_finite() && *v > 0.0)
                .ok_or_else(|| "durata media non disponibile".to_string())?;
            let mut has_video = false;
            let mut has_audio = false;
            if let Some(streams) = value.get("streams").and_then(|v| v.as_array()) {
                for stream in streams {
                    match stream.get("codec_type").and_then(|v| v.as_str()) {
                        Some("video") => {
                            let attached_picture = stream
                                .get("disposition")
                                .and_then(|v| v.get("attached_pic"))
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0)
                                != 0;
                            if !attached_picture {
                                has_video = true;
                            }
                        }
                        Some("audio") => has_audio = true,
                        _ => {}
                    }
                }
            }
            if !has_audio && !has_video {
                return Err("nessuna traccia audio o video utilizzabile".to_string());
            }
            Ok(ProbeInfo {
                duration,
                has_video,
                has_audio,
            })
        })();
        if let Ok(info) = ffprobe_result {
            return Ok(info);
        }
        append_podcast_log(&format!(
            "media_cutter.probe ffprobe_failed path={} falling_back_to_ffmpeg",
            path.display()
        ));
    } else {
        append_podcast_log(&format!(
            "media_cutter.probe ffprobe_unavailable path={} using_ffmpeg",
            path.display()
        ));
    }
    probe_media_with_ffmpeg(path)
}

fn mpv_path() -> PathBuf {
    crate::podcast_player::bundled_mpv_executable_path().unwrap_or_else(|| PathBuf::from("mpv"))
}

fn spawn_preview(path: &Path, position: f64, stop_at: Option<f64>, show_video: bool) -> Result<Child, String> {
    let mpv = mpv_path();
    let mut command = Command::new(&mpv);
    if let Some(parent) = mpv.parent().filter(|p| !p.as_os_str().is_empty()) {
        command.current_dir(parent);
    }
    command
        .arg(path)
        .arg("--no-terminal")
        .arg("--idle=no")
        .arg("--keep-open=no")
        .arg("--audio-channels=stereo")
        .arg(format!("--start={:.3}", position.max(0.0)));
    if let Some(end) = stop_at {
        let length = (end - position).max(0.01);
        command.arg(format!("--length={length:.3}"));
    }
    if show_video {
        command.arg("--force-window=yes").arg("--osc=yes");
    } else {
        command.arg("--vid=no").arg("--force-window=no");
    }
    command.stdout(Stdio::null()).stderr(Stdio::null());
    command.spawn().map_err(|err| format!("MPV: {err}"))
}

fn start_preview(preview: &Rc<RefCell<PreviewState>>, path: &Path, duration: f64, position: f64, stop_at: Option<f64>, show_video: bool) -> Result<(), String> {
    let mut state = preview.borrow_mut();
    state.stop_child();
    let pos = position.clamp(0.0, duration.max(0.0));
    let child = spawn_preview(path, pos, stop_at, show_video)?;
    state.child = Some(child);
    state.playing = true;
    state.base_position = pos;
    state.started_at = Some(Instant::now());
    state.stop_at = stop_at;
    Ok(())
}

fn movement_seconds(choice: Choice) -> f64 {
    match choice.get_selection().unwrap_or(1) {
        0 => 1.0,
        1 => 5.0,
        2 => 10.0,
        3 => 30.0,
        4 => 60.0,
        5 => 120.0,
        6 => 300.0,
        7 => 600.0,
        _ => 5.0,
    }
}

fn seek_preview(
    direction: f64,
    movement_choice: Choice,
    input: &Rc<RefCell<Option<PathBuf>>>,
    probe: &Rc<RefCell<Option<ProbeInfo>>>,
    preview: &Rc<RefCell<PreviewState>>,
    show_video: &Rc<Cell<bool>>,
    dialog: &Dialog,
    labels: Labels,
) {
    let Some(path) = input.borrow().clone() else {
        show_info(dialog, labels.title, labels.no_file);
        return;
    };
    let Some(info) = probe.borrow().clone() else {
        return;
    };
    let step = movement_seconds(movement_choice);
    let was_playing = preview.borrow().playing;
    let new_position = (preview.borrow().position(info.duration) + direction * step)
        .clamp(0.0, info.duration);
    preview.borrow_mut().pause(info.duration);
    preview.borrow_mut().base_position = new_position;
    if was_playing {
        let _ = start_preview(
            preview,
            &path,
            info.duration,
            new_position,
            None,
            show_video.get(),
        );
    }
    announce_voiceover_message(&format_time(new_position));
}

fn cut_precision_seconds(choice: Choice) -> f64 {
    match choice.get_selection().unwrap_or(0) {
        1 => 0.5,
        2 => 0.25,
        3 => 0.10,
        _ => 1.0,
    }
}

fn adjust_cut_cells(
    start: &Rc<Cell<f64>>,
    end: &Rc<Cell<f64>>,
    move_start: bool,
    direction: f64,
    precision: Choice,
    summary: StaticText,
    min_start: f64,
    max_end: f64,
    labels: Labels,
) {
    let step = cut_precision_seconds(precision);
    if move_start {
        let candidate = (start.get() + direction * step)
            .clamp(min_start, end.get() - MIN_PART_SECONDS);
        start.set(candidate);
    } else {
        let candidate = (end.get() + direction * step)
            .clamp(start.get() + MIN_PART_SECONDS, max_end);
        end.set(candidate);
    }
    summary.set_label(&format!(
        "{} – {}",
        format_time(start.get()),
        format_time(end.get())
    ));
    announce_voiceover_message(&format!(
        "{}: {} – {}",
        labels.adjusted,
        format_time(start.get()),
        format_time(end.get())
    ));
}

fn clamp_parts(parts: &mut Vec<MediaPart>, duration: f64) {
    parts.retain(|p| p.end - p.start >= MIN_PART_SECONDS / 2.0);
    for p in parts.iter_mut() {
        p.start = p.start.clamp(0.0, duration);
        p.end = p.end.clamp(0.0, duration);
    }
    parts.sort_by(|a, b| a.start.total_cmp(&b.start));
}

fn split_part_at(parts: &mut Vec<MediaPart>, point: f64) -> Option<usize> {
    for i in 0..parts.len() {
        if parts[i].keep
            && point > parts[i].start + MIN_PART_SECONDS
            && point < parts[i].end - MIN_PART_SECONDS
        {
            let original = parts[i].clone();
            parts[i].end = point;
            parts.insert(i + 1, MediaPart { start: point, end: original.end, keep: original.keep });
            return Some(i + 1);
        }
    }
    None
}

fn delete_range(
    parts: &mut Vec<MediaPart>,
    start: f64,
    end: f64,
    duration: f64,
) -> Vec<(f64, f64)> {
    if end - start < MIN_PART_SECONDS {
        return Vec::new();
    }
    let _ = split_part_at(parts, start);
    let _ = split_part_at(parts, end);
    let mut deleted = Vec::new();
    for part in parts.iter_mut() {
        if part.end > start && part.start < end && part.keep {
            part.keep = false;
            deleted.push((part.start, part.end));
        }
    }
    clamp_parts(parts, duration);
    deleted
}

fn adjust_part_boundary(parts: &mut [MediaPart], index: usize, move_start: bool, delta: f64, duration: f64) -> bool {
    if index >= parts.len() {
        return false;
    }
    if move_start {
        let min = if index == 0 {
            0.0
        } else {
            parts[index - 1].start + MIN_PART_SECONDS
        };
        let max = parts[index].end - MIN_PART_SECONDS;
        let new_boundary = (parts[index].start + delta).clamp(min, max).clamp(0.0, duration);
        if (new_boundary - parts[index].start).abs() < 0.0001 {
            return false;
        }
        if index > 0 {
            parts[index - 1].end = new_boundary;
        }
        parts[index].start = new_boundary;
        true
    } else {
        let min = parts[index].start + MIN_PART_SECONDS;
        let max = if index + 1 >= parts.len() {
            duration
        } else {
            parts[index + 1].end - MIN_PART_SECONDS
        };
        let new_boundary = (parts[index].end + delta).clamp(min, max).clamp(0.0, duration);
        if (new_boundary - parts[index].end).abs() < 0.0001 {
            return false;
        }
        parts[index].end = new_boundary;
        if index + 1 < parts.len() {
            parts[index + 1].start = new_boundary;
        }
        true
    }
}

fn part_label(labels: &Labels, index: usize, part: &MediaPart) -> String {
    if part.keep {
        format!("{} {}, {} – {}", labels.part_singular, index + 1, format_time(part.start), format_time(part.end))
    } else {
        format!("{} {}, {}, {} – {}", labels.part_singular, index + 1, labels.part_deleted_marker, format_time(part.start), format_time(part.end))
    }
}

fn refresh_parts_choice(choice: Choice, parts: &[MediaPart], labels: &Labels, preferred: Option<usize>) {
    choice.clear();
    for (index, part) in parts.iter().enumerate() {
        choice.append(&part_label(labels, index, part));
    }
    if !parts.is_empty() {
        let selected = preferred.unwrap_or(0).min(parts.len() - 1);
        choice.set_selection(selected as u32);
    }
}

fn show_info(parent: &Dialog, title: &str, message: &str) {
    let dialog = MessageDialog::builder(parent, message, title)
        .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation)
        .build();
    let _ = dialog.show_modal();
}

fn show_error(parent: &Dialog, title: &str, message: &str) {
    let dialog = MessageDialog::builder(parent, message, title)
        .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
        .build();
    let _ = dialog.show_modal();
}

fn ask_yes_no(parent: &Dialog, title: &str, message: &str) -> bool {
    let dialog = MessageDialog::builder(parent, message, title)
        .with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconQuestion)
        .build();
    dialog.show_modal() == ID_YES
}

fn choose_input(parent: &Dialog, labels: &Labels) -> Option<PathBuf> {
    let wildcard = "Media|*.mp3;*.m4a;*.mp4;*.aac;*.mkv;*.avi;*.mov;*.m4v;*.webm;*.mpg;*.mpeg;*.ts;*.m2ts;*.mts;*.wmv;*.asf;*.flv;*.vob;*.3gp;*.flac;*.ogg;*.opus;*.wma;*.aiff;*.aif;*.m4b;*.wav|Tutti|*.*";
    let dialog = FileDialog::builder(parent)
        .with_message(labels.open_file)
        .with_wildcard(wildcard)
        .build();
    crate::set_mac_native_file_dialog_open(true);
    let result = dialog.show_modal();
    crate::set_mac_native_file_dialog_open(false);
    if result != ID_OK {
        return None;
    }
    crate::resolve_file_dialog_path(&dialog, true)
}

fn choose_audio_track(parent: &Dialog, labels: &Labels) -> Option<PathBuf> {
    let wildcard = "Audio|*.mp3;*.m4a;*.aac;*.flac;*.ogg;*.opus;*.wma;*.aiff;*.aif;*.m4b;*.wav|Tutti|*.*";
    let dialog = FileDialog::builder(parent)
        .with_message(labels.choose_track)
        .with_wildcard(wildcard)
        .build();
    crate::set_mac_native_file_dialog_open(true);
    let result = dialog.show_modal();
    crate::set_mac_native_file_dialog_open(false);
    if result != ID_OK {
        return None;
    }
    crate::resolve_file_dialog_path(&dialog, true)
}

fn choose_output(parent: &Dialog, input: &Path, has_video: bool, labels: &Labels) -> Option<PathBuf> {
    let stem = input.file_stem().and_then(|v| v.to_str()).unwrap_or("media");
    let ext = if has_video { "mp4" } else { "m4a" };
    let default_name = format!("{stem}_tagliato.{ext}");
    let wildcard = if has_video { "MP4|*.mp4" } else { "M4A|*.m4a" };
    let dialog = FileDialog::builder(parent)
        .with_message(labels.save)
        .with_wildcard(wildcard)
        .with_default_file(&default_name)
        .with_style(FileDialogStyle::Save | FileDialogStyle::OverwritePrompt)
        .build();
    crate::set_mac_native_file_dialog_open(true);
    let result = dialog.show_modal();
    crate::set_mac_native_file_dialog_open(false);
    if result != ID_OK {
        return None;
    }
    let mut path = crate::resolve_file_dialog_path(&dialog, false)?;
    if path.extension().and_then(|value| value.to_str()).is_none() {
        path.set_extension(ext);
    }
    Some(path)
}

fn rotation_filter(rotation: VideoRotation) -> Option<&'static str> {
    match rotation {
        VideoRotation::None => None,
        VideoRotation::Right => Some("transpose=1"),
        VideoRotation::Left => Some("transpose=2"),
        VideoRotation::Half => Some("hflip,vflip"),
    }
}

fn ffmpeg_time(seconds: f64) -> String {
    format!("{:.3}", seconds.max(0.0))
}

fn run_ffmpeg_step(
    args: &[String],
    step_start: i32,
    step_span: i32,
    expected_duration: f64,
    progress: &Arc<Mutex<ExportProgress>>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    if cancel.load(Ordering::SeqCst) {
        return Err("__CANCELLED__".to_string());
    }
    let ffmpeg = ffmpeg_executable_path().unwrap_or_else(|| PathBuf::from("ffmpeg"));
    let mut command = Command::new(&ffmpeg);
    command.args(args).stdout(Stdio::null()).stderr(Stdio::piped());
    if let Some(parent) = ffmpeg.parent().filter(|p| !p.as_os_str().is_empty()) {
        command.current_dir(parent);
    }
    append_podcast_log(&format!("media_cutter.ffmpeg args={args:?}"));
    let mut child = command.spawn().map_err(|err| format!("FFmpeg: {err}"))?;
    let stderr = child.stderr.take().ok_or_else(|| "FFmpeg stderr non disponibile".to_string())?;
    let tail = Arc::new(Mutex::new(String::new()));
    let tail_reader = Arc::clone(&tail);
    let progress_reader = Arc::clone(progress);
    let reader = std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader.read_until(b'\r', &mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buffer).to_string();
                    {
                        let mut out = tail_reader.lock().unwrap();
                        out.push_str(&line);
                        if out.len() > 12_000 {
                            let target = out.len().saturating_sub(6_000);
                            let keep_from = out
                                .char_indices()
                                .map(|(index, _)| index)
                                .find(|index| *index >= target)
                                .unwrap_or(0);
                            out.drain(..keep_from);
                        }
                    }
                    if expected_duration > 0.0 {
                        if let Some(pos) = line.find("time=") {
                            let raw = &line[pos + 5..];
                            let end = raw.find(' ').unwrap_or(raw.len());
                            let parts: Vec<&str> = raw[..end].split(':').collect();
                            if parts.len() == 3 {
                                let h = parts[0].parse::<f64>().unwrap_or(0.0);
                                let m = parts[1].parse::<f64>().unwrap_or(0.0);
                                let s = parts[2].parse::<f64>().unwrap_or(0.0);
                                let current = h * 3600.0 + m * 60.0 + s;
                                let local = (current / expected_duration).clamp(0.0, 1.0);
                                progress_reader.lock().unwrap().percent = step_start + (local * step_span as f64) as i32;
                            }
                        }
                    }
                }
            }
        }
    });
    let status = loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(err) => break Err(err),
        }
    };
    let _ = reader.join();
    if cancel.load(Ordering::SeqCst) {
        return Err("__CANCELLED__".to_string());
    }
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(format!("FFmpeg: {}", tail.lock().unwrap().trim())),
        Err(err) => Err(format!("FFmpeg: {err}")),
    }
}

fn write_concat_file(path: &Path, segments: &[PathBuf]) -> Result<(), String> {
    let mut file = File::create(path).map_err(|err| err.to_string())?;
    for segment in segments {
        let escaped = segment.to_string_lossy().replace('\\', "\\\\").replace('\'', "'\\''");
        writeln!(file, "file '{escaped}'").map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn copy_file(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::copy(src, dst).map(|_| ()).map_err(|err| err.to_string())
}

fn export_media(snapshot: ExportSnapshot, progress: Arc<Mutex<ExportProgress>>, cancel: Arc<AtomicBool>) {
    let work_dir = std::env::temp_dir().join(format!("sonarpad_media_cutter_{}", Uuid::new_v4().simple()));
    let result = (|| -> Result<PathBuf, String> {
        std::fs::create_dir_all(&work_dir).map_err(|err| err.to_string())?;
        let kept: Vec<MediaPart> = snapshot.parts.iter().filter(|p| p.keep && p.duration() >= MIN_PART_SECONDS).cloned().collect();
        if kept.is_empty() {
            return Err("nessuna parte da salvare".to_string());
        }
        let ext = if snapshot.probe.has_video { "mp4" } else { "m4a" };
        let total_duration: f64 = kept.iter().map(MediaPart::duration).sum();
        let segment_span = (82_i32 / kept.len().max(1) as i32).max(1);
        let mut segments = Vec::with_capacity(kept.len());
        for (index, part) in kept.iter().enumerate() {
            if cancel.load(Ordering::SeqCst) {
                return Err("__CANCELLED__".to_string());
            }
            {
                let mut p = progress.lock().unwrap();
                p.message = format!("{} {}/{}", labels().processing, index + 1, kept.len());
            }
            let segment = work_dir.join(format!("segment_{index:03}.{ext}"));
            let mut args = vec![
                "-y".to_string(),
                "-fflags".to_string(), "+genpts".to_string(),
                "-ss".to_string(), ffmpeg_time(part.start),
                "-i".to_string(), snapshot.input.to_string_lossy().to_string(),
                "-t".to_string(), ffmpeg_time(part.duration()),
            ];
            if snapshot.probe.has_video {
                args.extend(["-map", "0:v:0", "-map", "0:a:0?"].into_iter().map(str::to_string));
                if let Some(filter) = rotation_filter(snapshot.rotation) {
                    args.extend(["-vf".to_string(), filter.to_string()]);
                }
                args.extend(["-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-ac", "2", "-movflags", "+faststart", "-avoid_negative_ts", "make_zero"].into_iter().map(str::to_string));
            } else {
                args.extend(["-vn", "-map", "0:a:0", "-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-ac", "2"].into_iter().map(str::to_string));
            }
            args.push(segment.to_string_lossy().to_string());
            let start = (index as i32 * segment_span).min(80);
            run_ffmpeg_step(&args, start, segment_span, part.duration(), &progress, &cancel)?;
            segments.push(segment);
        }

        let assembled = work_dir.join(format!("assembled.{ext}"));
        if segments.len() == 1 {
            copy_file(&segments[0], &assembled)?;
        } else {
            let list_path = work_dir.join("concat.txt");
            write_concat_file(&list_path, &segments)?;
            let args = vec![
                "-y".to_string(), "-f".to_string(), "concat".to_string(), "-safe".to_string(), "0".to_string(),
                "-i".to_string(), list_path.to_string_lossy().to_string(), "-c".to_string(), "copy".to_string(),
                "-avoid_negative_ts".to_string(), "make_zero".to_string(), assembled.to_string_lossy().to_string(),
            ];
            run_ffmpeg_step(&args, 83, 7, total_duration, &progress, &cancel)?;
        }

        let final_temp = work_dir.join(format!("final.{ext}"));
        if let Some(track) = snapshot.added_track.clone() {
            let mut args = vec!["-y".to_string(), "-i".to_string(), assembled.to_string_lossy().to_string()];
            if track.loop_track {
                args.extend(["-stream_loop".to_string(), "-1".to_string()]);
            }
            args.extend(["-i".to_string(), track.path.to_string_lossy().to_string()]);
            let original_gain = (track.original_volume as f64 / 100.0).clamp(0.0, 2.0);
            let new_gain = (track.new_volume as f64 / 100.0).clamp(0.0, 2.0);
            if snapshot.probe.has_audio {
                args.extend([
                    "-filter_complex".to_string(),
                    format!("[0:a:0]volume={original_gain:.3},aresample=48000,aformat=channel_layouts=stereo[orig];[1:a:0]volume={new_gain:.3},aresample=48000,aformat=channel_layouts=stereo[added];[orig][added]amix=inputs=2:duration=first:dropout_transition=0:normalize=0,alimiter=limit=0.98[outa]"),
                    "-map".to_string(), "0:v:0?".to_string(), "-map".to_string(), "[outa]".to_string(),
                ]);
            } else {
                args.extend(["-map".to_string(), "0:v:0?".to_string(), "-map".to_string(), "1:a:0".to_string(), "-filter:a".to_string(), format!("volume={new_gain:.3},aresample=48000,aformat=channel_layouts=stereo")]);
            }
            if snapshot.probe.has_video {
                args.extend(["-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart"].into_iter().map(str::to_string));
            } else {
                args.extend(["-vn", "-c:a", "aac", "-b:a", "192k"].into_iter().map(str::to_string));
            }
            args.extend(["-t".to_string(), ffmpeg_time(total_duration)]);
            args.push(final_temp.to_string_lossy().to_string());
            run_ffmpeg_step(&args, 91, 8, total_duration, &progress, &cancel)?;
        } else {
            copy_file(&assembled, &final_temp)?;
            progress.lock().unwrap().percent = 98;
        }
        if cancel.load(Ordering::SeqCst) {
            return Err("__CANCELLED__".to_string());
        }
        let final_probe = probe_media(&final_temp)?;
        let duration_tolerance = (total_duration * 0.02).max(2.0);
        if (final_probe.duration - total_duration).abs() > duration_tolerance {
            return Err(format!(
                "durata finale inattesa: {:.2}s invece di {:.2}s",
                final_probe.duration, total_duration
            ));
        }
        if snapshot.probe.has_video && !final_probe.has_video {
            return Err("il file finale non contiene il video".to_string());
        }
        if (snapshot.probe.has_audio || snapshot.added_track.is_some()) && !final_probe.has_audio {
            return Err("il file finale non contiene l'audio".to_string());
        }
        if let Some(parent) = snapshot.output.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let pending_name = format!(
            ".{}.sonarpad-pending-{}",
            snapshot
                .output
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| "media".to_string()),
            Uuid::new_v4().simple()
        );
        let pending = snapshot
            .output
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(pending_name);
        copy_file(&final_temp, &pending)?;
        std::fs::rename(&pending, &snapshot.output).map_err(|err| {
            let _ = std::fs::remove_file(&pending);
            err.to_string()
        })?;
        progress.lock().unwrap().percent = 100;
        Ok(snapshot.output)
    })();

    let _ = std::fs::remove_dir_all(&work_dir);
    let mut state = progress.lock().unwrap();
    state.finished = true;
    state.result = Some(result);
}

fn show_cut_adjust_dialog(
    parent: &Dialog,
    labels: Labels,
    start: Rc<Cell<f64>>,
    end: Rc<Cell<f64>>,
    min_start: f64,
    max_end: f64,
    preview_path: PathBuf,
    preview: Rc<RefCell<PreviewState>>,
    duration: f64,
    show_video: Rc<Cell<bool>>,
) {
    let dialog = Dialog::builder(parent, labels.modify_cut)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(560, 340)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let summary = StaticText::builder(&panel)
        .with_label(&format!("{} – {}", format_time(start.get()), format_time(end.get())))
        .build();
    root.add(&summary, 0, SizerFlag::Expand | SizerFlag::All, 8);
    let precision = Choice::builder(&panel).build();
    for value in ["1 s", "0,5 s", "0,25 s", "0,10 s"] {
        precision.append(value);
    }
    precision.set_selection(0);
    root.add(&StaticText::builder(&panel).with_label(labels.precision).build(), 0, SizerFlag::All, 6);
    root.add(&precision, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let start_back = Button::builder(&panel).with_label(labels.start_back).build();
    let start_forward = Button::builder(&panel).with_label(labels.start_forward).build();
    let end_back = Button::builder(&panel).with_label(labels.end_back).build();
    let end_forward = Button::builder(&panel).with_label(labels.end_forward).build();
    buttons.add(&start_back, 1, SizerFlag::All, 4);
    buttons.add(&start_forward, 1, SizerFlag::All, 4);
    buttons.add(&end_back, 1, SizerFlag::All, 4);
    buttons.add(&end_forward, 1, SizerFlag::All, 4);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    let bottom = BoxSizer::builder(Orientation::Horizontal).build();
    let listen = Button::builder(&panel).with_label(labels.listen_cut).build();
    let close = Button::builder(&panel).with_label(labels.close).build();
    bottom.add(&listen, 1, SizerFlag::All, 6);
    bottom.add(&close, 1, SizerFlag::All, 6);
    root.add_sizer(&bottom, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);
    dialog.set_escape_id(ID_CANCEL);

    let start_back_start = Rc::clone(&start);
    let start_back_end = Rc::clone(&end);
    start_back.on_click(move |_| {
        adjust_cut_cells(&start_back_start, &start_back_end, true, -1.0, precision, summary, min_start, max_end, labels);
    });
    let start_forward_start = Rc::clone(&start);
    let start_forward_end = Rc::clone(&end);
    start_forward.on_click(move |_| {
        adjust_cut_cells(&start_forward_start, &start_forward_end, true, 1.0, precision, summary, min_start, max_end, labels);
    });
    let end_back_start = Rc::clone(&start);
    let end_back_end = Rc::clone(&end);
    end_back.on_click(move |_| {
        adjust_cut_cells(&end_back_start, &end_back_end, false, -1.0, precision, summary, min_start, max_end, labels);
    });
    let end_forward_start = Rc::clone(&start);
    let end_forward_end = Rc::clone(&end);
    end_forward.on_click(move |_| {
        adjust_cut_cells(&end_forward_start, &end_forward_end, false, 1.0, precision, summary, min_start, max_end, labels);
    });

    let preview_listen = Rc::clone(&preview);
    let path_listen = preview_path.clone();
    let show_video_listen = Rc::clone(&show_video);
    listen.on_click(move |_| {
        if let Err(err) = start_preview(&preview_listen, &path_listen, duration, start.get(), Some(end.get()), show_video_listen.get()) {
            show_error(&dialog, labels.title, &err);
        }
    });
    let dialog_close = dialog;
    close.on_click(move |_| dialog_close.end_modal(ID_OK));
    dialog.show_modal();
    dialog.destroy();
}

fn show_add_track_dialog(parent: &Dialog, initial: Option<AddedTrackSettings>, labels: Labels) -> Option<AddedTrackSettings> {
    let dialog = Dialog::builder(parent, labels.add_track)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(560, 360)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let selected_path = Rc::new(RefCell::new(initial.as_ref().map(|v| v.path.clone())));
    let initial_name = selected_path
        .borrow()
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let selected = StaticText::builder(&panel)
        .with_label(&initial_name)
        .build();
    selected.show(!initial_name.is_empty());
    let choose = Button::builder(&panel).with_label(labels.choose_track).build();
    root.add(&choose, 0, SizerFlag::Expand | SizerFlag::All, 8);
    root.add(&selected, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);

    let volume_values = [0, 25, 30, 50, 75, 100, 125, 150, 200];
    let original = Choice::builder(&panel).build();
    let added = Choice::builder(&panel).build();
    for value in volume_values {
        original.append(&format!("{value}%"));
        added.append(&format!("{value}%"));
    }
    let orig_initial = initial.as_ref().map(|v| v.original_volume).unwrap_or(100);
    let add_initial = initial.as_ref().map(|v| v.new_volume).unwrap_or(30);
    let closest = |value: i32| -> u32 {
        volume_values.iter().enumerate().min_by_key(|(_, v)| (**v - value).abs()).map(|(i, _)| i as u32).unwrap_or(0)
    };
    original.set_selection(closest(orig_initial));
    added.set_selection(closest(add_initial));
    root.add(&StaticText::builder(&panel).with_label(labels.original_volume).build(), 0, SizerFlag::All, 6);
    root.add(&original, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
    root.add(&StaticText::builder(&panel).with_label(labels.new_volume).build(), 0, SizerFlag::All, 6);
    root.add(&added, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
    let loop_box = CheckBox::builder(&panel).with_label(labels.loop_track).build();
    loop_box.set_value(initial.as_ref().is_some_and(|v| v.loop_track));
    root.add(&loop_box, 0, SizerFlag::All, 8);
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok = Button::builder(&panel).with_label(labels.save).build();
    let cancel = Button::builder(&panel).with_label(labels.close).build();
    buttons.add(&ok, 1, SizerFlag::All, 6);
    buttons.add(&cancel, 1, SizerFlag::All, 6);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);
    dialog.set_escape_id(ID_CANCEL);

    let selected_path_choose = Rc::clone(&selected_path);
    let selected_label = selected;
    choose.on_click(move |_| {
        if let Some(path) = choose_audio_track(&dialog, &labels) {
            match probe_media(&path) {
                Ok(probe) if probe.has_audio => {
                    let name = path
                        .file_name()
                        .map(|value| value.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.to_string_lossy().into_owned());
                    selected_label.set_label(&name);
                    selected_label.show(true);
                    dialog.layout();
                    *selected_path_choose.borrow_mut() = Some(path);
                }
                _ => show_error(&dialog, labels.title, labels.invalid_media),
            }
        }
    });
    let dialog_ok = dialog;
    let selected_path_ok = Rc::clone(&selected_path);
    ok.on_click(move |_| {
        if selected_path_ok.borrow().is_none() {
            show_error(&dialog_ok, labels.title, labels.no_file);
        } else {
            dialog_ok.end_modal(ID_OK);
        }
    });
    let dialog_cancel = dialog;
    cancel.on_click(move |_| dialog_cancel.end_modal(ID_CANCEL));
    let result = dialog.show_modal();
    let out = if result == ID_OK {
        let path = selected_path.borrow().clone()?;
        let orig_idx = original.get_selection().unwrap_or(5) as usize;
        let add_idx = added.get_selection().unwrap_or(2) as usize;
        Some(AddedTrackSettings {
            path,
            original_volume: *volume_values.get(orig_idx).unwrap_or(&100),
            new_volume: *volume_values.get(add_idx).unwrap_or(&25),
            loop_track: loop_box.get_value(),
        })
    } else {
        None
    };
    dialog.destroy();
    out
}

pub fn open_dialog(parent: &Frame) {
    let labels = labels();
    let dialog = Dialog::builder(parent, labels.title)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(860, 650)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let input = Rc::new(RefCell::new(None::<PathBuf>));
    let probe = Rc::new(RefCell::new(None::<ProbeInfo>));
    let parts = Rc::new(RefCell::new(Vec::<MediaPart>::new()));
    let deleted_history = Rc::new(RefCell::new(Vec::<(f64, f64)>::new()));
    let guided_start = Rc::new(Cell::new(None::<f64>));
    let guided_end = Rc::new(Cell::new(None::<f64>));
    let edited = Rc::new(Cell::new(false));
    let preview = Rc::new(RefCell::new(PreviewState::new()));
    let show_video = Rc::new(Cell::new(false));
    let rotation = Rc::new(Cell::new(VideoRotation::None));
    let added_track = Rc::new(RefCell::new(None::<AddedTrackSettings>));

    let top_buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let open_button = Button::builder(&panel).with_label(labels.open_file).build();
    let close_button = Button::builder(&panel)
        .with_id(ID_CANCEL)
        .with_label(labels.close)
        .build();
    top_buttons.add(&open_button, 1, SizerFlag::All, 8);
    top_buttons.add(&close_button, 1, SizerFlag::All, 8);
    root.add_sizer(&top_buttons, 0, SizerFlag::Expand, 0);
    let file_status = StaticText::builder(&panel).with_label("").build();
    file_status.show(false);
    root.add(&file_status, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom, 8);

    let mode_row = BoxSizer::builder(Orientation::Horizontal).build();
    mode_row.add(&StaticText::builder(&panel).with_label(labels.mode).build(), 0, SizerFlag::AlignCenterVertical | SizerFlag::All, 5);
    let mode_choice = Choice::builder(&panel).build();
    mode_choice.append(labels.guided);
    mode_choice.append(labels.advanced);
    mode_choice.set_selection(0);
    mode_row.add(&mode_choice, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&mode_row, 0, SizerFlag::Expand, 0);

    let movement_row = BoxSizer::builder(Orientation::Horizontal).build();
    movement_row.add(&StaticText::builder(&panel).with_label(labels.movement).build(), 0, SizerFlag::AlignCenterVertical | SizerFlag::All, 5);
    let movement_choice = Choice::builder(&panel).build();
    for text in ["1 s", "5 s", "10 s", "30 s", "1 min", "2 min", "5 min", "10 min"] {
        movement_choice.append(text);
    }
    movement_choice.set_selection(1);
    movement_row.add(&movement_choice, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&movement_row, 0, SizerFlag::Expand, 0);

    let transport = BoxSizer::builder(Orientation::Horizontal).build();
    let back = Button::builder(&panel).with_label(labels.back).build();
    let play_pause = Button::builder(&panel).with_label(labels.play).build();
    let forward = Button::builder(&panel).with_label(labels.forward).build();
    transport.add(&back, 1, SizerFlag::All, 5);
    transport.add(&play_pause, 1, SizerFlag::All, 5);
    transport.add(&forward, 1, SizerFlag::All, 5);
    root.add_sizer(&transport, 0, SizerFlag::Expand, 0);
    let position_text = StaticText::builder(&panel).with_label("").build();
    position_text.show(false);
    root.add(&position_text, 0, SizerFlag::Expand | SizerFlag::All, 6);

    let guided_row = BoxSizer::builder(Orientation::Horizontal).build();
    let guided_primary = Button::builder(&panel).with_label(labels.set_start).build();
    let guided_listen = Button::builder(&panel).with_label(labels.listen_cut).build();
    let guided_modify = Button::builder(&panel).with_label(labels.modify_cut).build();
    guided_row.add(&guided_primary, 1, SizerFlag::All, 5);
    guided_row.add(&guided_listen, 1, SizerFlag::All, 5);
    guided_row.add(&guided_modify, 1, SizerFlag::All, 5);
    root.add_sizer(&guided_row, 0, SizerFlag::Expand, 0);
    guided_listen.enable(false);
    guided_modify.enable(false);

    let advanced_row = BoxSizer::builder(Orientation::Horizontal).build();
    let split_here = Button::builder(&panel).with_label(labels.split_here).build();
    let listen_part = Button::builder(&panel).with_label(labels.listen).build();
    let modify_part = Button::builder(&panel).with_label(labels.modify).build();
    let delete_part = Button::builder(&panel).with_label(labels.delete).build();
    let restore_part = Button::builder(&panel).with_label(labels.restore).build();
    advanced_row.add(&split_here, 1, SizerFlag::All, 4);
    advanced_row.add(&listen_part, 1, SizerFlag::All, 4);
    advanced_row.add(&modify_part, 1, SizerFlag::All, 4);
    advanced_row.add(&delete_part, 1, SizerFlag::All, 4);
    advanced_row.add(&restore_part, 1, SizerFlag::All, 4);
    split_here.show(false);
    listen_part.show(false);
    modify_part.show(false);
    delete_part.show(false);
    restore_part.show(false);
    root.add_sizer(&advanced_row, 0, SizerFlag::Expand, 0);
    let parts_choice = Choice::builder(&panel).build();
    parts_choice.show(false);
    root.add(&parts_choice, 0, SizerFlag::Expand | SizerFlag::All, 7);

    let secondary_row = BoxSizer::builder(Orientation::Horizontal).build();
    let rotation_choice = Choice::builder(&panel).build();
    for label in [labels.rotation_none, labels.rotation_right, labels.rotation_left, labels.rotation_half] {
        rotation_choice.append(label);
    }
    rotation_choice.set_selection(0);
    let rotation_label = StaticText::builder(&panel).with_label(labels.rotation).build();
    secondary_row.add(&rotation_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::All, 5);
    secondary_row.add(&rotation_choice, 1, SizerFlag::Expand | SizerFlag::All, 5);
    let video_preview_button = Button::builder(&panel).with_label(labels.show_video).build();
    let add_track_button = Button::builder(&panel).with_label(labels.add_track).build();
    secondary_row.add(&video_preview_button, 1, SizerFlag::All, 5);
    secondary_row.add(&add_track_button, 1, SizerFlag::All, 5);
    root.add_sizer(&secondary_row, 0, SizerFlag::Expand, 0);
    rotation_choice.enable(false);
    rotation_label.show(false);
    rotation_choice.show(false);
    video_preview_button.enable(false);
    video_preview_button.show(false);
    add_track_button.enable(false);

    let status_text = StaticText::builder(&panel).with_label(labels.ready).build();
    root.add(&status_text, 0, SizerFlag::Expand | SizerFlag::All, 8);
    let bottom = BoxSizer::builder(Orientation::Horizontal).build();
    let save_button = Button::builder(&panel).with_label(labels.save).build();
    let cancel_button = Button::builder(&panel).with_label(labels.cancel_processing).build();
    cancel_button.enable(false);
    save_button.enable(false);
    back.enable(false);
    play_pause.enable(false);
    forward.enable(false);
    guided_primary.enable(false);
    split_here.enable(false);
    bottom.add(&save_button, 1, SizerFlag::All, 6);
    bottom.add(&cancel_button, 1, SizerFlag::All, 6);
    root.add_sizer(&bottom, 0, SizerFlag::Expand, 0);

    panel.set_sizer(root, true);
    dialog.set_escape_id(ID_CANCEL);

    let focus_timer = Rc::new(Timer::new(&dialog));
    let open_focus = open_button;
    focus_timer.on_tick(move |_| open_focus.set_focus());
    focus_timer.start(80, true);

    let export_job = Rc::new(RefCell::new(None::<Arc<Mutex<ExportProgress>>>));
    let export_busy = Arc::new(AtomicBool::new(false));
    let export_cancel = Arc::new(AtomicBool::new(false));
    let close_approved = Rc::new(Cell::new(false));
    let timer = Rc::new(Timer::new(&dialog));

    // Open media.
    let input_open = Rc::clone(&input);
    let probe_open = Rc::clone(&probe);
    let parts_open = Rc::clone(&parts);
    let deleted_open = Rc::clone(&deleted_history);
    let preview_open = Rc::clone(&preview);
    let guided_start_open = Rc::clone(&guided_start);
    let guided_end_open = Rc::clone(&guided_end);
    let edited_open = Rc::clone(&edited);
    let show_video_open = Rc::clone(&show_video);
    let added_track_open = Rc::clone(&added_track);
    let rotation_open = Rc::clone(&rotation);
    let dialog_open = dialog;
    let file_status_open = file_status;
    let position_open = position_text;
    let parts_choice_open = parts_choice;
    open_button.on_click(move |_| {
        if (edited_open.get() || guided_start_open.get().is_some())
            && !ask_yes_no(&dialog_open, labels.unsaved_title, labels.unsaved_message)
        {
            return;
        }
        let Some(path) = choose_input(&dialog_open, &labels) else { return; };
        match probe_media(&path) {
            Ok(info) if info.has_audio || info.has_video => {
                preview_open.borrow_mut().stop_child();
                preview_open.borrow_mut().base_position = 0.0;
                play_pause.set_label(labels.play);
                rotation_choice.set_selection(0);
                video_preview_button.set_label(labels.show_video);
                *input_open.borrow_mut() = Some(path.clone());
                *probe_open.borrow_mut() = Some(info.clone());
                *parts_open.borrow_mut() = vec![MediaPart { start: 0.0, end: info.duration, keep: true }];
                deleted_open.borrow_mut().clear();
                guided_start_open.set(None);
                guided_end_open.set(None);
                edited_open.set(false);
                show_video_open.set(false);
                *added_track_open.borrow_mut() = None;
                rotation_open.set(VideoRotation::None);
                refresh_parts_choice(parts_choice_open, &parts_open.borrow(), &labels, Some(0));
                let name = path
                    .file_name()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                file_status_open.set_label(&format!("{name}, {}", format_duration_natural(info.duration)));
                file_status_open.show(true);
                position_open.set_label(&format!("0:00.0 / {}", format_time(info.duration)));
                position_open.show(true);
                rotation_label.show(info.has_video);
                rotation_choice.show(info.has_video);
                rotation_choice.enable(info.has_video);
                video_preview_button.show(info.has_video);
                video_preview_button.enable(info.has_video);
                panel.layout();
                dialog_open.layout();
                add_track_button.enable(true);
                save_button.enable(true);
                back.enable(true);
                play_pause.enable(true);
                forward.enable(true);
                guided_primary.enable(true);
                split_here.enable(true);
                mode_choice.set_focus();
                announce_voiceover_message(&format!("{}: {name}, {}", labels.file_loaded, format_duration_natural(info.duration)));
            }
            _ => show_error(&dialog_open, labels.title, labels.invalid_media),
        }
    });

    // Mode visibility.
    let panel_mode = panel;
    let dialog_mode = dialog;
    let guided_start_mode = Rc::clone(&guided_start);
    let guided_end_mode = Rc::clone(&guided_end);
    mode_choice.on_selection_changed(move |_| {
        let mut advanced = mode_choice.get_selection().unwrap_or(0) == 1;
        if advanced && guided_start_mode.get().is_some() {
            if !ask_yes_no(&dialog_mode, labels.unsaved_title, labels.discard_pending_cut) {
                mode_choice.set_selection(0);
                advanced = false;
            } else {
                guided_start_mode.set(None);
                guided_end_mode.set(None);
                guided_primary.set_label(labels.set_start);
                guided_listen.enable(false);
                guided_modify.enable(false);
            }
        }
        guided_primary.show(!advanced);
        guided_listen.show(!advanced);
        guided_modify.show(!advanced);
        split_here.show(advanced);
        listen_part.show(advanced);
        modify_part.show(advanced);
        delete_part.show(advanced);
        restore_part.show(advanced);
        parts_choice.show(advanced);
        panel_mode.layout();
        dialog_mode.layout();
    });

    // Transport controls.
    let input_play = Rc::clone(&input);
    let probe_play = Rc::clone(&probe);
    let preview_play = Rc::clone(&preview);
    let show_video_play = Rc::clone(&show_video);
    let dialog_play = dialog;
    play_pause.on_click(move |_| {
        let Some(path) = input_play.borrow().clone() else { show_info(&dialog_play, labels.title, labels.no_file); return; };
        let Some(info) = probe_play.borrow().clone() else { return; };
        if preview_play.borrow().playing {
            preview_play.borrow_mut().pause(info.duration);
            play_pause.set_label(labels.play);
        } else {
            let position = preview_play.borrow().position(info.duration);
            match start_preview(&preview_play, &path, info.duration, position, None, show_video_play.get()) {
                Ok(()) => play_pause.set_label(labels.pause),
                Err(err) => show_error(&dialog_play, labels.title, &err),
            }
        }
    });

    let input_back = Rc::clone(&input);
    let probe_back = Rc::clone(&probe);
    let preview_back = Rc::clone(&preview);
    let show_back = Rc::clone(&show_video);
    let d_back = dialog;
    back.on_click(move |_| {
        seek_preview(-1.0, movement_choice, &input_back, &probe_back, &preview_back, &show_back, &d_back, labels);
    });
    let input_forward = Rc::clone(&input);
    let probe_forward = Rc::clone(&probe);
    let preview_forward = Rc::clone(&preview);
    let show_forward = Rc::clone(&show_video);
    let d_forward = dialog;
    forward.on_click(move |_| {
        seek_preview(1.0, movement_choice, &input_forward, &probe_forward, &preview_forward, &show_forward, &d_forward, labels);
    });

    // Guided cut.
    let parts_guided = Rc::clone(&parts);
    let probe_guided = Rc::clone(&probe);
    let preview_guided = Rc::clone(&preview);
    let start_guided = Rc::clone(&guided_start);
    let end_guided = Rc::clone(&guided_end);
    let edited_guided = Rc::clone(&edited);
    let deleted_guided = Rc::clone(&deleted_history);
    guided_primary.on_click(move |_| {
        let Some(info) = probe_guided.borrow().clone() else { show_info(&dialog, labels.title, labels.no_file); return; };
        let current = preview_guided.borrow().position(info.duration);
        if start_guided.get().is_none() {
            start_guided.set(Some(current));
            end_guided.set(None);
            guided_primary.set_label(labels.set_end);
            guided_listen.enable(false);
            guided_modify.enable(false);
            status_text.set_label(&format!("{}: {}", labels.start_set, format_time(current)));
            announce_voiceover_message(&format!("{}: {}", labels.start_set, format_time(current)));
        } else if end_guided.get().is_none() {
            let first = start_guided.get().unwrap_or(current);
            let (start, end) = if first <= current { (first, current) } else { (current, first) };
            if end - start < MIN_PART_SECONDS {
                show_error(&dialog, labels.title, labels.invalid_cut);
                return;
            }
            start_guided.set(Some(start));
            end_guided.set(Some(end));
            guided_primary.set_label(labels.apply_cut);
            guided_listen.enable(true);
            guided_modify.enable(true);
            status_text.set_label(&format!("{}: {} – {}", labels.end_set, format_time(start), format_time(end)));
            announce_voiceover_message(&format!("{}: {} – {}", labels.end_set, format_time(start), format_time(end)));
        } else {
            let start = start_guided.get().unwrap();
            let end = end_guided.get().unwrap();
            let newly_deleted = delete_range(
                &mut parts_guided.borrow_mut(),
                start,
                end,
                info.duration,
            );
            if !newly_deleted.is_empty() {
                deleted_guided.borrow_mut().extend(newly_deleted);
                edited_guided.set(true);
                refresh_parts_choice(parts_choice, &parts_guided.borrow(), &labels, None);
                status_text.set_label(labels.cut_applied);
                announce_voiceover_message(labels.cut_applied);
            }
            start_guided.set(None);
            end_guided.set(None);
            guided_primary.set_label(labels.set_start);
            guided_listen.enable(false);
            guided_modify.enable(false);
        }
    });

    let input_guided_listen = Rc::clone(&input); let probe_guided_listen = Rc::clone(&probe); let preview_guided_listen = Rc::clone(&preview); let start_guided_listen = Rc::clone(&guided_start); let end_guided_listen = Rc::clone(&guided_end); let show_guided_listen = Rc::clone(&show_video);
    guided_listen.on_click(move |_| {
        let (Some(path), Some(info), Some(start), Some(end)) = (input_guided_listen.borrow().clone(), probe_guided_listen.borrow().clone(), start_guided_listen.get(), end_guided_listen.get()) else { return; };
        if let Err(err) = start_preview(&preview_guided_listen, &path, info.duration, start, Some(end), show_guided_listen.get()) {
            show_error(&dialog, labels.title, &err);
        }
    });

    let input_guided_modify = Rc::clone(&input); let probe_guided_modify = Rc::clone(&probe); let preview_guided_modify = Rc::clone(&preview); let start_guided_modify = Rc::clone(&guided_start); let end_guided_modify = Rc::clone(&guided_end); let show_guided_modify = Rc::clone(&show_video);
    guided_modify.on_click(move |_| {
        let (Some(path), Some(info), Some(start), Some(end)) = (input_guided_modify.borrow().clone(), probe_guided_modify.borrow().clone(), start_guided_modify.get(), end_guided_modify.get()) else { return; };
        let start_cell = Rc::new(Cell::new(start));
        let end_cell = Rc::new(Cell::new(end));
        show_cut_adjust_dialog(&dialog, labels, Rc::clone(&start_cell), Rc::clone(&end_cell), 0.0, info.duration, path, Rc::clone(&preview_guided_modify), info.duration, Rc::clone(&show_guided_modify));
        start_guided_modify.set(Some(start_cell.get()));
        end_guided_modify.set(Some(end_cell.get()));
        status_text.set_label(&format!("{}: {} – {}", labels.adjusted, format_time(start_cell.get()), format_time(end_cell.get())));
    });

    // Advanced mode actions.
    let parts_split = Rc::clone(&parts); let probe_split = Rc::clone(&probe); let preview_split = Rc::clone(&preview); let edited_split = Rc::clone(&edited);
    split_here.on_click(move |_| {
        let Some(info) = probe_split.borrow().clone() else { show_info(&dialog, labels.title, labels.no_file); return; };
        let point = preview_split.borrow().position(info.duration);
        if let Some(index) = split_part_at(&mut parts_split.borrow_mut(), point) {
            edited_split.set(true);
            refresh_parts_choice(parts_choice, &parts_split.borrow(), &labels, Some(index));
            status_text.set_label(&format!("{}: {}", labels.split_added, format_time(point)));
            announce_voiceover_message(&format!("{}: {}", labels.split_added, format_time(point)));
        } else {
            show_error(&dialog, labels.title, labels.invalid_cut);
        }
    });

    let parts_listen = Rc::clone(&parts); let input_listen = Rc::clone(&input); let probe_listen = Rc::clone(&probe); let preview_listen = Rc::clone(&preview); let show_listen = Rc::clone(&show_video);
    listen_part.on_click(move |_| {
        let index = parts_choice.get_selection().unwrap_or(0) as usize;
        let Some(part) = parts_listen.borrow().get(index).cloned() else { return; };
        let (Some(path), Some(info)) = (input_listen.borrow().clone(), probe_listen.borrow().clone()) else { return; };
        if let Err(err) = start_preview(&preview_listen, &path, info.duration, part.start, Some(part.end), show_listen.get()) {
            show_error(&dialog, labels.title, &err);
        }
    });

    let parts_delete = Rc::clone(&parts); let deleted_delete = Rc::clone(&deleted_history); let edited_delete = Rc::clone(&edited);
    delete_part.on_click(move |_| {
        let index = parts_choice.get_selection().unwrap_or(0) as usize;
        let mut parts = parts_delete.borrow_mut();
        let Some(part) = parts.get_mut(index) else { return; };
        if part.keep {
            part.keep = false;
            deleted_delete.borrow_mut().push((part.start, part.end));
            edited_delete.set(true);
            refresh_parts_choice(parts_choice, &parts, &labels, Some(index));
            status_text.set_label(labels.part_deleted);
            announce_voiceover_message(labels.part_deleted);
        }
    });

    let parts_restore = Rc::clone(&parts); let deleted_restore = Rc::clone(&deleted_history); let edited_restore = Rc::clone(&edited);
    restore_part.on_click(move |_| {
        let mut history = deleted_restore.borrow_mut();
        while let Some((start, end)) = history.pop() {
            let mut parts = parts_restore.borrow_mut();
            if let Some((index, part)) = parts
                .iter_mut()
                .enumerate()
                .find(|(_, part)| !part.keep && (part.start - start).abs() < 0.001 && (part.end - end).abs() < 0.001)
            {
                part.keep = true;
                edited_restore.set(true);
                refresh_parts_choice(parts_choice, &parts, &labels, Some(index));
                status_text.set_label(labels.part_restored);
                announce_voiceover_message(labels.part_restored);
                return;
            }
        }
        show_info(&dialog, labels.title, labels.no_deleted_part);
    });

    let parts_modify = Rc::clone(&parts); let probe_modify = Rc::clone(&probe); let input_modify = Rc::clone(&input); let preview_modify = Rc::clone(&preview); let show_modify = Rc::clone(&show_video); let edited_modify = Rc::clone(&edited); let deleted_modify = Rc::clone(&deleted_history);
    modify_part.on_click(move |_| {
        let index = parts_choice.get_selection().unwrap_or(0) as usize;
        let Some(current) = parts_modify.borrow().get(index).cloned() else { return; };
        let (Some(info), Some(path)) = (probe_modify.borrow().clone(), input_modify.borrow().clone()) else { return; };
        let min_start = if index == 0 { 0.0 } else { parts_modify.borrow()[index - 1].start + MIN_PART_SECONDS };
        let max_end = if index + 1 >= parts_modify.borrow().len() { info.duration } else { parts_modify.borrow()[index + 1].end - MIN_PART_SECONDS };
        let start_cell = Rc::new(Cell::new(current.start));
        let end_cell = Rc::new(Cell::new(current.end));
        show_cut_adjust_dialog(&dialog, labels, Rc::clone(&start_cell), Rc::clone(&end_cell), min_start, max_end, path, Rc::clone(&preview_modify), info.duration, Rc::clone(&show_modify));
        let new_start = start_cell.get();
        let new_end = end_cell.get();
        let mut p = parts_modify.borrow_mut();
        let start_delta = new_start - current.start;
        let end_delta = new_end - current.end;
        let mut changed = false;
        if start_delta.abs() > 0.0001 {
            changed |= adjust_part_boundary(&mut p, index, true, start_delta, info.duration);
        }
        if end_delta.abs() > 0.0001 {
            changed |= adjust_part_boundary(&mut p, index, false, end_delta, info.duration);
        }
        if changed {
            if !current.keep {
                if let Some(updated) = p.get(index) {
                    if let Some(entry) = deleted_modify
                        .borrow_mut()
                        .iter_mut()
                        .rev()
                        .find(|(start, end)| {
                            (*start - current.start).abs() < 0.001
                                && (*end - current.end).abs() < 0.001
                        })
                    {
                        *entry = (updated.start, updated.end);
                    }
                }
            }
            edited_modify.set(true);
            refresh_parts_choice(parts_choice, &p, &labels, Some(index));
            status_text.set_label(labels.adjusted);
        }
    });

    // Video rotation and preview.
    let rotation_select = Rc::clone(&rotation);
    let edited_rotation = Rc::clone(&edited);
    rotation_choice.on_selection_changed(move |_| {
        rotation_select.set(match rotation_choice.get_selection().unwrap_or(0) {
            1 => VideoRotation::Right,
            2 => VideoRotation::Left,
            3 => VideoRotation::Half,
            _ => VideoRotation::None,
        });
        edited_rotation.set(true);
    });

    let show_video_toggle = Rc::clone(&show_video); let preview_toggle = Rc::clone(&preview); let input_toggle = Rc::clone(&input); let probe_toggle = Rc::clone(&probe);
    video_preview_button.on_click(move |_| {
        let new_value = !show_video_toggle.get();
        show_video_toggle.set(new_value);
        video_preview_button.set_label(if new_value { labels.hide_video } else { labels.show_video });
        if let (Some(path), Some(info)) = (input_toggle.borrow().clone(), probe_toggle.borrow().clone()) {
            let was_playing = preview_toggle.borrow().playing;
            let position = preview_toggle.borrow().position(info.duration);
            if new_value || was_playing {
                let _ = start_preview(&preview_toggle, &path, info.duration, position, None, new_value);
            }
        }
    });

    // Added audio track.
    let added_track_button_state = Rc::clone(&added_track);
    let edited_track = Rc::clone(&edited);
    let input_track = Rc::clone(&input);
    add_track_button.on_click(move |_| {
        if input_track.borrow().is_none() {
            show_info(&dialog, labels.title, labels.no_file);
            return;
        }
        if let Some(settings) = show_add_track_dialog(&dialog, added_track_button_state.borrow().clone(), labels) {
            *added_track_button_state.borrow_mut() = Some(settings);
            edited_track.set(true);
            status_text.set_label(labels.track_added);
            announce_voiceover_message(labels.track_added);
        }
    });

    // Timer: playback and export progress.
    let timer_tick = Rc::clone(&timer);
    let preview_tick = Rc::clone(&preview);
    let probe_tick = Rc::clone(&probe);
    let export_job_tick = Rc::clone(&export_job);
    let export_busy_tick = Arc::clone(&export_busy);
    let edited_tick = Rc::clone(&edited);
    let guided_start_tick = Rc::clone(&guided_start);
    let guided_end_tick = Rc::clone(&guided_end);
    let dialog_tick = dialog;
    timer_tick.on_tick(move |_| {
        if let Some(info) = probe_tick.borrow().clone() {
            let mut state = preview_tick.borrow_mut();
            let position = state.position(info.duration);
            if let Some(stop_at) = state.stop_at {
                if position >= stop_at - 0.02 {
                    state.base_position = stop_at.min(info.duration);
                    state.stop_child();
                    play_pause.set_label(labels.play);
                }
            }
            if let Some(child) = state.child.as_mut() {
                if child.try_wait().ok().flatten().is_some() {
                    state.base_position = position.min(info.duration);
                    state.child = None;
                    state.playing = false;
                    state.started_at = None;
                    play_pause.set_label(labels.play);
                }
            }
            play_pause.set_label(if state.playing { labels.pause } else { labels.play });
            position_text.set_label(&format!("{} / {}", format_time(state.position(info.duration)), format_time(info.duration)));
        }
        let state = export_job_tick.borrow().as_ref().cloned();
        let Some(state) = state else { return; };
        let snapshot = {
            let p = state.lock().unwrap();
            (p.percent, p.message.clone(), p.finished, p.result.clone())
        };
        if !snapshot.2 {
            status_text.set_label(&format!("{} {}%", snapshot.1, snapshot.0.clamp(0, 99)));
            return;
        }
        *export_job_tick.borrow_mut() = None;
        export_busy_tick.store(false, Ordering::SeqCst);
        open_button.enable(true);
        mode_choice.enable(true);
        movement_choice.enable(true);
        back.enable(true);
        play_pause.enable(true);
        forward.enable(true);
        guided_primary.enable(true);
        let pending_guided_ready = guided_start_tick.get().is_some() && guided_end_tick.get().is_some();
        guided_listen.enable(pending_guided_ready);
        guided_modify.enable(pending_guided_ready);
        split_here.enable(true);
        listen_part.enable(true);
        modify_part.enable(true);
        delete_part.enable(true);
        restore_part.enable(true);
        parts_choice.enable(true);
        rotation_choice.enable(probe_tick.borrow().as_ref().is_some_and(|p| p.has_video));
        video_preview_button.enable(probe_tick.borrow().as_ref().is_some_and(|p| p.has_video));
        add_track_button.enable(true);
        save_button.enable(true);
        cancel_button.enable(false);
        close_button.enable(true);
        match snapshot.3.unwrap_or_else(|| Err("errore sconosciuto".to_string())) {
            Ok(path) => {
                edited_tick.set(false);
                let message = format!("{}: {}", labels.saved, path.display());
                status_text.set_label(&message);
                announce_voiceover_message(&message);
                show_info(&dialog_tick, labels.title, &message);
            }
            Err(err) if err == "__CANCELLED__" => {
                status_text.set_label(labels.processing_cancelled);
                announce_voiceover_message(labels.processing_cancelled);
            }
            Err(err) => {
                status_text.set_label(labels.ready);
                show_error(&dialog_tick, labels.save_failed, &err);
            }
        }
    });
    timer.start(150, false);

    // Export.
    let input_save = Rc::clone(&input); let probe_save = Rc::clone(&probe); let parts_save = Rc::clone(&parts); let rotation_save = Rc::clone(&rotation); let added_save = Rc::clone(&added_track); let export_job_save = Rc::clone(&export_job); let export_busy_save = Arc::clone(&export_busy); let export_cancel_save = Arc::clone(&export_cancel); let preview_save = Rc::clone(&preview); let guided_start_save = Rc::clone(&guided_start);
    save_button.on_click(move |_| {
        if export_busy_save.load(Ordering::SeqCst) { return; }
        if guided_start_save.get().is_some() {
            show_info(&dialog, labels.title, labels.pending_cut_save);
            return;
        }
        let (Some(input_path), Some(probe_info)) = (input_save.borrow().clone(), probe_save.borrow().clone()) else { show_info(&dialog, labels.title, labels.no_file); return; };
        if !parts_save.borrow().iter().any(|p| p.keep && p.duration() >= MIN_PART_SECONDS) {
            show_error(&dialog, labels.title, labels.nothing_to_save);
            return;
        }
        let Some(output) = choose_output(&dialog, &input_path, probe_info.has_video, &labels) else { return; };
        if output == input_path {
            show_error(&dialog, labels.title, labels.same_output);
            return;
        }
        preview_save.borrow_mut().pause(probe_info.duration);
        play_pause.set_label(labels.play);
        let state = Arc::new(Mutex::new(ExportProgress { percent: 0, message: labels.processing.to_string(), finished: false, result: None }));
        *export_job_save.borrow_mut() = Some(Arc::clone(&state));
        export_cancel_save.store(false, Ordering::SeqCst);
        export_busy_save.store(true, Ordering::SeqCst);
        open_button.enable(false);
        mode_choice.enable(false);
        movement_choice.enable(false);
        back.enable(false);
        play_pause.enable(false);
        forward.enable(false);
        guided_primary.enable(false);
        guided_listen.enable(false);
        guided_modify.enable(false);
        split_here.enable(false);
        listen_part.enable(false);
        modify_part.enable(false);
        delete_part.enable(false);
        restore_part.enable(false);
        parts_choice.enable(false);
        rotation_choice.enable(false);
        video_preview_button.enable(false);
        add_track_button.enable(false);
        save_button.enable(false);
        cancel_button.enable(true);
        close_button.enable(false);
        status_text.set_label(&format!("{} 0%", labels.processing));
        announce_voiceover_message(labels.processing_started);
        let snapshot = ExportSnapshot { input: input_path, output, parts: parts_save.borrow().clone(), probe: probe_info, rotation: rotation_save.get(), added_track: added_save.borrow().clone() };
        let cancel = Arc::clone(&export_cancel_save);
        std::thread::spawn(move || export_media(snapshot, state, cancel));
    });

    let export_cancel_click = Arc::clone(&export_cancel);
    cancel_button.on_click(move |_| {
        export_cancel_click.store(true, Ordering::SeqCst);
        cancel_button.enable(false);
        status_text.set_label(labels.processing_cancelled);
    });

    let export_busy_close = Arc::clone(&export_busy);
    let edited_close = Rc::clone(&edited);
    let guided_start_close = Rc::clone(&guided_start);
    let close_approved_event = Rc::clone(&close_approved);
    let preview_close = Rc::clone(&preview);
    dialog.on_close(move |event| {
        if export_busy_close.load(Ordering::SeqCst) {
            event.skip(false);
            return;
        }
        if close_approved_event.get() {
            preview_close.borrow_mut().stop_child();
            event.skip(true);
            return;
        }
        let pending = edited_close.get() || guided_start_close.get().is_some();
        if pending && !ask_yes_no(&dialog, labels.unsaved_title, labels.unsaved_message) {
            event.skip(false);
            return;
        }
        preview_close.borrow_mut().stop_child();
        event.skip(true);
    });

    let export_busy_button = Arc::clone(&export_busy);
    let edited_button = Rc::clone(&edited);
    let guided_start_button = Rc::clone(&guided_start);
    let close_approved_button = Rc::clone(&close_approved);
    let preview_button = Rc::clone(&preview);
    let dialog_close = dialog;
    close_button.on_click(move |_| {
        if export_busy_button.load(Ordering::SeqCst) {
            return;
        }
        let pending = edited_button.get() || guided_start_button.get().is_some();
        if pending && !ask_yes_no(&dialog_close, labels.unsaved_title, labels.unsaved_message) {
            return;
        }
        preview_button.borrow_mut().stop_child();
        close_approved_button.set(true);
        dialog_close.end_modal(ID_CANCEL);
    });

    dialog.show_modal();
    focus_timer.stop();
    timer.stop();
    preview.borrow_mut().stop_child();
    dialog.destroy();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guided_delete_only_marks_requested_range() {
        let mut parts = vec![MediaPart { start: 0.0, end: 100.0, keep: true }];
        let deleted = delete_range(&mut parts, 20.0, 30.0, 100.0);
        assert_eq!(deleted, vec![(20.0, 30.0)]);
        assert_eq!(parts.len(), 3);
        assert!(parts[0].keep);
        assert!(!parts[1].keep);
        assert!(parts[2].keep);
        assert!((parts[1].start - 20.0).abs() < 0.001);
        assert!((parts[1].end - 30.0).abs() < 0.001);
    }

    #[test]
    fn advanced_split_preserves_keep_state() {
        let mut parts = vec![MediaPart { start: 0.0, end: 10.0, keep: true }];
        assert_eq!(split_part_at(&mut parts, 5.0), Some(1));
        assert_eq!(parts.len(), 2);
        assert!(parts[0].keep && parts[1].keep);
    }

    #[test]
    fn rotation_filters_match_expected_ffmpeg_transforms() {
        assert_eq!(rotation_filter(VideoRotation::None), None);
        assert_eq!(rotation_filter(VideoRotation::Right), Some("transpose=1"));
        assert_eq!(rotation_filter(VideoRotation::Left), Some("transpose=2"));
        assert_eq!(rotation_filter(VideoRotation::Half), Some("hflip,vflip"));
    }
}
