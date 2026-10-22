use crate::{append_podcast_log, ffmpeg_executable_path};
#[path = "media_cutter_accessibility.rs"]
mod accessibility;
use accessibility::announce_voiceover_message;
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

#[path = "media_cutter_keyboard.rs"]
mod keyboard;
use keyboard::{CutterAction, cutter_shortcut, cutter_key_from_code};

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
    playback_ranges: Vec<MediaPart>,
}

impl PreviewState {
    fn new() -> Self {
        Self {
            child: None,
            playing: false,
            base_position: 0.0,
            started_at: None,
            playback_ranges: Vec::new(),
        }
    }

    fn position(&self, duration: f64) -> f64 {
        let mut position = self.base_position;
        if self.playing {
            if let Some(started_at) = self.started_at {
                if !self.playback_ranges.is_empty() {
                    return kept_position(&self.playback_ranges, started_at.elapsed().as_secs_f64());
                }
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
        self.playback_ranges.clear();
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
    split_here: &'static str,
    part_singular: &'static str,
    listen: &'static str,
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
    choose_destination: &'static str,
    cancel_processing: &'static str,
    close: &'static str,
    ready: &'static str,
    no_file: &'static str,
    invalid_media: &'static str,
    file_loaded: &'static str,
    guide_start: &'static str,
    guide_end: &'static str,
    guide_review: &'static str,
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
    command_area: &'static str,
    keys_common: &'static str,
    keys_guided: &'static str,
    keys_advanced: &'static str,
    close_export: &'static str,
    output_suffix: &'static str,
    parts_label: &'static str,
    remove_track: &'static str,
    track_removed: &'static str,
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
        back: t(l, "Indietro (Option+←)", "Back (Option+←)", "Reculer (Option+←)", "Atrás (Option+←)", "Recuar (Option+←)", "Zpět (Option+←)", "Wstecz (Option+←)"),
        play: t(l, "Riproduci (Option+Spazio)", "Play (Option+Space)", "Lire (Option+Espace)", "Reproducir (Option+Espacio)", "Reproduzir (Option+Espaço)", "Přehrát (Option+Mezerník)", "Odtwórz (Option+Spacja)"),
        pause: t(l, "Pausa (Option+Spazio)", "Pause (Option+Space)", "Pause (Option+Espace)", "Pausa (Option+Espacio)", "Pausa (Option+Espaço)", "Pauza (Option+Mezerník)", "Pauza (Option+Spacja)"),
        forward: t(l, "Avanti (Option+→)", "Forward (Option+→)", "Avancer (Option+→)", "Adelante (Option+→)", "Avançar (Option+→)", "Vpřed (Option+→)", "Dalej (Option+→)"),
        set_start: t(l, "Inizio taglio (Option+1)", "Set cut start (Option+1)", "Début de coupe (Option+1)", "Inicio del corte (Option+1)", "Início do corte (Option+1)", "Začátek střihu (Option+1)", "Początek cięcia (Option+1)"),
        set_end: t(l, "Fine taglio (Option+2)", "Set cut end (Option+2)", "Fin de coupe (Option+2)", "Fin del corte (Option+2)", "Fim do corte (Option+2)", "Konec střihu (Option+2)", "Koniec cięcia (Option+2)"),
        apply_cut: t(l, "Applica taglio (Option+4)", "Apply cut (Option+4)", "Appliquer la coupe (Option+4)", "Aplicar corte (Option+4)", "Aplicar corte (Option+4)", "Použít střih (Option+4)", "Zastosuj cięcie (Option+4)"),
        listen_cut: t(l, "Ascolta taglio (Option+3)", "Listen to cut (Option+3)", "Écouter la coupe (Option+3)", "Escuchar corte (Option+3)", "Ouvir corte (Option+3)", "Poslechnout střih (Option+3)", "Odsłuchaj cięcie (Option+3)"),
        split_here: t(l, "Dividi qui (Option+S)", "Split here (Option+S)", "Diviser ici (Option+S)", "Dividir aquí (Option+S)", "Dividir aqui (Option+S)", "Rozdělit zde (Option+S)", "Podziel tutaj (Option+S)"),
        part_singular: t(l, "Parte", "Part", "Partie", "Parte", "Parte", "Část", "Część"),
        listen: t(l, "Ascolta (Option+3)", "Listen (Option+3)", "Écouter (Option+3)", "Escuchar (Option+3)", "Ouvir (Option+3)", "Poslechnout (Option+3)", "Odsłuchaj (Option+3)"),
        delete: t(l, "Elimina (Option+D)", "Delete (Option+D)", "Supprimer (Option+D)", "Eliminar (Option+D)", "Eliminar (Option+D)", "Odstranit (Option+D)", "Usuń (Option+D)"),
        restore: t(l, "Ripristina parte eliminata (Option+R)", "Restore deleted part (Option+R)", "Restaurer la partie supprimée (Option+R)", "Restaurar parte eliminada (Option+R)", "Restaurar parte eliminada (Option+R)", "Obnovit odstraněnou část (Option+R)", "Przywróć usuniętą część (Option+R)"),
        rotation: t(l, "Rotazione video", "Video rotation", "Rotation vidéo", "Rotación de vídeo", "Rotação do vídeo", "Otočení videa", "Obrót wideo"),
        rotation_none: t(l, "Nessuna", "None", "Aucune", "Ninguna", "Nenhuma", "Žádné", "Brak"),
        rotation_right: t(l, "Destra", "Right", "Droite", "Derecha", "Direita", "Doprava", "W prawo"),
        rotation_left: t(l, "Sinistra", "Left", "Gauche", "Izquierda", "Esquerda", "Doleva", "W lewo"),
        rotation_half: "180°",
        show_video: t(l, "Mostra anteprima video", "Show video preview", "Afficher l’aperçu vidéo", "Mostrar vista previa de vídeo", "Mostrar pré-visualização do vídeo", "Zobrazit náhled videa", "Pokaż podgląd wideo"),
        hide_video: t(l, "Nascondi anteprima video", "Hide video preview", "Masquer l’aperçu vidéo", "Ocultar vista previa de vídeo", "Ocultar pré-visualização do vídeo", "Skrýt náhled videa", "Ukryj podgląd wideo"),
        add_track: t(l, "Aggiungi nuova traccia…", "Add new track…", "Ajouter une nouvelle piste…", "Añadir nueva pista…", "Adicionar nova faixa…", "Přidat novou stopu…", "Dodaj nową ścieżkę…"),
        choose_destination: t(l, "Cambia cartella di destinazione...", "Change destination folder...", "Changer le dossier de destination...", "Cambiar carpeta de destino...", "Alterar pasta de destino...", "Změnit cílovou složku...", "Zmień folder docelowy..."),
        save: t(l, "Salva…", "Save…", "Enregistrer…", "Guardar…", "Guardar…", "Uložit…", "Zapisz…"),
        cancel_processing: t(l, "Interrompi", "Stop", "Interrompre", "Interrumpir", "Interromper", "Přerušit", "Przerwij"),
        close: t(l, "Chiudi", "Close", "Fermer", "Cerrar", "Fechar", "Zavřít", "Zamknij"),
        ready: t(l, "Pronto", "Ready", "Prêt", "Listo", "Pronto", "Připraveno", "Gotowe"),
        no_file: t(l, "Apri prima un file media.", "Open a media file first.", "Ouvrez d’abord un fichier multimédia.", "Abre primero un archivo multimedia.", "Abra primeiro um ficheiro multimédia.", "Nejprve otevřete multimediální soubor.", "Najpierw otwórz plik multimedialny."),
        invalid_media: t(l, "Il file non contiene audio o video utilizzabile.", "The file contains no usable audio or video.", "Le fichier ne contient aucun audio ou vidéo utilisable.", "El archivo no contiene audio o vídeo utilizable.", "O ficheiro não contém áudio ou vídeo utilizável.", "Soubor neobsahuje použitelný zvuk ani video.", "Plik nie zawiera użytecznego audio ani wideo."),
        file_loaded: t(l, "File caricato", "File loaded", "Fichier chargé", "Archivo cargado", "Ficheiro carregado", "Soubor načten", "Plik wczytany"),
        guide_start: t(l, "Vai all'inizio della parte da eliminare, poi premi Inizio taglio.", "Go to the beginning of the part to remove, then press Set cut start.", "Allez au début de la partie à supprimer, puis appuyez sur Début de coupe.", "Ve al inicio de la parte que quieres eliminar y pulsa Inicio del corte.", "Vá ao início da parte a eliminar e prima Início do corte.", "Přejděte na začátek části k odstranění a stiskněte Začátek střihu.", "Przejdź do początku fragmentu do usunięcia i naciśnij Początek cięcia."),
        guide_end: t(l, "Ora vai alla fine della parte da eliminare, poi premi Fine taglio.", "Now go to the end of the part to remove, then press Set cut end.", "Allez maintenant à la fin de la partie à supprimer, puis appuyez sur Fin de coupe.", "Ahora ve al final de la parte que quieres eliminar y pulsa Fin del corte.", "Agora vá ao fim da parte a eliminar e prima Fim do corte.", "Nyní přejděte na konec části k odstranění a stiskněte Konec střihu.", "Teraz przejdź do końca fragmentu do usunięcia i naciśnij Koniec cięcia."),
        guide_review: t(l, "Usa Ascolta taglio per sentire la parte che verrà eliminata, oppure Applica taglio per eliminarla.", "Use Listen to cut to hear the part that will be removed, or Apply cut to remove it.", "Utilisez Écouter la coupe pour écouter la partie à supprimer, ou Appliquer la coupe pour la supprimer.", "Usa Escuchar corte para oír la parte que se eliminará, o Aplicar corte para eliminarla.", "Use Ouvir corte para ouvir a parte a eliminar, ou Aplicar corte para a eliminar.", "Použijte Poslechnout střih k poslechu odstraňované části nebo Použít střih k jejímu odstranění.", "Użyj Odsłuchaj cięcie, aby usłyszeć usuwany fragment, lub Zastosuj cięcie, aby go usunąć."),
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
        precision: t(l, "Regolazione spostamento taglio", "Cut boundary adjustment step", "Pas de déplacement de la coupe", "Paso de ajuste del corte", "Passo de ajuste do corte", "Krok posunu hranic střihu", "Krok przesunięcia granic cięcia"),
        start_back: t(l, "Sposta inizio taglio indietro", "Move cut start backward", "Reculer le début de coupe", "Mover inicio del corte atrás", "Recuar início do corte", "Posunout začátek střihu zpět", "Przesuń początek cięcia wstecz"),
        start_forward: t(l, "Sposta inizio taglio avanti", "Move cut start forward", "Avancer le début de coupe", "Mover inicio del corte adelante", "Avançar início do corte", "Posunout začátek střihu vpřed", "Przesuń początek cięcia dalej"),
        end_back: t(l, "Sposta fine taglio indietro", "Move cut end backward", "Reculer la fin de coupe", "Mover fin del corte atrás", "Recuar fim do corte", "Posunout konec střihu zpět", "Przesuń koniec cięcia wstecz"),
        end_forward: t(l, "Sposta fine taglio avanti", "Move cut end forward", "Avancer la fin de coupe", "Mover fin del corte adelante", "Avançar fim do corte", "Posunout konec střihu vpřed", "Przesuń koniec cięcia dalej"),
        adjusted: t(l, "Taglio aggiornato", "Cut updated", "Coupe mise à jour", "Corte actualizado", "Corte atualizado", "Střih upraven", "Cięcie zaktualizowane"),
        choose_track: t(l, "Aggiungi nuova traccia…", "Add new track…", "Ajouter une nouvelle piste…", "Añadir nueva pista…", "Adicionar nova faixa…", "Přidat novou stopu…", "Dodaj nową ścieżkę…"),
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
        command_area: t(l, "Area comandi: usa i comandi di taglio senza Option o Command. F1: elenco tasti. Tab: esci dall’area.", "Command area: use cut commands without Option or Command. F1: key list. Tab: leave the area.", "Zone de commandes : utilisez les commandes de découpe sans Option ni Command. F1 : liste des touches. Tab : quitter la zone.", "Área de comandos: usa los comandos de corte sin Option ni Command. F1: lista de teclas. Tab: salir del área.", "Área de comandos: use os comandos de corte sem Option ou Command. F1: lista de teclas. Tab: sair da área.", "Oblast příkazů: používejte příkazy střihu bez Option nebo Command. F1: seznam kláves. Tab: opustit oblast.", "Obszar poleceń: używaj poleceń cięcia bez Option lub Command. F1: lista klawiszy. Tab: wyjdź z obszaru."),
        keys_common: t(l, "Spazio: riproduci/pausa. Frecce sinistra/destra: spostamento scelto. Shift+frecce: 5 secondi. Command+frecce: 10 secondi. 3 o Invio: ascolta la parte selezionata. Command+S: salva. Tab: esci dall’area. Home: inizio del file. End: fine del file.", "Space: play/pause. Left/right arrows: selected seek step. Shift+arrows: 5 seconds. Command+arrows: 10 seconds. 3 or Return: listen to the selected part. Command+S: save. Tab: leave the area. Home: beginning of the file. End: end of the file.", "Espace : lecture/pause. Flèches gauche/droite : déplacement choisi. Shift+flèches : 5 secondes. Command+flèches : 10 secondes. 3 ou Entrée : écouter la partie sélectionnée. Command+S : enregistrer. Tab : quitter la zone. Home : début du fichier. End : fin du fichier.", "Espacio: reproducir/pausa. Flechas izquierda/derecha: desplazamiento elegido. Shift+flechas: 5 segundos. Command+flechas: 10 segundos. 3 o Intro: escuchar la parte seleccionada. Command+S: guardar. Tab: salir del área. Home: inicio del archivo. End: final del archivo.", "Espaço: reproduzir/pausa. Setas esquerda/direita: deslocamento escolhido. Shift+setas: 5 segundos. Command+setas: 10 segundos. 3 ou Enter: ouvir a parte selecionada. Command+S: guardar. Tab: sair da área. Home: início do ficheiro. End: fim do ficheiro.", "Mezerník: přehrát/pozastavit. Šipky vlevo/vpravo: zvolený posun. Shift+šipky: 5 sekund. Command+šipky: 10 sekund. 3 nebo Enter: poslech vybrané části. Command+S: uložit. Tab: opustit oblast. Home: začátek souboru. End: konec souboru.", "Spacja: odtwarzanie/pauza. Strzałki w lewo/prawo: wybrany krok. Shift+strzałki: 5 sekund. Command+strzałki: 10 sekund. 3 lub Enter: odsłuchaj wybraną część. Command+S: zapisz. Tab: wyjdź z obszaru. Home: początek pliku. End: koniec pliku."),
        keys_guided: t(l, "1: inizio taglio. 2: fine taglio. 4: applica taglio.", "1: cut start. 2: cut end. 4: apply cut.", "1 : début de coupe. 2 : fin de coupe. 4 : appliquer la coupe.", "1: inicio del corte. 2: fin del corte. 4: aplicar corte.", "1: início do corte. 2: fim do corte. 4: aplicar corte.", "1: začátek střihu. 2: konec střihu. 4: použít střih.", "1: początek cięcia. 2: koniec cięcia. 4: zastosuj cięcie."),
        keys_advanced: t(l, "S: dividi qui. D o Canc (⌫/⌦): elimina parte. R: ripristina parte eliminata. Frecce su/giù: parte precedente/successiva.", "S: split here. D or Delete (⌫/⌦): delete part. R: restore deleted part. Up/down arrows: previous/next part.", "S : diviser ici. D ou Suppr (⌫/⌦) : supprimer la partie. R : restaurer la partie supprimée. Flèches haut/bas : partie précédente/suivante.", "S: dividir aquí. D o Supr (⌫/⌦): eliminar parte. R: restaurar parte eliminada. Flechas arriba/abajo: parte anterior/siguiente.", "S: dividir aqui. D ou Delete (⌫/⌦): eliminar parte. R: restaurar parte eliminada. Setas cima/baixo: parte anterior/seguinte.", "S: rozdělit zde. D nebo Delete (⌫/⌦): odstranit část. R: obnovit odstraněnou část. Šipky nahoru/dolů: předchozí/další část.", "S: podziel tutaj. D lub Delete (⌫/⌦): usuń część. R: przywróć usuniętą część. Strzałki w górę/dół: poprzednia/następna część."),
        close_export: t(l, "È in corso un’esportazione. Vuoi interromperla e chiudere? Il lavoro non salvato andrà perso.", "An export is in progress. Stop it and close? Unsaved work will be lost.", "Une exportation est en cours. L’arrêter et fermer ? Les modifications non enregistrées seront perdues.", "Hay una exportación en curso. ¿Interrumpirla y cerrar? Se perderá el trabajo sin guardar.", "Está em curso uma exportação. Interromper e fechar? O trabalho não guardado será perdido.", "Probíhá export. Přerušit jej a zavřít? Neuložené změny budou ztraceny.", "Trwa eksport. Przerwać go i zamknąć? Niezapisana praca zostanie utracona."),
        output_suffix: t(l, "tagliato", "trimmed", "découpé", "recortado", "cortado", "oříznutý", "przycięty"),
        parts_label: t(l, "Parti (Option+↑ / Option+↓)", "Parts (Option+↑ / Option+↓)", "Parties (Option+↑ / Option+↓)", "Partes (Option+↑ / Option+↓)", "Partes (Option+↑ / Option+↓)", "Části (Option+↑ / Option+↓)", "Części (Option+↑ / Option+↓)"),
        remove_track: t(l, "Rimuovi traccia aggiunta", "Remove added track", "Retirer la piste ajoutée", "Quitar pista añadida", "Remover faixa adicionada", "Odebrat přidanou stopu", "Usuń dodaną ścieżkę"),
        track_removed: t(l, "Traccia aggiunta rimossa", "Added track removed", "Piste ajoutée retirée", "Pista añadida eliminada", "Faixa adicionada removida", "Přidaná stopa odebrána", "Dodana ścieżka usunięta"),
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

thread_local! {
    static CUTTER_LANGUAGE: RefCell<String> = RefCell::new(String::from("it"));
}

fn format_duration_natural(seconds: f64) -> String {
    let lang = CUTTER_LANGUAGE.with(|lang| lang.borrow().clone());
    let units = match lang.as_str() {
        "en" => ["hour", "hours", "minute", "minutes", "second", "seconds"],
        "fr" => ["heure", "heures", "minute", "minutes", "seconde", "secondes"],
        "es" => ["hora", "horas", "minuto", "minutos", "segundo", "segundos"],
        "pt" => ["hora", "horas", "minuto", "minutos", "segundo", "segundos"],
        "cs" => ["hodina", "hodin", "minuta", "minut", "sekunda", "sekund"],
        "pl" => ["godzina", "godzin", "minuta", "minut", "sekunda", "sekund"],
        _ => ["ora", "ore", "minuto", "minuti", "secondo", "secondi"],
    };
    let total = seconds.max(0.0).floor() as u64;
    let mut words = Vec::new();
    for (i, n) in [total / 3600, total % 3600 / 60, total % 60].into_iter().enumerate() {
        if n > 0 || (i == 2 && words.is_empty()) {
            let unit = if (lang == "cs" && (2..=4).contains(&n)) || (lang == "pl" && (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100))) {
                if lang == "cs" { ["hodiny", "minuty", "sekundy"][i] } else { ["godziny", "minuty", "sekundy"][i] }
            } else { units[i * 2 + usize::from(n != 1)] };
            words.push(format!("{n} {unit}"));
        }
    }
    words.join(", ")
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

// Both normal playback and auditions use retained ranges.
fn remaining_kept_ranges(parts: &[MediaPart], position: f64) -> Vec<MediaPart> {
    parts.iter().filter(|p| p.keep && p.duration() >= MIN_PART_SECONDS && p.end > position)
        .map(|p| MediaPart { start: p.start.max(position), end: p.end, keep: true })
        .collect()
}

fn kept_position(parts: &[MediaPart], elapsed: f64) -> f64 {
    let mut remaining = elapsed.max(0.0);
    let mut end = 0.0;
    for part in parts.iter().filter(|p| p.keep && p.duration() > 0.0) {
        end = part.end;
        if remaining < part.duration() { return part.start + remaining; }
        remaining -= part.duration();
    }
    end
}

fn kept_seek_position(parts: &[MediaPart], position: f64, delta: f64) -> f64 {
    let offset: f64 = parts.iter().filter(|p| p.keep && p.duration() >= MIN_PART_SECONDS)
        .map(|p| (position - p.start).clamp(0.0, p.duration())).sum();
    kept_position(parts, offset + delta)
}

fn playback_edl(path: &Path, ranges: &[MediaPart]) -> String {
    let name = path.to_string_lossy();
    let segments: Vec<String> = ranges.iter().map(|p|
        format!("%{}%{},{:.6},{:.6}", name.len(), name, p.start, p.duration())
    ).collect();
    format!("edl://{}", segments.join(";"))
}

fn start_playback(preview: &Rc<RefCell<PreviewState>>, path: &Path, duration: f64,
    position: f64, parts: &[MediaPart], show_video: bool) -> Result<(), String> {
    let ranges = remaining_kept_ranges(parts, position);
    let mut state = preview.borrow_mut();
    state.stop_child();
    if ranges.is_empty() {
        state.base_position = duration;
        return Ok(());
    }
    // MPV clips and joins the ranges itself, so no deleted audio leaks between
    // UI timer ticks. Positions exposed to the editor remain source positions.
    let edl = playback_edl(path, &ranges);
    let child = spawn_preview(Path::new(&edl), 0.0, None, show_video)?;
    state.child = Some(child);
    state.playing = true;
    state.base_position = ranges[0].start;
    state.started_at = Some(Instant::now());
    state.playback_ranges = ranges;
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
    parts: &Rc<RefCell<Vec<MediaPart>>>,
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
    let new_position = kept_seek_position(&parts.borrow(), preview.borrow().position(info.duration), direction * step);
    preview.borrow_mut().pause(info.duration);
    preview.borrow_mut().base_position = new_position;
    if was_playing {
        let _ = start_playback(
            preview,
            &path,
            info.duration,
            new_position,
            &parts.borrow(),
            show_video.get(),
        );
    }
    announce_voiceover_message(&format_duration_natural(new_position));
}

fn cut_precision_seconds(choice: Choice) -> f64 {
    match choice.get_selection().unwrap_or(0) {
        1 => 0.5,
        2 => 0.25,
        3 => 0.10,
        _ => 1.0,
    }
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

fn restore_part_index(parts: &[MediaPart], selected: usize, history: &[(f64, f64)]) -> Option<usize> {
    if parts.get(selected).is_some_and(|p| !p.keep) { return Some(selected); }
    history.iter().rev().find_map(|(start,end)| parts.iter().position(|p|
        !p.keep && (p.start-start).abs() < 0.001 && (p.end-end).abs() < 0.001))
        .or_else(|| parts.iter().rposition(|p| !p.keep))
}

fn audition_ranges(parts: &[MediaPart], start: f64, end: f64) -> Vec<MediaPart> {
    parts.iter().filter(|p| p.keep).filter_map(|p| {
        let start = start.max(p.start);
        let end = end.min(p.end);
        (end - start >= MIN_PART_SECONDS).then_some(MediaPart { start, end, keep: true })
    }).collect()
}

fn part_label(labels: &Labels, index: usize, part: &MediaPart) -> String {
    if part.keep {
        format!("{} {}, {} – {}", labels.part_singular, index + 1, format_duration_natural(part.start), format_duration_natural(part.end))
    } else {
        format!("{} {}, {}, {} – {}", labels.part_singular, index + 1, labels.part_deleted_marker, format_duration_natural(part.start), format_duration_natural(part.end))
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
    // The shared dialog supplies app-localized labels (wxDragon issue #156).
    crate::ask_yes_no_dialog(parent, title, message)
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
    let default_name = format!("{stem}_{}.{ext}", labels.output_suffix);
    let formats: &[&str] = if has_video { &["mp4", "mkv", "avi", "mov", "m4a", "mp3", "wav", "flac", "aiff"] } else { &["m4a", "mp3", "wav", "flac", "aiff"] };
    let wildcard = formats.iter().map(|ext| format!("{}|*.{ext}", ext.to_uppercase())).collect::<Vec<_>>().join("|");
    let dialog = FileDialog::builder(parent)
        .with_message(labels.choose_destination)
        .with_wildcard(&wildcard)
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
    let selected = formats.get(dialog.get_filter_index().max(0) as usize).copied().unwrap_or(ext);
    path.set_extension(selected);
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
            let start = (index * 70 / kept.len()) as i32;
            let segment_span = ((index + 1) * 70 / kept.len()) as i32 - start;
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
            run_ffmpeg_step(&args, 71, 7, total_duration, &progress, &cancel)?;
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
            run_ffmpeg_step(&args, 79, 12, total_duration, &progress, &cancel)?;
        } else {
            copy_file(&assembled, &final_temp)?;
            progress.lock().unwrap().percent = 91;
        }
        if cancel.load(Ordering::SeqCst) {
            return Err("__CANCELLED__".to_string());
        }
        let output_ext = snapshot.output.extension().and_then(|v| v.to_str()).unwrap_or(ext).to_ascii_lowercase();
        let wants_video = matches!(output_ext.as_str(), "mp4" | "mkv" | "avi" | "mov");
        if wants_video && !snapshot.probe.has_video { return Err("video source required".into()); }
        let final_temp = if output_ext == ext {
            final_temp
        } else {
            let converted = work_dir.join(format!("converted.{output_ext}"));
            let mut args = vec!["-y".into(), "-i".into(), final_temp.to_string_lossy().into_owned()];
            let options: &[&str] = match output_ext.as_str() {
                "mp3" => &["-vn", "-c:a", "libmp3lame", "-b:a", "192k"],
                "wav" => &["-vn", "-c:a", "pcm_s16le"],
                "aiff" => &["-vn", "-c:a", "pcm_s16be"],
                "flac" => &["-vn", "-c:a", "flac"],
                "m4a" => &["-vn", "-c:a", "copy"],
                "mkv" | "mov" => &["-c", "copy"],
                "avi" => &["-c:v", "mpeg4", "-q:v", "4", "-bf", "0", "-c:a", "libmp3lame", "-b:a", "192k"],
                _ => return Err(format!("unsupported output format: {output_ext}")),
            };
            args.extend(options.iter().map(|v| v.to_string()));
            args.push(converted.to_string_lossy().into_owned());
            run_ffmpeg_step(&args, 92, 7, total_duration, &progress, &cancel)?;
            converted
        };
        let final_probe = probe_media(&final_temp)?;
        let duration_tolerance = (total_duration * 0.02).max(2.0);
        if (final_probe.duration - total_duration).abs() > duration_tolerance {
            return Err(format!(
                "durata finale inattesa: {:.2}s invece di {:.2}s",
                final_probe.duration, total_duration
            ));
        }
        if wants_video && !final_probe.has_video {
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
    let original_label = StaticText::builder(&panel).with_label(labels.original_volume).build();
    root.add(&original_label, 0, SizerFlag::All, 6);
    root.add(&original, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
    let added_label = StaticText::builder(&panel).with_label(labels.new_volume).build();
    root.add(&added_label, 0, SizerFlag::All, 6);
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

    original_label.show(initial.is_some());
    added_label.show(initial.is_some());
    original.show(initial.is_some());
    added.show(initial.is_some());
    loop_box.show(initial.is_some());
    ok.enable(initial.is_some());
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
                    original_label.show(true); added_label.show(true);
                    original.show(true); added.show(true); loop_box.show(true); ok.enable(true);
                    panel.layout();
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
    CUTTER_LANGUAGE.with(|lang| *lang.borrow_mut() = crate::normalize_ui_language(&crate::Settings::load().ui_language));
    let labels = labels();
    let dialog = Dialog::builder(parent, labels.title)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(860, 650)
        .build();
    let panel = Panel::builder(&dialog).build();
    let progress_panel = Panel::builder(&dialog).build();
    progress_panel.show(false);
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
    top_buttons.add(&open_button, 1, SizerFlag::All, 8);
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
    guided_row.add(&guided_primary, 1, SizerFlag::All, 5);
    guided_row.add(&guided_listen, 1, SizerFlag::All, 5);
    root.add_sizer(&guided_row, 0, SizerFlag::Expand, 0);
    guided_listen.enable(false);

    let advanced_row = BoxSizer::builder(Orientation::Horizontal).build();
    let split_here = Button::builder(&panel).with_label(labels.split_here).build();
    let listen_part = Button::builder(&panel).with_label(labels.listen).build();
    let delete_part = Button::builder(&panel).with_label(labels.delete).build();
    let restore_part = Button::builder(&panel).with_label(labels.restore).build();
    advanced_row.add(&split_here, 1, SizerFlag::All, 4);
    advanced_row.add(&listen_part, 1, SizerFlag::All, 4);
    advanced_row.add(&delete_part, 1, SizerFlag::All, 4);
    advanced_row.add(&restore_part, 1, SizerFlag::All, 4);
    split_here.show(false);
    listen_part.show(false);
    delete_part.show(false);
    restore_part.show(false);
    root.add_sizer(&advanced_row, 0, SizerFlag::Expand, 0);
    let parts_label = StaticText::builder(&panel).with_label(labels.parts_label).build();
    parts_label.show(false);
    root.add(&parts_label, 0, SizerFlag::All, 5);
    let parts_choice = Choice::builder(&panel).build();
    parts_choice.set_accessibility_label(labels.parts_label);
    parts_choice.show(false);
    root.add(&parts_choice, 0, SizerFlag::Expand | SizerFlag::All, 7);

    let adjustment_panel = Panel::builder(&panel).build();
    let adjustment_root = BoxSizer::builder(Orientation::Vertical).build();
    let boundary_summary = StaticText::builder(&adjustment_panel).with_label("").build();
    adjustment_root.add(&boundary_summary, 0, SizerFlag::Expand | SizerFlag::All, 5);
    let precision_label = StaticText::builder(&adjustment_panel).with_label(labels.precision).build();
    let precision = Choice::builder(&adjustment_panel).build();
    precision.set_accessibility_label(labels.precision);
    for value in ["1 s", "0.5 s", "0.25 s", "0.10 s"] { precision.append(value); }
    precision.set_selection(0);
    adjustment_root.add(&precision_label, 0, SizerFlag::All, 5);
    adjustment_root.add(&precision, 0, SizerFlag::Expand | SizerFlag::All, 5);
    let boundary_buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let start_back = Button::builder(&adjustment_panel).with_label(labels.start_back).build();
    let start_forward = Button::builder(&adjustment_panel).with_label(labels.start_forward).build();
    let end_back = Button::builder(&adjustment_panel).with_label(labels.end_back).build();
    let end_forward = Button::builder(&adjustment_panel).with_label(labels.end_forward).build();
    for button in [start_back, start_forward, end_back, end_forward] { boundary_buttons.add(&button, 1, SizerFlag::All, 4); }
    adjustment_root.add_sizer(&boundary_buttons, 0, SizerFlag::Expand, 0);
    adjustment_panel.set_sizer(adjustment_root, true);
    adjustment_panel.show(false);
    root.add(&adjustment_panel, 0, SizerFlag::Expand, 0);

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
    let remove_track = Button::builder(&panel).with_label(labels.remove_track).build();
    remove_track.show(false);
    secondary_row.add(&remove_track, 1, SizerFlag::All, 5);
    root.add_sizer(&secondary_row, 0, SizerFlag::Expand, 0);
    rotation_choice.enable(false);
    rotation_label.show(false);
    rotation_choice.show(false);
    video_preview_button.enable(false);
    video_preview_button.show(false);
    add_track_button.enable(false);

    // A read-only text field has no native button activation/default action.
    // Do not use ProcessTab: Tab and Shift+Tab must leave this command area.
    let command_area = TextCtrl::builder(&panel)
        .with_value(labels.command_area)
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly | TextCtrlStyle::ProcessEnter)
        .with_size(Size::new(780, 65))
        .build();
    command_area.set_accessibility_label(labels.command_area.split(':').next().unwrap_or(labels.title));
    root.add(&command_area, 0, SizerFlag::Expand | SizerFlag::All, 8);
    let status_text = StaticText::builder(&panel).with_label(labels.ready).build();
    root.add(&status_text, 0, SizerFlag::Expand | SizerFlag::All, 8);
    let bottom = BoxSizer::builder(Orientation::Horizontal).build();
    let save_button = Button::builder(&panel).with_label(&format!("{} (Command+S)", labels.save)).build();
    save_button.enable(false);
    back.enable(false);
    play_pause.enable(false);
    forward.enable(false);
    guided_primary.enable(false);
    split_here.enable(false);
    bottom.add(&save_button, 1, SizerFlag::All, 6);
    root.add_sizer(&bottom, 0, SizerFlag::Expand, 0);

    panel.set_sizer(root, true);
    let progress_root = BoxSizer::builder(Orientation::Vertical).build();
    let progress_gauge = Gauge::builder(&progress_panel).with_range(100).build();
    progress_gauge.set_accessibility_label(labels.processing);
    let progress_text = StaticText::builder(&progress_panel).with_label(&format!("{}: 0%", labels.processing)).build();
    progress_root.add(&progress_text, 0, SizerFlag::Expand | SizerFlag::All, 12);
    let last_progress = Rc::new(Cell::new(-1));
    let cancel_button = Button::builder(&progress_panel).with_label(labels.cancel_processing).build();
    cancel_button.enable(false);
    progress_root.add(&progress_gauge, 0, SizerFlag::Expand | SizerFlag::All, 12);
    progress_root.add(&cancel_button, 0, SizerFlag::All, 12);
    progress_panel.set_sizer(progress_root, true);
    let dialog_root = BoxSizer::builder(Orientation::Vertical).build();
    dialog_root.add(&panel, 1, SizerFlag::Expand, 0);
    dialog_root.add(&progress_panel, 1, SizerFlag::Expand, 0);
    dialog.set_sizer(dialog_root, true);
    // Route Escape through the same unsaved/export guard as the native close button.
    dialog.set_escape_id(wxdragon::id::ID_NONE);

    let export_job = Rc::new(RefCell::new(None::<Arc<Mutex<ExportProgress>>>));
    let export_busy = Arc::new(AtomicBool::new(false));
    let export_cancel = Arc::new(AtomicBool::new(false));
    let close_after_export = Rc::new(Cell::new(false));
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
                guided_primary.set_label(labels.set_start);
                guided_listen.enable(false);
                guided_start_open.set(None);
                guided_end_open.set(None);
                edited_open.set(false);
                show_video_open.set(false);
                *added_track_open.borrow_mut() = None;
                remove_track.show(false);
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
                let instruction = if mode_choice.get_selection().unwrap_or(0) == 0 { labels.guide_start } else { labels.ready };
                status_text.set_label(labels.ready);
                status_text.wrap(780);
                panel.layout();
                announce_voiceover_message(&format!("{}: {name}, {}. {}", labels.file_loaded, format_duration_natural(info.duration), instruction));
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
            }
        }
        guided_primary.show(!advanced);
        guided_listen.show(!advanced);
        split_here.show(advanced);
        listen_part.show(advanced);
        delete_part.show(advanced);
        restore_part.show(advanced);
        parts_choice.show(advanced);
        parts_label.show(advanced);
        if !advanced {
            let instruction = if guided_end_mode.get().is_some() { labels.guide_review }
                else if guided_start_mode.get().is_some() { labels.guide_end }
                else { labels.guide_start };
            announce_voiceover_message(instruction);
        }
        panel_mode.layout();
        dialog_mode.layout();
    });

    // Transport controls.
    let input_play = Rc::clone(&input);
    let probe_play = Rc::clone(&probe);
    let preview_play = Rc::clone(&preview);
    let show_video_play = Rc::clone(&show_video);
    let parts_play = Rc::clone(&parts);
    let dialog_play = dialog;
    let play_pause_action: Rc<dyn Fn()> = Rc::new(move || {
        let Some(path) = input_play.borrow().clone() else { show_info(&dialog_play, labels.title, labels.no_file); return; };
        let Some(info) = probe_play.borrow().clone() else { return; };
        if preview_play.borrow().playing {
            preview_play.borrow_mut().pause(info.duration);
            play_pause.set_label(labels.play);
        } else {
            let position = preview_play.borrow().position(info.duration);
            match start_playback(&preview_play, &path, info.duration, position, &parts_play.borrow(), show_video_play.get()) {
                Ok(()) => play_pause.set_label(if preview_play.borrow().playing { labels.pause } else { labels.play }),
                Err(err) => show_error(&dialog_play, labels.title, &err),
            }
        }
    });
    let click_action = Rc::clone(&play_pause_action);
    play_pause.on_click(move |_| click_action());

    let input_back = Rc::clone(&input);
    let probe_back = Rc::clone(&probe);
    let preview_back = Rc::clone(&preview);
    let show_back = Rc::clone(&show_video);
    let parts_back = Rc::clone(&parts);
    let d_back = dialog;
    back.on_click(move |_| {
        seek_preview(-1.0, movement_choice, &input_back, &probe_back, &preview_back, &show_back, &parts_back, &d_back, labels);
    });
    let input_forward = Rc::clone(&input);
    let probe_forward = Rc::clone(&probe);
    let preview_forward = Rc::clone(&preview);
    let show_forward = Rc::clone(&show_video);
    let parts_forward = Rc::clone(&parts);
    let d_forward = dialog;
    forward.on_click(move |_| {
        seek_preview(1.0, movement_choice, &input_forward, &probe_forward, &preview_forward, &show_forward, &parts_forward, &d_forward, labels);
    });

    // Guided cut.
    let parts_guided = Rc::clone(&parts);
    let probe_guided = Rc::clone(&probe);
    let preview_guided = Rc::clone(&preview);
    let start_guided = Rc::clone(&guided_start);
    let end_guided = Rc::clone(&guided_end);
    let edited_guided = Rc::clone(&edited);
    let deleted_guided = Rc::clone(&deleted_history);
    let guided_primary_action: Rc<dyn Fn()> = Rc::new(move || {
        let Some(info) = probe_guided.borrow().clone() else { show_info(&dialog, labels.title, labels.no_file); return; };
        let current = preview_guided.borrow().position(info.duration);
        if start_guided.get().is_none() {
            start_guided.set(Some(current));
            end_guided.set(None);
            guided_primary.set_label(labels.set_end);
            guided_listen.enable(false);
            let message = format!("{}: {}. {}", labels.start_set, format_duration_natural(current), labels.guide_end);
            status_text.set_label(&format!("{}: {}", labels.start_set, format_duration_natural(current)));
            announce_voiceover_message(&message);
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
            preview_guided.borrow_mut().pause(info.duration);
            play_pause.set_label(labels.play);
            let message = format!("{}: {} – {}. {}", labels.end_set, format_duration_natural(start), format_duration_natural(end), labels.guide_review);
            status_text.set_label(&format!("{}: {} – {}", labels.end_set, format_time(start), format_time(end)));
            announce_voiceover_message(&message);
        } else {
            let start = start_guided.get().unwrap();
            let end = end_guided.get().unwrap();
            preview_guided.borrow_mut().pause(info.duration);
            preview_guided.borrow_mut().base_position = start;
            play_pause.set_label(labels.play);
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
                let message = format!("{}. {}", labels.cut_applied, labels.guide_start);
                status_text.set_label(labels.cut_applied);
                announce_voiceover_message(&message);
            }
            start_guided.set(None);
            end_guided.set(None);
            guided_primary.set_label(labels.set_start);
            guided_listen.enable(false);
        }
    });
    let click_action = Rc::clone(&guided_primary_action);
    guided_primary.on_click(move |_| click_action());

    let input_guided_listen = Rc::clone(&input); let probe_guided_listen = Rc::clone(&probe); let preview_guided_listen = Rc::clone(&preview); let start_guided_listen = Rc::clone(&guided_start); let end_guided_listen = Rc::clone(&guided_end); let show_guided_listen = Rc::clone(&show_video);
    let parts_guided_listen = Rc::clone(&parts);
    let guided_listen_action: Rc<dyn Fn()> = Rc::new(move || {
        let (Some(path), Some(info), Some(start), Some(end)) = (input_guided_listen.borrow().clone(), probe_guided_listen.borrow().clone(), start_guided_listen.get(), end_guided_listen.get()) else { return; };
        let ranges = audition_ranges(&parts_guided_listen.borrow(), start, end);
        if ranges.is_empty() { announce_voiceover_message(labels.part_deleted); return; }
        if let Err(err) = start_playback(&preview_guided_listen, &path, info.duration, start, &ranges, show_guided_listen.get()) {
            show_error(&dialog, labels.title, &err);
        }
    });
    let click_action = Rc::clone(&guided_listen_action);
    guided_listen.on_click(move |_| click_action());

    // Advanced mode actions.
    let parts_split = Rc::clone(&parts); let probe_split = Rc::clone(&probe); let preview_split = Rc::clone(&preview); let edited_split = Rc::clone(&edited);
    let split_here_action: Rc<dyn Fn()> = Rc::new(move || {
        let Some(info) = probe_split.borrow().clone() else { show_info(&dialog, labels.title, labels.no_file); return; };
        let point = preview_split.borrow().position(info.duration);
        let split_index = split_part_at(&mut parts_split.borrow_mut(), point);
        if let Some(index) = split_index {
            edited_split.set(true);
            refresh_parts_choice(parts_choice, &parts_split.borrow(), &labels, Some(index));
            status_text.set_label(&format!("{}: {}", labels.split_added, format_duration_natural(point)));
            announce_voiceover_message(&format!("{}: {}", labels.split_added, format_duration_natural(point)));
        } else {
            show_error(&dialog, labels.title, labels.invalid_cut);
        }
    });
    let click_action = Rc::clone(&split_here_action);
    split_here.on_click(move |_| click_action());

    let parts_listen = Rc::clone(&parts); let input_listen = Rc::clone(&input); let probe_listen = Rc::clone(&probe); let preview_listen = Rc::clone(&preview); let show_listen = Rc::clone(&show_video);
    let listen_part_action: Rc<dyn Fn()> = Rc::new(move || {
        let index = parts_choice.get_selection().unwrap_or(0) as usize;
        let Some(part) = parts_listen.borrow().get(index).cloned() else { return; };
        append_podcast_log(&format!("media_cutter.listen index={index} keep={}", part.keep));
        if !part.keep {
            announce_voiceover_message(labels.part_deleted);
            return;
        }
        let (Some(path), Some(info)) = (input_listen.borrow().clone(), probe_listen.borrow().clone()) else { return; };
        let ranges = audition_ranges(&parts_listen.borrow(), part.start, part.end);
        if let Err(err) = start_playback(&preview_listen, &path, info.duration, part.start, &ranges, show_listen.get()) {
            show_error(&dialog, labels.title, &err);
        }
    });
    let click_action = Rc::clone(&listen_part_action);
    listen_part.on_click(move |_| click_action());

    let parts_delete = Rc::clone(&parts); let deleted_delete = Rc::clone(&deleted_history); let edited_delete = Rc::clone(&edited);
    let preview_delete = Rc::clone(&preview); let probe_delete = Rc::clone(&probe);
    let delete_part_action: Rc<dyn Fn()> = Rc::new(move || {
        let index = parts_choice.get_selection().unwrap_or(0) as usize;
        let mut parts = parts_delete.borrow_mut();
        let Some(part) = parts.get_mut(index) else { return; };
        if part.keep {
            if let Some(info) = probe_delete.borrow().as_ref() { preview_delete.borrow_mut().pause(info.duration); }
            play_pause.set_label(labels.play);
            append_podcast_log(&format!("media_cutter.delete index={index} start={} end={}", part.start, part.end));
            part.keep = false;
            deleted_delete.borrow_mut().push((part.start, part.end));
            edited_delete.set(true);
            refresh_parts_choice(parts_choice, &parts, &labels, Some(index));
            status_text.set_label(labels.part_deleted);
            announce_voiceover_message(labels.part_deleted);
        }
    });
    let click_action = Rc::clone(&delete_part_action);
    delete_part.on_click(move |_| click_action());

    let parts_restore = Rc::clone(&parts); let deleted_restore = Rc::clone(&deleted_history); let edited_restore = Rc::clone(&edited);
    let preview_restore = Rc::clone(&preview); let probe_restore = Rc::clone(&probe);
    let restore_part_action: Rc<dyn Fn()> = Rc::new(move || {
        let selected = parts_choice.get_selection().unwrap_or(0) as usize;
        let mut parts = parts_restore.borrow_mut();
        let index = restore_part_index(&parts, selected, &deleted_restore.borrow());
        let Some(index) = index else { show_info(&dialog, labels.title, labels.no_deleted_part); return; };
        if let Some(info) = probe_restore.borrow().as_ref() { preview_restore.borrow_mut().pause(info.duration); }
        let part = &mut parts[index];
        append_podcast_log(&format!("media_cutter.restore index={index} start={} end={}", part.start, part.end));
        part.keep = true;
        deleted_restore.borrow_mut().retain(|(start,end)| (*start-part.start).abs() > 0.001 || (*end-part.end).abs() > 0.001);
        edited_restore.set(true);
        refresh_parts_choice(parts_choice, &parts, &labels, Some(index));
        status_text.set_label(labels.part_restored);
        announce_voiceover_message(&format!("{}. {}", labels.part_restored, part_label(&labels, index, &parts[index])));
    });
    let click_action = Rc::clone(&restore_part_action);
    restore_part.on_click(move |_| click_action());

    for (button, move_start, direction) in [(start_back, true, -1.0), (start_forward, true, 1.0), (end_back, false, -1.0), (end_forward, false, 1.0)] {
        let parts = Rc::clone(&parts);
        let probe = Rc::clone(&probe);
        let preview = Rc::clone(&preview);
        let edited = Rc::clone(&edited);
        let start = Rc::clone(&guided_start);
        let end = Rc::clone(&guided_end);
        button.on_click(move |_| {
            let Some(info) = probe.borrow().clone() else { return; };
            let delta = direction * cut_precision_seconds(precision);
            preview.borrow_mut().pause(info.duration);
            let bounds = if mode_choice.get_selection().unwrap_or(0) == 1 {
                let index = parts_choice.get_selection().unwrap_or(0) as usize;
                let mut parts = parts.borrow_mut();
                if !parts.get(index).is_some_and(|p| p.keep) { announce_voiceover_message(labels.part_deleted); return; }
                if adjust_part_boundary(&mut parts, index, move_start, delta, info.duration) {
                    edited.set(true);
                    refresh_parts_choice(parts_choice, &parts, &labels, Some(index));
                }
                (parts[index].start, parts[index].end)
            } else {
                let (Some(a), Some(b)) = (start.get(), end.get()) else { return; };
                let (a,b) = if move_start { ((a+delta).clamp(0.0,b-MIN_PART_SECONDS),b) } else { (a,(b+delta).clamp(a+MIN_PART_SECONDS,info.duration)) };
                start.set(Some(a)); end.set(Some(b));
                (a,b)
            };
            let message = format!("{}: {} – {}", labels.adjusted, format_duration_natural(bounds.0), format_duration_natural(bounds.1));
            boundary_summary.set_label(&message);
            announce_voiceover_message(&message);
        });
    }

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
    let parts_toggle = Rc::clone(&parts);
    video_preview_button.on_click(move |_| {
        let new_value = !show_video_toggle.get();
        show_video_toggle.set(new_value);
        video_preview_button.set_label(if new_value { labels.hide_video } else { labels.show_video });
        if let (Some(path), Some(info)) = (input_toggle.borrow().clone(), probe_toggle.borrow().clone()) {
            let was_playing = preview_toggle.borrow().playing;
            let position = preview_toggle.borrow().position(info.duration);
            if new_value || was_playing {
                let ranges = preview_toggle.borrow().playback_ranges.clone();
                if !ranges.is_empty() {
                    let _ = start_playback(&preview_toggle, &path, info.duration, position, &ranges, new_value);
                } else {
                    let _ = start_playback(&preview_toggle, &path, info.duration, position, &parts_toggle.borrow(), new_value);
                }
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
        let initial = added_track_button_state.borrow().clone();
        if let Some(settings) = show_add_track_dialog(&dialog, initial, labels) {
            *added_track_button_state.borrow_mut() = Some(settings);
            edited_track.set(true);
            remove_track.show(true);
            panel.layout(); dialog.layout();
            status_text.set_label(labels.track_added);
            announce_voiceover_message(labels.track_added);
        }
    });

    let track_remove = Rc::clone(&added_track);
    let edited_remove = Rc::clone(&edited);
    remove_track.on_click(move |_| {
        *track_remove.borrow_mut() = None;
        edited_remove.set(true);
        remove_track.show(false);
        panel.layout(); dialog.layout();
        add_track_button.set_focus();
        announce_voiceover_message(labels.track_removed);
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
    // wxdragon 0.9.18 binds timer callbacks to the owner, not the timer ID.
    // Use one timer and focus only on its first tick; a second focus timer
    // would share the playback events and could pull VoiceOver back repeatedly.
    let parts_tick = Rc::clone(&parts);
    let adjustment_visible = Cell::new(false);
    let close_after_export_tick = Rc::clone(&close_after_export);
    let last_progress_tick = Rc::clone(&last_progress);
    let initial_focus_pending = Cell::new(true);
    timer_tick.on_tick(move |_| {
        if initial_focus_pending.replace(false) {
            open_button.set_focus();
            append_podcast_log("media_cutter.initial_focus");
        }
        if let Some(info) = probe_tick.borrow().clone() {
            let mut state = preview_tick.borrow_mut();
            let position = state.position(info.duration);
            if let Some(child) = state.child.as_mut() {
                if child.try_wait().ok().flatten().is_some() {
                    state.base_position = position.min(info.duration);
                    state.child = None;
                    state.playing = false;
                    state.started_at = None;
                    play_pause.set_label(labels.play);
                }
            }
            let label = if state.playing { labels.pause } else { labels.play };
            if play_pause.get_label() != label { play_pause.set_label(label); }
            position_text.set_label(&format!("{} / {}", format_time(state.position(info.duration)), format_time(info.duration)));
        }
        let bounds = if mode_choice.get_selection().unwrap_or(0) == 1 {
            parts_tick.borrow().get(parts_choice.get_selection().unwrap_or(0) as usize)
                .filter(|p| p.keep).map(|p| (p.start, p.end))
        } else { guided_start_tick.get().zip(guided_end_tick.get()) };
        if adjustment_visible.replace(bounds.is_some()) != bounds.is_some() {
            if bounds.is_none() && [start_back.has_focus(), start_forward.has_focus(), end_back.has_focus(), end_forward.has_focus(), precision.has_focus()].into_iter().any(|v| v) { command_area.set_focus(); }
            adjustment_panel.show(bounds.is_some());
            panel.layout(); dialog_tick.layout();
        }
        if let Some((a,b)) = bounds {
            let text = format!("{} – {}", format_time(a), format_time(b));
            if boundary_summary.get_label() != text { boundary_summary.set_label(&text); }
        }
        let state = export_job_tick.borrow().as_ref().cloned();
        let Some(state) = state else { return; };
        let snapshot = {
            let p = state.lock().unwrap();
            (p.percent, p.message.clone(), p.finished, p.result.clone())
        };
        if !snapshot.2 {
            let percent = snapshot.0.clamp(0, 99);
            if last_progress_tick.replace(percent) != percent {
                progress_gauge.set_value(percent);
                let label = format!("{}: {percent}%", labels.processing);
                progress_text.set_label(&label);
                progress_gauge.set_accessibility_label(&label);
                progress_panel.layout();
            }
            return;
        }
        *export_job_tick.borrow_mut() = None;
        export_busy_tick.store(false, Ordering::SeqCst);
        if close_after_export_tick.get() {
            preview_tick.borrow_mut().stop_child();
            dialog_tick.end_modal(ID_CANCEL);
            return;
        }
        progress_gauge.set_value(if snapshot.3.as_ref().is_some_and(|r| r.is_ok()) { 100 } else { snapshot.0.clamp(0, 99) });
        progress_panel.show(false);
        panel.show(true);
        dialog_tick.layout();
        open_button.enable(true);
        mode_choice.enable(true);
        movement_choice.enable(true);
        back.enable(true);
        play_pause.enable(true);
        forward.enable(true);
        guided_primary.enable(true);
        let pending_guided_ready = guided_start_tick.get().is_some() && guided_end_tick.get().is_some();
        guided_listen.enable(pending_guided_ready);
        split_here.enable(true);
        listen_part.enable(true);
        delete_part.enable(true);
        restore_part.enable(true);
        parts_choice.enable(true);
        rotation_choice.enable(probe_tick.borrow().as_ref().is_some_and(|p| p.has_video));
        video_preview_button.enable(probe_tick.borrow().as_ref().is_some_and(|p| p.has_video));
        add_track_button.enable(true);
        save_button.enable(true);
        cancel_button.enable(false);
        save_button.set_focus();
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
    let save_button_action: Rc<dyn Fn()> = Rc::new(move || {
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
        split_here.enable(false);
        listen_part.enable(false);
        delete_part.enable(false);
        restore_part.enable(false);
        parts_choice.enable(false);
        rotation_choice.enable(false);
        video_preview_button.enable(false);
        add_track_button.enable(false);
        save_button.enable(false);
        cancel_button.enable(true);
        progress_gauge.set_value(0);
        last_progress.set(-1);
        let label = format!("{}: 0%", labels.processing);
        progress_text.set_label(&label);
        progress_gauge.set_accessibility_label(&label);
        panel.show(false);
        progress_panel.show(true);
        dialog.layout();
        progress_gauge.set_focus();
        announce_voiceover_message(labels.processing_started);
        let snapshot = ExportSnapshot { input: input_path, output, parts: parts_save.borrow().clone(), probe: probe_info, rotation: rotation_save.get(), added_track: added_save.borrow().clone() };
        let cancel = Arc::clone(&export_cancel_save);
        std::thread::spawn(move || export_media(snapshot, state, cancel));
    });
    let click_action = Rc::clone(&save_button_action);
    save_button.on_click(move |_| click_action());

    // Every control uses the same actions as its button. Bare keys are confined
    // to the command area, so native Choice navigation and Tab keep working.
    let parts_selected = Rc::clone(&parts);
    parts_choice.on_selection_changed(move |_| {
        let index = parts_choice.get_selection().unwrap_or(0) as usize;
        if let Some(part) = parts_selected.borrow().get(index) {
            announce_voiceover_message(&part_label(&labels, index, part));
        }
    });
    let input_keys = Rc::clone(&input);
    let probe_keys = Rc::clone(&probe);
    let preview_keys = Rc::clone(&preview);
    let parts_keys = Rc::clone(&parts);
    let show_keys = Rc::clone(&show_video);
    let start_keys = Rc::clone(&guided_start);
    let end_keys = Rc::clone(&guided_end);
    let busy_keys = Arc::clone(&export_busy);
    let held_key = Rc::new(Cell::new(None::<i32>));
    let held_down = Rc::clone(&held_key);
    let key_handler: Rc<dyn Fn(WindowEventData)> = Rc::new(move |event| {
        let WindowEventData::Keyboard(key) = &event else { event.skip(true); return; };
        let code = key.get_key_code().unwrap_or_default();
        let key_kind = cutter_key_from_code(code);
        let advanced = mode_choice.get_selection().unwrap_or(0) == 1;
        #[cfg(target_os = "macos")]
        let control = accessibility::physical_control_down();
        #[cfg(not(target_os = "macos"))]
        let control = key.control_down();
        if !control && !key.alt_down() && (code == 27 || (key.cmd_down() && code == 87)) {
            event.skip(false);
            dialog.close(false);
            held_down.set(None);
            return;
        }
        let action = cutter_shortcut(key_kind, command_area.has_focus(), key.alt_down(),
            key.cmd_down(), control, key.shift_down(), advanced);
        let Some(action) = action else { event.skip(true); return; };
        append_podcast_log(&format!("media_cutter.key code={code} area={} option={} command={} control={control} shift={} advanced={advanced} action={action:?}",
            command_area.has_focus(), key.alt_down(), key.cmd_down(), key.shift_down()));
        event.skip(false);
        if busy_keys.load(Ordering::SeqCst) { return; }
        if held_down.replace(Some(code)) == Some(code)
            && !matches!(action, CutterAction::Seek(..) | CutterAction::Select(..)) { return; }
        match action {
            CutterAction::Help => {
                let help = format!("{}\n{}", labels.keys_common,
                    if advanced { labels.keys_advanced } else { labels.keys_guided });
                show_info(&dialog, labels.title, &help);
                command_area.set_focus();
                held_down.set(None);
            }
            CutterAction::Save => { save_button_action(); held_down.set(None); }
            _ if input_keys.borrow().is_none() => announce_voiceover_message(labels.no_file),
            CutterAction::Play => play_pause_action(),
            CutterAction::Seek(direction, seconds) => {
                let ratio = seconds.map(|n| n as f64 / movement_seconds(movement_choice)).unwrap_or(1.0);
                seek_preview(direction as f64 * ratio, movement_choice, &input_keys, &probe_keys,
                    &preview_keys, &show_keys, &parts_keys, &dialog, labels);
            }
            CutterAction::FileBoundary(end) => {
                let (Some(info), Some(path)) = (probe_keys.borrow().clone(), input_keys.borrow().clone()) else { return; };
                let running = preview_keys.borrow().playing;
                let position = if end { info.duration } else { 0.0 };
                preview_keys.borrow_mut().pause(info.duration);
                preview_keys.borrow_mut().base_position = position;
                if running && !end {
                    if let Err(err) = start_playback(&preview_keys, &path, info.duration, position, &parts_keys.borrow(), show_keys.get()) {
                        show_error(&dialog, labels.title, &err);
                    }
                }
                announce_voiceover_message(&format_duration_natural(position));
            }
            CutterAction::Select(direction) => {
                let parts = parts_keys.borrow();
                if !parts.is_empty() {
                    let current = parts_choice.get_selection().unwrap_or(0) as i32;
                    let index = (current + direction).clamp(0, parts.len() as i32 - 1) as usize;
                    append_podcast_log(&format!("media_cutter.select index={index} keep={}", parts[index].keep));
                    parts_choice.set_selection(index as u32);
                    announce_voiceover_message(&part_label(&labels, index, &parts[index]));
                }
            }
            CutterAction::Start if start_keys.get().is_none() => guided_primary_action(),
            CutterAction::End if start_keys.get().is_some() && end_keys.get().is_none() => guided_primary_action(),
            CutterAction::Apply if start_keys.get().is_some() && end_keys.get().is_some() => guided_primary_action(),
            CutterAction::Listen => if advanced { listen_part_action(); } else { guided_listen_action(); },
            CutterAction::Split => split_here_action(),
            CutterAction::Delete => delete_part_action(),
            CutterAction::Restore => restore_part_action(),
            _ => {}
        }
    });
    macro_rules! bind_cutter_keys {
        ($($control:expr),+ $(,)?) => { $(
            let handler = Rc::clone(&key_handler);
            $control.on_key_down(move |event| handler(event));
            let held = Rc::clone(&held_key);
            $control.on_key_up(move |event| { held.set(None); event.skip(true); });
            let held = Rc::clone(&held_key);
            $control.on_kill_focus(move |event| { held.set(None); event.skip(true); });
        )+ };
    }
    bind_cutter_keys!(dialog, panel, progress_panel, progress_gauge, cancel_button, open_button, mode_choice, movement_choice, back, play_pause,
        forward, guided_primary, guided_listen, split_here, listen_part,
        delete_part, restore_part, parts_choice, rotation_choice, video_preview_button,
        add_track_button, remove_track, save_button, command_area, adjustment_panel, precision, start_back,
        start_forward, end_back, end_forward);

    let export_cancel_click = Arc::clone(&export_cancel);
    cancel_button.on_click(move |_| {
        export_cancel_click.store(true, Ordering::SeqCst);
        cancel_button.enable(false);
        status_text.set_label(labels.processing_cancelled);
    });

    let export_cancel_close = Arc::clone(&export_cancel);
    let export_busy_close = Arc::clone(&export_busy);
    let edited_close = Rc::clone(&edited);
    let guided_start_close = Rc::clone(&guided_start);
    let preview_close = Rc::clone(&preview);
    dialog.on_close(move |event| {
        if export_busy_close.load(Ordering::SeqCst) {
            if !close_after_export.get() && ask_yes_no(&dialog, labels.title, labels.close_export) {
                // The worker can finish while the confirmation's modal loop is open.
                if !export_busy_close.load(Ordering::SeqCst) {
                    preview_close.borrow_mut().stop_child();
                    dialog.end_modal(ID_CANCEL);
                    event.skip(false);
                    return;
                }
                close_after_export.set(true);
                export_cancel_close.store(true, Ordering::SeqCst);
                cancel_button.enable(false);
            }
            event.skip(false);
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

    append_podcast_log("media_cutter.dialog.open");
    dialog.show_modal();
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
