# Native screenshot ownership

Cueglint's explicit primary-window screenshot request has one UUID and one
bounded lifecycle. A request is not fulfilled by asking egui to capture, by a
surface acquisition, or by submitting a copy. It requires a matching ROOT/UUID
readback and the screenshot worker's successful private PNG export.

The native renderer borrows pending request tokens. Missing render state,
window/surface absence, recreation failure and surface acquisition skips leave
the tokens with their viewport owner. Only a submitted command buffer containing
the capture copy transfers them to asynchronous readback. An ordinary renderer
queue submission without a capture copy does not acknowledge a screenshot.
After copy submission, format or map failure remains a failure; it does not
produce an image or resubmit the request.

Each viewport retains at most 16 pending tokens. Overflow is rejected and traced;
viewport removal or shutdown drops unsubmitted tokens. The app emits its request
once and never uses a timer to resend it. Existing repaint scheduling remains
responsible for advancing the renderer and readback callbacks.

Optional request-local diagnostics are created only for an explicit screenshot.
Each of 17 fixed transport stages is emitted at most once per request, including
surface skips, actual copy submission, map failure and prepared readback. The
request UUID, fixture and viewport bind these diagnostics to the exported PNG's
receipt. The native runner requires a unique matching requested/copy-submitted/
readback-ready chain and rejects terminal failure traces, even if a PNG exists.
PNG validation still checks actual pixels, dimensions, scale and file hashes.

Explicit capture also waits for settled application fonts. The existing legacy
platform-file loader reads at most 32 MiB on one startup thread and returns one
result to application logic. Logic queues the definitions, observes those exact
definitions in egui after the next root pass, and waits for that UI pass to
complete before releasing a screenshot request. Headless contexts use the named,
hashed embedded fallback without a filesystem thread. A timer or a worker result
alone does not establish font readiness.

Requested and saved receipts bind the same settled font-set identity. Developer
metadata records registered face names, indices, hashes, coordinates, families and
fallback reasons, never font bytes. It identifies `legacy-platform-file` rather
than claiming the separate OS resolver was used. This is registered-font-set
provenance, not a glyph-by-glyph or reference-font match. The separate resolver and
declared typography candidates remain unwired by this readiness repair.

This uses a pinned local patch of the already-installed egui-wgpu 0.35.0, retaining
its licenses and compatibility by-value API. Its added borrowed-queue API is used
by the existing native ROOT/deferred renderer. The immediate-child caller still
passes an empty capture batch; no child-window readback support is claimed.
Web and Glow painters use their existing separate paths.

Retained failure

The combined source 85a497932b2a283179f75d75c617c003a4e61355, native executable
938cde031d4f353ade1577cb9e80e55df18c3e004577b6f0d663115c020e6e05,
passed all seven native-v2 fixtures and five of six legacy fixtures. Legacy
`markdown` reached the ROOT requested stage, then timed out after 30 seconds
without an event, receipt or PNG. The run was not retried. Its null visibility
value means occlusion was unknown; it does not prove the window was hidden.
The old trace cannot distinguish a pre-submission surface skip from a map failure.
This repair addresses the source-confirmed token-loss boundary, without assigning
an unobserved cause to that historical failure.

Verification plan

- Drive the real borrowed-queue painter with no GPU backends/render state/window
  and prove that the exact request survives repeated skipped paints without an
  image or success acknowledgement.
- Test the production submission/handoff function: unwinding before submission
  retains ownership; a submission without a copy retains ownership; a successful
  copy submission transfers the exact token once.
- Exercise the real viewport owner for capacity, token order, unrelated clipboard
  actions, overflow diagnostics and destruction cleanup.
- Drive native egui screenshot events through the app and bounded worker queue;
  reject a stale UUID and the wrong viewport before accepting the matching image.
- Validate runner rejection of missing/mismatched request traces and genuine
  failure stages independently of valid PNG bytes.

Source preparation and standalone formatting do not verify GPU behavior. Builds,
headless tests and new native captures require separate retained results against
this source. A future successful native run does not erase the failed combined
run or establish reference, font, OS material, inference or full-product parity.
