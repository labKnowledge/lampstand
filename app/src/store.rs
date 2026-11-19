use crate::model::{Library, LiveState, Settings, Showable};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub struct Saved {
    pub settings: Settings,
    pub library: Library,
    pub state: LiveState,
}

pub fn app_dir() -> PathBuf {
    let base = dirs::config_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from("."));
    base.join("Lampstand")
}

pub fn bibles_dir() -> PathBuf {
    app_dir().join("bibles")
}

fn file() -> PathBuf {
    app_dir().join("lampstand.json")
}

pub fn load() -> Option<Saved> {
    let text = std::fs::read_to_string(file()).ok()?;
    let mut saved: Saved = serde_json::from_str(&text).ok()?;
    let st = &mut saved.state;
    st.is_live = false;
    for s in [&mut st.lower_third.show, &mut st.title.show, &mut st.caption.show, &mut st.ticker.show, &mut st.logo.show, &mut st.brk.show] {
        *s = Showable::default();
    }
    st.brk.ends_at = None;
    st.caption.lines.clear();
    for o in st.outputs.iter_mut() {
        o.black = false;
        o.black_t0 = -10.0;
        o.clean = false;
        o.clean_t0 = -10.0;
        o.pip_t0 = -10.0;
        o.bg_t0 = -10.0;
        o.slide_t0 = -10.0;
        o.prev_slide = None;
        o.prev_background = None;
    }
    Some(saved)
}

pub fn save(saved: &Saved) {
    let _ = std::fs::create_dir_all(app_dir());
    if let Ok(text) = serde_json::to_string_pretty(saved) {
        let tmp = file().with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, file());
        }
    }
}
