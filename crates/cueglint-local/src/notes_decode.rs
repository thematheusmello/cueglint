//! Notes-only constrained decoding; the model still chooses every selected index.

use crate::notes::MAX_OUTPUT_BYTES;
use anyhow::{Context, Result, ensure};
use candle_core::{DType, Tensor};
use cueglint_core::NOTES_MAX_SELECTIONS_PER_CHUNK;
use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokenizers::{Model, Tokenizer, decoders::DecoderWrapper};

const MAX_VOCABULARY: usize = 262_144;
const MAX_CANDIDATES: usize = 16_384;
const MAX_PIECE_BYTES: usize = 1_048_576;
const CANCEL_INTERVAL: usize = 64;

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "highlight decoding cancelled"
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Start,
    First,
    Next,
    Number(usize),
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct State {
    minimum: usize,
    maximum: usize,
    last_index: Option<usize>,
    count: usize,
    phase: Phase,
    bytes: usize,
}

impl State {
    fn new(range: Range<usize>) -> Result<Self> {
        ensure!(
            range.start < range.end && range.end <= 10_000,
            "invalid highlight decoding range"
        );
        Ok(Self {
            minimum: range.start,
            maximum: range.end - 1,
            last_index: None,
            count: 0,
            phase: Phase::Start,
            bytes: 0,
        })
    }

    fn lower_bound(self) -> usize {
        self.last_index.map_or(self.minimum, |last| last + 1)
    }

    fn viable_number(self, prefix: usize) -> bool {
        let lower = self.lower_bound();
        let mut first = prefix;
        let mut last = prefix;
        loop {
            if first > self.maximum {
                return false;
            }
            if last >= lower {
                return true;
            }
            if prefix == 0 {
                return false;
            }
            first *= 10;
            last = last * 10 + 9;
        }
    }

    fn advance(mut self, piece: &[u8]) -> Option<Self> {
        if piece.is_empty() || self.bytes.checked_add(piece.len())? > MAX_OUTPUT_BYTES {
            return None;
        }
        for &byte in piece {
            self.phase = match (self.phase, byte) {
                (Phase::Start, b'[') => Phase::First,
                (Phase::First, b']') => Phase::Closed,
                (Phase::First | Phase::Next, b'0'..=b'9')
                    if self.count < NOTES_MAX_SELECTIONS_PER_CHUNK =>
                {
                    let number = usize::from(byte - b'0');
                    if !self.viable_number(number) {
                        return None;
                    }
                    Phase::Number(number)
                }
                (Phase::Number(prefix), b'0'..=b'9') if prefix != 0 => {
                    let number = prefix
                        .checked_mul(10)?
                        .checked_add(usize::from(byte - b'0'))?;
                    if !self.viable_number(number) {
                        return None;
                    }
                    Phase::Number(number)
                }
                (Phase::Number(number), b',' | b']')
                    if (self.lower_bound()..=self.maximum).contains(&number) =>
                {
                    self.count += 1;
                    self.last_index = Some(number);
                    if byte == b']' {
                        Phase::Closed
                    } else if self.count < NOTES_MAX_SELECTIONS_PER_CHUNK && number < self.maximum {
                        Phase::Next
                    } else {
                        return None;
                    }
                }
                _ => return None,
            };
        }
        self.bytes += piece.len();
        Some(self)
    }
}

struct Piece {
    id: u32,
    bytes: Vec<u8>,
}

pub(crate) struct Vocabulary {
    pieces: Vec<Piece>,
    end_tokens: Vec<u32>,
    size: usize,
}

impl Vocabulary {
    pub(crate) fn prepare(
        tokenizer: &Tokenizer,
        end_tokens: &[u32],
        cancel: &AtomicBool,
    ) -> Result<Self> {
        check_cancel(cancel)?;
        // ByteLevel maps this ASCII alphabet identically and concatenates without cleanup.
        ensure!(
            matches!(tokenizer.get_decoder(), Some(DecoderWrapper::ByteLevel(_))),
            "highlight decoding requires the exact ByteLevel tokenizer decoder"
        );
        // Count the union without get_vocab_size(true)'s full vocabulary clone.
        let added = tokenizer.get_added_vocabulary().get_vocab();
        let mut size = tokenizer.get_vocab_size(false);
        ensure!(
            size <= MAX_VOCABULARY && added.len() <= MAX_VOCABULARY,
            "highlight tokenizer vocabulary exceeds supported bounds"
        );
        for (index, token) in added.keys().enumerate() {
            if index % CANCEL_INTERVAL == 0 {
                check_cancel(cancel)?;
            }
            if tokenizer.get_model().token_to_id(token).is_none() {
                size += 1;
            }
        }
        ensure!(
            (1..=MAX_VOCABULARY).contains(&size),
            "highlight tokenizer vocabulary exceeds supported bounds"
        );
        ensure!(
            !end_tokens.is_empty() && end_tokens.iter().all(|id| (*id as usize) < size),
            "highlight tokenizer end tokens are outside its vocabulary"
        );
        let mut pieces = Vec::new();
        let mut piece_bytes = 0;
        for id in 0..size as u32 {
            if id as usize % CANCEL_INTERVAL == 0 {
                check_cancel(cancel)?;
            }
            let token = tokenizer
                .id_to_token(id)
                .context("highlight decoding requires dense tokenizer IDs")?;
            if tokenizer
                .get_added_vocabulary()
                .simple_id_to_token(id)
                .is_some()
                || end_tokens.contains(&id)
                || token.is_empty()
                || token.len() > MAX_OUTPUT_BYTES
                || !token.bytes().all(|byte| b"[],0123456789".contains(&byte))
            {
                continue;
            }
            piece_bytes += token.len();
            ensure!(
                pieces.len() < MAX_CANDIDATES && piece_bytes <= MAX_PIECE_BYTES,
                "highlight tokenizer candidate cache exceeds supported bounds"
            );
            pieces.push(Piece {
                id,
                bytes: token.into_bytes(),
            });
        }
        ensure!(
            b"[],0123456789"
                .iter()
                .all(|byte| pieces.iter().any(|piece| piece.bytes == [*byte])),
            "highlight tokenizer lacks required non-added ASCII character tokens"
        );
        check_cancel(cancel)?;
        Ok(Self {
            pieces,
            end_tokens: end_tokens.to_vec(),
            size,
        })
    }
}

pub(crate) struct Decoder {
    vocabulary: Arc<Vocabulary>,
    state: State,
}

impl Decoder {
    pub(crate) fn new(vocabulary: Arc<Vocabulary>, range: Range<usize>) -> Result<Self> {
        Ok(Self {
            vocabulary,
            state: State::new(range)?,
        })
    }

    pub(crate) fn mask(&self, logits: &Tensor, cancel: &AtomicBool) -> Result<Tensor> {
        check_cancel(cancel)?;
        let size = logits.dims1()?;
        ensure!(
            (self.vocabulary.size..=MAX_VOCABULARY).contains(&size),
            "highlight logits do not cover the supported tokenizer vocabulary"
        );
        let values = logits.to_dtype(DType::F32)?.to_vec1::<f32>()?;
        let mut masked = vec![f32::NEG_INFINITY; values.len()];
        for (index, piece) in self.vocabulary.pieces.iter().enumerate() {
            if index % CANCEL_INTERVAL == 0 {
                check_cancel(cancel)?;
            }
            if self.state.advance(&piece.bytes).is_some() {
                let value = values[piece.id as usize];
                ensure!(value.is_finite(), "nonfinite highlight candidate logit");
                masked[piece.id as usize] = value;
            }
        }
        if self.state.phase == Phase::Closed {
            for &id in &self.vocabulary.end_tokens {
                let value = values[id as usize];
                ensure!(value.is_finite(), "nonfinite highlight end-token logit");
                masked[id as usize] = value;
            }
        }
        ensure!(
            masked.iter().any(|value| value.is_finite()),
            "highlight decoding has no legal token"
        );
        check_cancel(cancel)?;
        Ok(Tensor::from_vec(masked, values.len(), logits.device())?)
    }

    pub(crate) fn accept(&mut self, token: u32) -> Result<()> {
        if self.vocabulary.end_tokens.contains(&token) {
            ensure!(
                self.state.phase == Phase::Closed,
                "highlight end token before complete JSON"
            );
        } else {
            let index = self
                .vocabulary
                .pieces
                .binary_search_by_key(&token, |piece| piece.id)
                .map_err(|_| anyhow::anyhow!("unsupported highlight token"))?;
            self.state = self
                .state
                .advance(&self.vocabulary.pieces[index].bytes)
                .context("highlight token violates JSON selection constraint")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::Device;
    use tokenizers::{AddedToken, decoders::byte_level::ByteLevel, models::wordlevel::WordLevel};

    fn tokenizer(extra: &[&str]) -> Tokenizer {
        let tokens = [
            "[", "]", ",", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9",
        ]
        .into_iter()
        .chain(extra.iter().copied())
        .enumerate()
        .map(|(id, token)| (token.to_owned(), id as u32))
        .collect();
        let model = WordLevel::builder().vocab(tokens).build().unwrap();
        let mut tokenizer = Tokenizer::new(model);
        tokenizer.with_decoder(Some(ByteLevel::default()));
        tokenizer.add_special_tokens(&[AddedToken::from("<|im_end|>", true)]);
        tokenizer
    }

    fn vocabulary(tokenizer: &Tokenizer) -> Arc<Vocabulary> {
        Arc::new(
            Vocabulary::prepare(
                tokenizer,
                &[tokenizer.token_to_id("<|im_end|>").unwrap()],
                &AtomicBool::new(false),
            )
            .unwrap(),
        )
    }

    #[test]
    fn exact_minified_json_is_independent_of_token_boundaries() {
        for text in ["[]", "[0]", "[0,3,17]", "[0,1,2,3,4,5,6,9999]"] {
            for split in 1..text.len() {
                let state = State::new(0..10_000).unwrap();
                let state = state.advance(&text.as_bytes()[..split]).unwrap();
                let state = state.advance(&text.as_bytes()[split..]).unwrap();
                assert_eq!(state.phase, Phase::Closed, "{text:?} split at {split}");
                assert_eq!(state.bytes, text.len());
            }
            let mut state = State::new(0..10_000).unwrap();
            for byte in text.bytes() {
                state = state.advance(&[byte]).unwrap();
            }
            assert_eq!(state.phase, Phase::Closed);
        }
    }

    #[test]
    fn malformed_out_of_order_and_ninth_indices_are_never_prefixes() {
        for invalid in [
            " [0]",
            "[0] ",
            "[00]",
            "[01]",
            "[0,]",
            "[,0]",
            "[0,0]",
            "[3,2]",
            "[-1]",
            "[1.0]",
            "[1e1]",
            "[\"1\"]",
            "[10]",
            "[0,1,2,3,4,5,6,7,8]",
            "[0][1]",
            "[0] prose",
            "[0,9999999999999999999999999999]",
        ] {
            assert!(
                State::new(0..10)
                    .unwrap()
                    .advance(invalid.as_bytes())
                    .is_none(),
                "accepted {invalid:?}"
            );
        }
        assert!(State::new(0..0).is_err());
        assert!(State::new(0..10_001).is_err());
    }

    #[test]
    fn decimal_prefixes_can_grow_into_range_but_cannot_close_early() {
        let state = State::new(2..12).unwrap().advance(b"[1").unwrap();
        assert!(state.advance(b"]").is_none());
        assert_eq!(state.advance(b"0]").unwrap().phase, Phase::Closed);
        assert_eq!(state.advance(b"1]").unwrap().phase, Phase::Closed);
        assert!(state.advance(b"2").is_none());
        let state = State::new(0..112).unwrap().advance(b"[9,1").unwrap();
        assert!(state.advance(b"]").is_none());
        assert_eq!(state.advance(b"00]").unwrap().phase, Phase::Closed);
        assert!(State::new(2..12).unwrap().advance(b"[0").is_none());
        assert!(State::new(10..20).unwrap().advance(b"[2").is_none());
    }

    #[test]
    fn byte_limit_is_checked_before_any_token_transition() {
        let mut state = State::new(0..1).unwrap().advance(b"[").unwrap();
        state.bytes = MAX_OUTPUT_BYTES - 1;
        let before = state;
        assert_eq!(state.advance(b"]").unwrap().bytes, MAX_OUTPUT_BYTES);
        assert!(state.advance(b"0]").is_none());
        assert_eq!(state, before);
        assert!(state.advance(b"").is_none());
    }

    #[test]
    fn masks_every_byte_of_merged_pieces_and_only_allows_eos_after_close() -> Result<()> {
        let tokenizer = tokenizer(&["[1", "0,11]", "0,11,11]", "[0]", "word", "Ġ[", "é"]);
        let mut decoder = Decoder::new(vocabulary(&tokenizer), 2..12)?;
        let eos = tokenizer.token_to_id("<|im_end|>").unwrap();
        let logits = Tensor::new(vec![1f32; tokenizer.get_vocab_size(true) + 2], &Device::Cpu)?;
        let values = decoder
            .mask(&logits, &AtomicBool::new(false))?
            .to_vec1::<f32>()?;
        let permitted =
            |text: &str| values[tokenizer.token_to_id(text).unwrap() as usize].is_finite();
        assert!(permitted("[") && permitted("[1"));
        assert!(!permitted("[0]") && !permitted("word") && !permitted("Ġ[") && !permitted("é"));
        assert!(!values[eos as usize].is_finite());
        assert!(values[values.len() - 1].is_infinite());
        assert!(decoder.accept(eos).is_err());
        decoder.accept(tokenizer.token_to_id("[1").unwrap())?;
        let values = decoder
            .mask(&logits, &AtomicBool::new(false))?
            .to_vec1::<f32>()?;
        assert!(values[tokenizer.token_to_id("0,11]").unwrap() as usize].is_finite());
        assert!(!values[tokenizer.token_to_id("0,11,11]").unwrap() as usize].is_finite());
        decoder.accept(tokenizer.token_to_id("0,11]").unwrap())?;
        let values = decoder
            .mask(&logits, &AtomicBool::new(false))?
            .to_vec1::<f32>()?;
        assert_eq!(values.iter().filter(|value| value.is_finite()).count(), 1);
        assert!(values[eos as usize].is_finite());
        decoder.accept(eos)?;
        let ids = ["[1", "0,11]"].map(|text| tokenizer.token_to_id(text).unwrap());
        assert_eq!(tokenizer.decode(&ids, true).unwrap(), "[10,11]");
        Ok(())
    }

    #[test]
    fn unsupported_decoders_added_ascii_tokens_and_cancellation_fail_closed() -> Result<()> {
        let mut tokenizer = tokenizer(&[]);
        let eos = [tokenizer.token_to_id("<|im_end|>").unwrap()];
        let cancel = AtomicBool::new(true);
        assert!(Vocabulary::prepare(&tokenizer, &eos, &cancel).is_err());
        cancel.store(false, Ordering::Relaxed);
        let decoder = Decoder::new(vocabulary(&tokenizer), 0..2)?;
        let logits = Tensor::new(vec![0f32; tokenizer.get_vocab_size(true)], &Device::Cpu)?;
        cancel.store(true, Ordering::Relaxed);
        assert!(decoder.mask(&logits, &cancel).is_err());
        cancel.store(false, Ordering::Relaxed);
        tokenizer.with_decoder(None::<ByteLevel>);
        assert!(Vocabulary::prepare(&tokenizer, &eos, &cancel).is_err());
        tokenizer.with_decoder(Some(ByteLevel::default()));
        tokenizer.add_special_tokens(&[AddedToken::from("[", true)]);
        assert!(Vocabulary::prepare(&tokenizer, &eos, &cancel).is_err());
        Ok(())
    }

    #[test]
    fn empty_masks_nonfinite_logits_and_wrong_logit_shapes_fail_closed() -> Result<()> {
        let tokenizer = tokenizer(&[]);
        let mut decoder = Decoder::new(vocabulary(&tokenizer), 0..2)?;
        let size = tokenizer.get_vocab_size(true);
        let cancel = AtomicBool::new(false);
        let logits = Tensor::new(vec![0f32; size], &Device::Cpu)?;
        decoder.state.bytes = MAX_OUTPUT_BYTES;
        assert!(decoder.mask(&logits, &cancel).is_err());
        decoder.state.bytes = 0;
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let logits = Tensor::new(vec![invalid; size], &Device::Cpu)?;
            assert!(decoder.mask(&logits, &cancel).is_err());
        }
        assert!(decoder.mask(&logits.unsqueeze(0)?, &cancel).is_err());
        let too_short = Tensor::new(vec![0f32; size - 1], &Device::Cpu)?;
        assert!(decoder.mask(&too_short, &cancel).is_err());
        Ok(())
    }
}
