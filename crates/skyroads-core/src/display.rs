const MIN_FULLSCREEN_WIDTH: u32 = 1280;
const MIN_FULLSCREEN_HEIGHT: u32 = 720;
const MIN_FULLSCREEN_REFRESH_HZ: u32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VideoMode {
    width: u32,
    height: u32,
    refresh_hz: Option<u32>,
}

impl VideoMode {
    pub const fn new(width: u32, height: u32, refresh_hz: Option<u32>) -> Option<Self> {
        let has_dimensions = width > 0 && height > 0;
        let has_valid_refresh = !matches!(refresh_hz, Some(0));
        if !has_dimensions || !has_valid_refresh {
            return None;
        }

        Some(Self {
            width,
            height,
            refresh_hz,
        })
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    pub const fn refresh_hz(self) -> Option<u32> {
        self.refresh_hz
    }

    fn is_suitable_fullscreen_mode(self) -> bool {
        let has_suitable_resolution =
            self.width >= MIN_FULLSCREEN_WIDTH && self.height >= MIN_FULLSCREEN_HEIGHT;
        let has_suitable_refresh = self
            .refresh_hz
            .is_some_and(|refresh_hz| refresh_hz >= MIN_FULLSCREEN_REFRESH_HZ);
        has_suitable_resolution && has_suitable_refresh
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayMode {
    Windowed,
    #[default]
    BorderlessDesktop,
    ExclusiveFullscreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplaySettings {
    Windowed,
    #[default]
    BorderlessDesktop,
    ExclusiveFullscreen(VideoMode),
}

impl DisplaySettings {
    pub const fn mode(self) -> DisplayMode {
        match self {
            Self::Windowed => DisplayMode::Windowed,
            Self::BorderlessDesktop => DisplayMode::BorderlessDesktop,
            Self::ExclusiveFullscreen(_) => DisplayMode::ExclusiveFullscreen,
        }
    }

    pub const fn video_mode(self) -> Option<VideoMode> {
        match self {
            Self::ExclusiveFullscreen(mode) => Some(mode),
            Self::Windowed | Self::BorderlessDesktop => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DisplayModeCatalog {
    desktop_mode: Option<VideoMode>,
    modes: Vec<VideoMode>,
}

impl DisplayModeCatalog {
    pub fn new(
        desktop_mode: VideoMode,
        reported_modes: impl IntoIterator<Item = VideoMode>,
    ) -> Self {
        let mut modes = reported_modes
            .into_iter()
            .filter(|mode| mode.is_suitable_fullscreen_mode())
            .collect::<Vec<_>>();
        modes.sort_unstable();
        modes.dedup();

        Self {
            desktop_mode: Some(desktop_mode),
            modes,
        }
    }

    pub fn desktop_mode(&self) -> Option<VideoMode> {
        self.desktop_mode
    }

    pub fn modes(&self) -> &[VideoMode] {
        &self.modes
    }

    pub fn contains(&self, mode: VideoMode) -> bool {
        self.modes.binary_search(&mode).is_ok()
    }

    pub fn recommended_mode(&self) -> Option<VideoMode> {
        let desktop_mode = self.desktop_mode?;
        let desktop_resolution_mode = self
            .modes
            .iter()
            .copied()
            .filter(|mode| mode.width == desktop_mode.width && mode.height == desktop_mode.height)
            .max_by_key(|mode| mode.refresh_hz.unwrap_or(0));
        if desktop_resolution_mode.is_some() {
            return desktop_resolution_mode;
        }

        let largest_mode_that_fits = self
            .modes
            .iter()
            .copied()
            .filter(|mode| mode.width <= desktop_mode.width && mode.height <= desktop_mode.height)
            .max_by_key(|mode| {
                let pixel_count = u64::from(mode.width) * u64::from(mode.height);
                (pixel_count, mode.refresh_hz.unwrap_or(0))
            });
        largest_mode_that_fits.or_else(|| {
            self.modes.iter().copied().min_by_key(|mode| {
                let pixel_count = u64::from(mode.width) * u64::from(mode.height);
                (pixel_count, std::cmp::Reverse(mode.refresh_hz.unwrap_or(0)))
            })
        })
    }

    pub fn next_mode(&self, current: Option<VideoMode>) -> Option<VideoMode> {
        let current_index = current.and_then(|mode| self.modes.binary_search(&mode).ok());
        let Some(current_index) = current_index else {
            return self.recommended_mode();
        };
        let next_index = (current_index + 1) % self.modes.len();
        self.modes.get(next_index).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::{DisplayMode, DisplayModeCatalog, DisplaySettings, VideoMode};

    fn mode(width: u32, height: u32, refresh_hz: Option<u32>) -> VideoMode {
        VideoMode::new(width, height, refresh_hz).unwrap()
    }

    #[test]
    fn default_display_is_borderless_desktop() {
        assert_eq!(DisplayMode::default(), DisplayMode::BorderlessDesktop);
        assert_eq!(
            DisplaySettings::default(),
            DisplaySettings::BorderlessDesktop
        );
    }

    #[test]
    fn video_mode_rejects_zero_dimensions_and_zero_refresh() {
        assert_eq!(VideoMode::new(0, 1080, Some(60)), None);
        assert_eq!(VideoMode::new(1920, 0, Some(60)), None);
        assert_eq!(VideoMode::new(1920, 1080, Some(0)), None);
        assert_eq!(
            VideoMode::new(1920, 1080, None).map(VideoMode::refresh_hz),
            Some(None)
        );
    }

    #[test]
    fn catalog_filters_deduplicates_and_recommends_desktop_resolution_refresh() {
        let desktop = mode(3840, 2160, Some(60));
        let full_hd = mode(1920, 1080, Some(60));
        let four_k_144 = mode(3840, 2160, Some(144));
        let catalog = DisplayModeCatalog::new(
            desktop,
            [
                mode(640, 480, Some(60)),
                mode(1920, 1080, Some(30)),
                mode(1920, 1080, None),
                full_hd,
                four_k_144,
                four_k_144,
                desktop,
            ],
        );

        assert_eq!(catalog.modes(), &[full_hd, desktop, four_k_144]);
        assert_eq!(catalog.recommended_mode(), Some(four_k_144));
    }

    #[test]
    fn catalog_does_not_invent_an_exclusive_desktop_mode() {
        let legacy_desktop = mode(1024, 768, None);
        let catalog = DisplayModeCatalog::new(legacy_desktop, []);

        assert_eq!(catalog.desktop_mode(), Some(legacy_desktop));
        assert!(catalog.modes().is_empty());
        assert_eq!(catalog.recommended_mode(), None);
    }

    #[test]
    fn catalog_recommends_the_largest_suitable_fallback_below_desktop_size() {
        let desktop = mode(3840, 2160, Some(60));
        let full_hd_144 = mode(1920, 1080, Some(144));
        let catalog = DisplayModeCatalog::new(desktop, [mode(1280, 720, Some(240)), full_hd_144]);

        assert_eq!(catalog.recommended_mode(), Some(full_hd_144));
    }

    #[test]
    fn catalog_uses_the_smallest_mode_when_none_fit_the_desktop() {
        let desktop = mode(1024, 768, Some(60));
        let hd = mode(1280, 720, Some(144));
        let catalog = DisplayModeCatalog::new(desktop, [mode(3840, 2160, Some(60)), hd]);

        assert_eq!(catalog.recommended_mode(), Some(hd));
    }
}
