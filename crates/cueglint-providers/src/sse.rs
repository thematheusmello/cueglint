use crate::{ErrorKind, ProviderError, ProviderResult};

pub(crate) const MAX_EVENT_BYTES: usize = 256 * 1024;
pub(crate) const MAX_STREAM_BYTES: usize = 2 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct Decoder {
    line: Vec<u8>,
    data: Vec<u8>,
    event: String,
    bytes: usize,
    events: usize,
    after_cr: bool,
    seen_line: bool,
}

pub(crate) struct Event {
    pub name: String,
    pub data: String,
}

impl Decoder {
    /// UTF-8 is decoded only after an entire SSE line arrives, including fragmented code points.
    pub fn push(
        &mut self,
        bytes: &[u8],
        mut on_event: impl FnMut(Event) -> ProviderResult<()>,
    ) -> ProviderResult<()> {
        self.bytes = self.bytes.checked_add(bytes.len()).ok_or_else(limit)?;
        if self.bytes > MAX_STREAM_BYTES {
            return Err(limit());
        }
        for &byte in bytes {
            if self.after_cr && byte == b'\n' {
                self.after_cr = false;
                continue;
            }
            self.after_cr = byte == b'\r';
            if byte == b'\n' || byte == b'\r' {
                if let Some(event) = self.finish_line()? {
                    on_event(event)?;
                }
            } else {
                if self.line.len() + self.data.len() >= MAX_EVENT_BYTES {
                    return Err(limit());
                }
                self.line.push(byte);
            }
        }
        Ok(())
    }

    fn finish_line(&mut self) -> ProviderResult<Option<Event>> {
        let bytes = std::mem::take(&mut self.line);
        let line =
            std::str::from_utf8(&bytes).map_err(|_| ProviderError::new(ErrorKind::Protocol))?;
        let line = if std::mem::replace(&mut self.seen_line, true) {
            line
        } else {
            line.strip_prefix('\u{feff}').unwrap_or(line)
        };
        if line.is_empty() {
            if self.data.is_empty() {
                self.event.clear();
                return Ok(None);
            }
            self.events += 1;
            if self.events > 16_384 {
                return Err(limit());
            }
            self.data.pop();
            let data = String::from_utf8(std::mem::take(&mut self.data))
                .map_err(|_| ProviderError::new(ErrorKind::Protocol))?;
            return Ok(Some(Event {
                name: std::mem::take(&mut self.event),
                data,
            }));
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => {
                if value.len() > 128 {
                    return Err(limit());
                }
                self.event = value.to_owned();
            }
            "data" => {
                if self.data.len() + value.len() + 1 > MAX_EVENT_BYTES {
                    return Err(limit());
                }
                self.data.extend_from_slice(value.as_bytes());
                self.data.push(b'\n');
            }
            _ => {}
        }
        Ok(None)
    }
}

fn limit() -> ProviderError {
    ProviderError::new(ErrorKind::LimitExceeded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragmented_unicode_multiline_and_all_line_endings() {
        let bytes =
            "\u{feff}: heartbeat\r\nevent: message\rdata: {\"text\":\"Olá 🌍\",\r\ndata: \"ok\":true}\n\n"
                .as_bytes();
        let mut decoder = Decoder::default();
        let mut events = Vec::new();
        for byte in bytes {
            decoder
                .push(&[*byte], |event| {
                    events.push(event);
                    Ok(())
                })
                .unwrap();
        }
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "message");
        let value: serde_json::Value = serde_json::from_str(&events[0].data).unwrap();
        assert_eq!(value["text"], "Olá 🌍");
        assert_eq!(value["ok"], true);
    }

    #[test]
    fn missing_delimiter_is_not_a_completed_event_and_buffers_are_bounded() {
        let mut decoder = Decoder::default();
        decoder
            .push(b"data: [DONE]\n", |_| panic!("unterminated event"))
            .unwrap();
        assert_eq!(
            decoder
                .push(&vec![b'x'; MAX_EVENT_BYTES + 1], |_| Ok(()))
                .unwrap_err()
                .kind,
            ErrorKind::LimitExceeded
        );
        let mut decoder = Decoder::default();
        assert_eq!(
            decoder
                .push(b"data: \xff\n\n", |_| Ok(()))
                .unwrap_err()
                .kind,
            ErrorKind::Protocol
        );
    }
}
