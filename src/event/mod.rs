//! Klavye ve terminal olayları modülü.
//! Crossterm üzerinden terminal olaylarını okur ve Nova'nın kendi
//! `Event` türüne dönüştürür.

use std::time::Duration;

/// Nova tarafından işlenen terminal olayı türleri.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Klavyeden bir tuş basıldı.
    Key(crossterm::event::KeyEvent),
    /// Terminal yeniden boyutlandırıldı.
    Resize(u16, u16),
    /// Uygulamanın çıkması isteniyor.
    Quit,
    /// Desteklenmeyen / yoksayılan olay.
    Ignored,
}

/// Terminal olaylarını okur ve Nova olaylarına dönüştürür.
pub struct EventReader;

impl EventReader {
    /// Belirtilen süre içinde bir olay varsa okur, yoksa `None` döner.
    pub fn next_event(timeout: Duration) -> Result<Option<Event>, crate::error::NovaError> {
        if crossterm::event::poll(timeout)? {
            match crossterm::event::read()? {
                crossterm::event::Event::Key(key) => {
                    if key.kind == crossterm::event::KeyEventKind::Press {
                        Ok(Some(Event::Key(key)))
                    } else {
                        Ok(Some(Event::Ignored))
                    }
                }
                crossterm::event::Event::Resize(w, h) => Ok(Some(Event::Resize(w, h))),
                _ => Ok(Some(Event::Ignored)),
            }
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_is_debug_printable() {
        let ev = Event::Quit;
        let _ = format!("{ev:?}");
    }
}
