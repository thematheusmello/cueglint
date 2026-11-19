//! Synthetic implementation checks; these do not establish live capture or reference parity.
use super::*;

fn packet(source: Source, sequence: u64) -> AudioPacket {
    AudioPacket {
        source,
        sequence,
        start_ns: sequence * 64_000_000,
        sample_rate: 16_000,
        len: 1024,
        samples: [0.05; 1024],
    }
}

fn utterance(source: Source, id: &str, finalized: bool) -> Utterance {
    Utterance {
        id: id.into(),
        source,
        start_ns: 0,
        end_ns: 64_000_000,
        samples: vec![0.05; 1024],
        finalized,
    }
}

#[test]
fn stop_failure_drains_both_sources_and_finalizes_each_identity_once() {
    let sources = [Source::Microphone, Source::SystemAudio];
    let mut endpoints = sources.map(Endpoint::new);
    let mut partials = Vec::new();
    for (endpoint, source) in endpoints.iter_mut().zip(sources) {
        for sequence in 0..32 {
            partials.extend(endpoint.push(&packet(source, sequence)).unwrap());
        }
    }
    assert_eq!(partials.len(), 2);
    let (packets, buffered) = mpsc::sync_channel(4);
    for sequence in 32..34 {
        for source in sources {
            packets.send(packet(source, sequence)).unwrap();
        }
    }
    drop(packets);
    let (finals, decoded) = mpsc::sync_channel(2);
    let error = finish_capture_audio(
        Err(std::io::Error::other("microphone stop failed").into()),
        &buffered,
        &mut endpoints,
        |utterance| queue_utterance(&finals, utterance, 123, || {}),
    )
    .unwrap_err();
    assert!(error.downcast_ref::<std::io::Error>().is_some());
    let finals: Vec<_> = decoded.try_iter().collect();
    assert_eq!(finals.len(), 2);
    for ((finalized, partial), source) in finals.iter().zip(&partials).zip(sources) {
        assert_eq!(finalized.id, partial.id);
        assert_eq!(finalized.source, source);
        assert!(finalized.finalized);
        assert_eq!(finalized.samples.len(), 34 * 1024);
        assert_eq!(finalized.start_ns, 123);
        assert_eq!(finalized.end_ns, 34 * 64_000_000 + 123);
    }
    assert!(buffered.try_recv().is_err());
    assert!(
        endpoints
            .iter_mut()
            .all(|endpoint| endpoint.finish().is_none())
    );
}

#[test]
fn two_source_fixture_drains_saves_and_reopens() -> Result<()> {
    const PACKET_NS: u64 = 64_000_000;
    const TIMELINE_OFFSET_NS: u64 = 123_000_000;
    const PARTIAL_PACKETS: u64 = 32;

    const FINAL_PACKETS: u64 = 34;
    const ROUTE_SPACING_NS: u64 = 20_000_000;

    // Timeline order differs from the microphone-first final drain.
    let tracks = [
        (
            Source::SystemAudio,
            TranscriptSource::SystemAudio,
            0.1,
            ["Lantern", "Lantern stock is 14."],
        ),
        (
            Source::Microphone,
            TranscriptSource::Microphone,
            0.2,
            ["Cedar", "Cedar stock is 31."],
        ),
    ];
    let directory = std::env::temp_dir().join(format!("cueglint-two-source-{}", uuid()));
    std::fs::create_dir(&directory)?;
    let database = directory.join("meeting.sqlite3");
    let mut store = Store::open(&database)?;
    let initial = store.start_session("Original two-source fixture", None)?;
    let session = &initial.id;
    let mut runtime = Runtime::new(&store, true, egui::Context::default())?;
    let stop = Arc::new(AtomicBool::new(false));
    runtime.capture = Some(CaptureJob {
        session: session.clone(),
        stop: stop.clone(),
        end_action: None,
    });

    let (packets, buffered) = mpsc::sync_channel(PACKET_QUEUE_CAPACITY);
    for sequence in 0..FINAL_PACKETS {
        for (index, &(source, _, tail_sample, _)) in tracks.iter().enumerate().rev() {
            let mut packet = packet(source, sequence);
            packet.start_ns += index as u64 * ROUTE_SPACING_NS;
            if sequence >= PARTIAL_PACKETS {
                packet.samples.fill(tail_sample);
            }
            packets.try_send(packet)?;
        }
    }
    drop(packets);
    assert!(runtime.end_capture(session, EndAction::Stop)?);
    assert!(stop.load(Ordering::Relaxed));

    let mut endpoints = [
        Endpoint::new(Source::Microphone),
        Endpoint::new(Source::SystemAudio),
    ];
    let updates = tracks.len() * 2;
    let (utterances, pending) = mpsc::sync_channel(updates);
    let mut emitted = 0;
    finish_capture_audio(Ok(()), &buffered, &mut endpoints, |utterance| {
        // The drain and decoder run serially here. Fail before oversupplying the queue.
        ensure!(emitted < updates, "unexpected extra fixture utterance");
        emitted += 1;
        queue_utterance(&utterances, utterance, TIMELINE_OFFSET_NS, || {})
    })?;
    drop(utterances);
    assert_eq!(emitted, updates);
    assert!(buffered.try_recv().is_err());

    let profile = Arc::new(models::profile(models::TRANSCRIPTION_PROFILE)?);
    transcribe_pending(
        pending,
        &stop,
        |utterance| {
            let &(_, _, tail_sample, texts) = tracks
                .iter()
                .find(|(source, _, _, _)| *source == utterance.source)
                .context("unexpected fixture audio source")?;
            let stage = usize::from(utterance.finalized);
            let packet_count = [PARTIAL_PACKETS, FINAL_PACKETS][stage];
            assert_eq!(utterance.samples.len(), (packet_count * 1024) as usize);
            let (prefix, tail) = utterance
                .samples
                .split_at((PARTIAL_PACKETS * 1024) as usize);
            assert!(prefix.iter().all(|sample| *sample == 0.05));
            assert!(tail.iter().all(|sample| *sample == tail_sample));
            Ok(texts[stage].to_owned())
        },
        |utterance, text| {
            runtime.tx.try_send(Event::Transcript(
                session.clone(),
                utterance,
                text,
                profile.clone(),
                "fixture".into(),
                "en".into(),
            ))?;
            Ok(())
        },
    )?;
    runtime.tx.try_send(Event::CaptureEnded(session.clone()))?;

    for _ in &tracks {
        let event = runtime
            .try_event()
            .context("missing partial transcript event")?;
        assert!(runtime.apply(&mut store, event)?.0);
    }
    let partials = store.session(session)?;
    assert_eq!(partials.transcript.len(), tracks.len());
    assert_eq!(partials.context_revision, initial.context_revision);

    for _ in &tracks {
        let event = runtime
            .try_event()
            .context("missing final transcript event")?;
        assert!(runtime.apply(&mut store, event)?.0);
    }
    let finalizing = store.session(session)?;
    assert_eq!(finalizing.status, SessionStatus::Recording);
    assert!(finalizing.saved_at.is_none());
    assert_eq!(finalizing.transcript.len(), tracks.len());
    assert_eq!(finalizing.context_revision, initial.context_revision + 2);
    for (index, &(_, source, _, [partial_text, final_text])) in tracks.iter().enumerate() {
        let partial = &partials.transcript[index];
        let finalized = &finalizing.transcript[index];
        assert!(!partial.finalized && finalized.finalized);
        assert_eq!(partial.id, finalized.id);
        assert_eq!(partial.source, source);
        assert_eq!(finalized.source, source);
        assert_eq!(partial.text, partial_text);
        assert_eq!(finalized.text, final_text);
        let previous = partial
            .audio
            .as_ref()
            .context("missing partial audio metadata")?;
        let audio = finalized
            .audio
            .as_ref()
            .context("missing final audio metadata")?;
        let start_ns = TIMELINE_OFFSET_NS + index as u64 * ROUTE_SPACING_NS;
        assert_eq!(audio.start_ns, start_ns);
        assert_eq!(audio.end_ns, start_ns + FINAL_PACKETS * PACKET_NS);
        assert_eq!(previous.start_ns, audio.start_ns);
        assert_eq!(
            previous.end_ns,
            audio.start_ns + PARTIAL_PACKETS * PACKET_NS
        );
        assert_eq!(audio.sample_rate, 16_000);
        assert_eq!(audio.model_profile, profile.id);
        assert_eq!(audio.backend.as_deref(), Some("fixture"));
        assert_eq!(audio.language, "en");
    }
    assert_ne!(finalizing.transcript[0].id, finalizing.transcript[1].id);

    let ended = runtime.try_event().context("missing capture-ended event")?;
    let (changed, notice) = runtime.apply(&mut store, ended)?;
    assert!(changed);
    assert_eq!(notice.as_deref(), Some("Session saved locally."));
    assert!(runtime.try_event().is_none());
    let saved = store.session(session)?;
    assert_eq!(saved.status, SessionStatus::Stopped);
    assert!(saved.saved_at.is_some());
    assert_eq!(saved.transcript, finalizing.transcript);
    drop(runtime);
    drop(store);
    let reopened = Store::open(&database)?;
    assert_eq!(reopened.session(session)?, saved);
    drop(reopened);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn invalid_packet_and_failed_delivery_do_not_skip_remaining_buffered_audio() {
    let mut endpoints = [
        Endpoint::new(Source::Microphone),
        Endpoint::new(Source::SystemAudio),
    ];
    endpoints[0].push(&packet(Source::Microphone, 0)).unwrap();
    endpoints[1].push(&packet(Source::SystemAudio, 0)).unwrap();
    let (packets, buffered) = mpsc::sync_channel(3);
    let mut invalid = packet(Source::Microphone, 1);
    invalid.samples[0] = f32::NAN;
    packets.send(invalid).unwrap();
    packets.send(packet(Source::SystemAudio, 1)).unwrap();
    packets.send(packet(Source::Microphone, 2)).unwrap();
    drop(packets);
    let mut attempted = Vec::new();
    let error = finish_capture_audio(Ok(()), &buffered, &mut endpoints, |utterance| {
        attempted.push(utterance);
        if attempted.len() == 1 {
            anyhow::bail!("first final delivery failed");
        }
        Ok(())
    })
    .unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("draining Microphone audio packet 1"));
    assert!(message.contains("invalid audio sample"));
    assert!(message.contains("first final delivery failed"));
    assert_eq!(attempted.len(), 3);
    assert!(attempted.iter().all(|utterance| utterance.finalized));
    assert_eq!(attempted[0].source, Source::Microphone);
    assert_eq!(attempted[0].samples.len(), 1024);
    assert_ne!(attempted[0].id, attempted[1].id);
    assert_eq!(attempted[1].source, Source::Microphone);
    assert_eq!(attempted[1].samples.len(), 1024);
    assert_eq!(attempted[2].source, Source::SystemAudio);
    assert_eq!(attempted[2].samples.len(), 2048);
    assert!(
        endpoints
            .iter_mut()
            .all(|endpoint| endpoint.finish().is_none())
    );
}

#[test]
fn final_drain_has_a_packet_budget_and_reports_any_undelivered_tail() {
    let mut endpoints = [
        Endpoint::new(Source::Microphone),
        Endpoint::new(Source::SystemAudio),
    ];
    let (packets, buffered) = mpsc::sync_channel(PACKET_QUEUE_CAPACITY + 1);
    for sequence in 0..=PACKET_QUEUE_CAPACITY {
        packets
            .send(packet(Source::Microphone, sequence as u64))
            .unwrap();
    }
    let mut updates = Vec::new();
    let error = finish_capture_audio(Ok(()), &buffered, &mut endpoints, |utterance| {
        updates.push(utterance);
        Ok(())
    })
    .unwrap_err();
    let final_audio = updates.last().unwrap();
    assert!(final_audio.finalized);
    assert_eq!(final_audio.samples.len(), PACKET_QUEUE_CAPACITY * 1024);
    let message = error.to_string();
    assert!(message.contains("audio continued after stop"));
    assert!(message.contains("undelivered Microphone audio packet 128"));
    assert!(
        endpoints
            .iter_mut()
            .all(|endpoint| endpoint.finish().is_none())
    );
}

#[test]
fn decoding_failure_requests_stop_and_drains_later_finals_with_stable_ids() {
    let (queued, pending) = mpsc::sync_channel(3);
    queued
        .send(utterance(Source::Microphone, "mic", false))
        .unwrap();
    queued
        .send(utterance(Source::Microphone, "mic", true))
        .unwrap();
    queued
        .send(utterance(Source::SystemAudio, "system", true))
        .unwrap();
    drop(queued);
    let stop = AtomicBool::new(false);
    let mut delivered = Vec::new();
    let error = transcribe_pending(
        pending,
        &stop,
        |utterance| {
            if !utterance.finalized {
                anyhow::bail!("synthetic decoder failure");
            }
            assert!(stop.load(Ordering::Relaxed));
            Ok(format!("final {}", utterance.id))
        },
        |utterance, text| {
            delivered.push((utterance, text));
            Ok(())
        },
    )
    .unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("transcribing Microphone segment mic"));
    assert!(message.contains("synthetic decoder failure"));
    assert_eq!(delivered.len(), 2);
    assert_eq!(delivered[0].0.id, "mic");
    assert_eq!(delivered[0].0.source, Source::Microphone);
    assert_eq!(delivered[1].0.id, "system");
    assert_eq!(delivered[1].0.source, Source::SystemAudio);
    assert!(delivered.iter().all(|(utterance, _)| utterance.finalized));
}

#[test]
fn failed_transcript_delivery_disconnects_capture_queue_before_join() {
    let (queued, pending) = mpsc::sync_channel(2);
    queued
        .send(utterance(Source::Microphone, "mic", true))
        .unwrap();
    let stop = AtomicBool::new(false);
    let error = transcribe_pending(
        pending,
        &stop,
        |_| Ok("decoded".into()),
        |_, _| anyhow::bail!("UI event receiver disconnected"),
    )
    .unwrap_err();
    assert!(stop.load(Ordering::Relaxed));
    assert!(format!("{error:#}").contains("delivering Microphone segment mic"));
    assert!(matches!(
        queued.try_send(utterance(Source::SystemAudio, "system", true)),
        Err(TrySendError::Disconnected(_))
    ));
    let (finals, decoded) = mpsc::sync_channel(1);
    drop(decoded);
    let error = queue_utterance(
        &finals,
        utterance(Source::SystemAudio, "system", true),
        0,
        || {},
    )
    .unwrap_err();
    assert!(error.to_string().contains("SystemAudio segment system"));
}
