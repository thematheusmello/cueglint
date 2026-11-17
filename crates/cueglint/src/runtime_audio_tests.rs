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
