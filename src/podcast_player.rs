#[cfg(target_os = "macos")]
mod imp {
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_av_foundation::AVPlayer;
    use objc2_core_media::CMTime;
    use objc2_foundation::{NSString, NSURL};

    pub struct PodcastPlayer {
        player: Retained<AVPlayer>,
    }

    impl PodcastPlayer {
        pub fn new(url: &str) -> Result<Self, String> {
            let player = build_player(url)?;
            Ok(Self { player })
        }

        pub fn play(&self) -> Result<(), String> {
            ensure_main_thread()?;
            unsafe {
                self.player.play();
            }
            Ok(())
        }

        pub fn debug_snapshot(&self) -> Result<String, String> {
            ensure_main_thread()?;
            let player_status = unsafe { self.player.status().0 };
            let time_control_status = unsafe { self.player.timeControlStatus().0 };
            let rate = unsafe { self.player.rate() };
            let current_time = unsafe { self.player.currentTime().seconds() };
            let player_error = format!("{:?}", unsafe { self.player.error() });

            let (item_status, item_error) = if let Some(item) = unsafe { self.player.currentItem() }
            {
                (
                    unsafe { item.status().0 },
                    format!("{:?}", unsafe { item.error() }),
                )
            } else {
                (-1, "None".to_string())
            };

            Ok(format!(
                "player_status={player_status} time_control_status={time_control_status} rate={rate} current_time={current_time} item_status={item_status} player_error={player_error} item_error={item_error}"
            ))
        }

        pub fn is_ready_for_playback(&self) -> Result<bool, String> {
            ensure_main_thread()?;
            let time_control_status = unsafe { self.player.timeControlStatus().0 };
            let rate = unsafe { self.player.rate() };
            let Some(item) = (unsafe { self.player.currentItem() }) else {
                return Ok(false);
            };
            let item_status = unsafe { item.status().0 };
            let buffer_empty = unsafe { item.isPlaybackBufferEmpty() };
            let likely_to_keep_up = unsafe { item.isPlaybackLikelyToKeepUp() };

            Ok(item_status == 1
                && (time_control_status == 2 || likely_to_keep_up || (!buffer_empty && rate > 0.0)))
        }

        pub fn pause(&self) -> Result<(), String> {
            ensure_main_thread()?;
            unsafe {
                self.player.pause();
            }
            Ok(())
        }

        pub fn seek_by_seconds(&self, offset_seconds: f64) -> Result<(), String> {
            ensure_main_thread()?;
            let current = unsafe { self.player.currentTime() };
            let current_seconds = unsafe { current.seconds() };
            let target_seconds = (current_seconds + offset_seconds).max(0.0);
            let target = unsafe { CMTime::with_seconds(target_seconds, 600) };
            unsafe {
                self.player.seekToTime(target);
            }
            Ok(())
        }
    }

    fn build_player(url: &str) -> Result<Retained<AVPlayer>, String> {
        let mtm = ensure_main_thread()?;
        let url = make_nsurl(url, mtm)?;
        Ok(unsafe { AVPlayer::playerWithURL(&url, mtm) })
    }

    fn make_nsurl(url: &str, _mtm: MainThreadMarker) -> Result<Retained<NSURL>, String> {
        let ns_string = NSString::from_str(url);
        NSURL::URLWithString(&ns_string).ok_or_else(|| format!("URL podcast non valido: {url}"))
    }

    fn ensure_main_thread() -> Result<MainThreadMarker, String> {
        MainThreadMarker::new().ok_or_else(|| {
            "Il player podcast macOS deve essere usato dal thread principale".to_string()
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub struct PodcastPlayer;

    impl PodcastPlayer {
        pub fn new(_url: &str) -> Result<Self, String> {
            Err("Player podcast interno disponibile solo su macOS".to_string())
        }

        pub fn play(&self) -> Result<(), String> {
            Err("Player podcast interno disponibile solo su macOS".to_string())
        }

        pub fn debug_snapshot(&self) -> Result<String, String> {
            Err("Player podcast interno disponibile solo su macOS".to_string())
        }

        pub fn is_ready_for_playback(&self) -> Result<bool, String> {
            Err("Player podcast interno disponibile solo su macOS".to_string())
        }

        pub fn pause(&self) -> Result<(), String> {
            Err("Player podcast interno disponibile solo su macOS".to_string())
        }

        pub fn seek_by_seconds(&self, _offset_seconds: f64) -> Result<(), String> {
            Err("Player podcast interno disponibile solo su macOS".to_string())
        }
    }
}

pub use imp::PodcastPlayer;
