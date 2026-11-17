# Companion, sharing and synchronization discovery

Discovery date: **2026-11-17 UTC**. These are current public-page observations
and dated documentation claims. No companion application was installed or run,
no account was opened, and no sharing or synchronization action was performed.
Every companion runtime baseline remains **unpinned and unobserved**. The
desktop 2.1.28 investigation does not pin these other surfaces.

The machine-readable companion is `reference-companions.json`. Its surface IDs
are stable discovery identities, not statements of implemented or verified parity.
Raw response evidence and hashes are kept in the private reference workspace.

## Discovered surfaces

| Stable surface ID | Scope | Reviewed documentation claim or public boundary |
| --- | --- | --- |
| `desktop.dashboard.documented` | desktop | Dated dashboard guide describes meeting records, summary editing and public summary links. It identifies itself as desktop guidance. [Dashboard guide](https://support.cluely.com/en/articles/12101340-how-to-use-cluely-s-dashboard-app) |
| `companion.mobile.ios` | mobile | Current official download links to App Store ID `6755442775`. The listing reports 1.2.7 and iOS 17+, without establishing an installed build. [Mobile page](https://cluely.com/mobile), [App Store listing](https://apps.apple.com/us/app/cluely-ai-notetaker/id6755442775) |
| `companion.mobile.microphone` | mobile | Guide documents microphone-based in-person or voice-memo recording and transcript creation. [Mobile guide](https://support.cluely.com/en/articles/13008320-how-to-use-cluely-mobile) |
| `companion.mobile.records` | mobile, sync | Guide describes browsing notes, title/notes editing, transcript copying, resume, share and delete actions. [Mobile guide](https://support.cluely.com/en/articles/13008320-how-to-use-cluely-mobile) |
| `companion.mobile.assistant` | mobile | Guide describes questions during recording and questions about a completed meeting. [Mobile guide](https://support.cluely.com/en/articles/13008320-how-to-use-cluely-mobile) |
| `companion.browser.dashboard` | browser | Transcript guide explicitly names a web app with summary, transcript and usage views, copying and meeting chat. The unauthenticated `app.cluely.com` request reached an authentication redirect. [Transcript guide](https://support.cluely.com/en/articles/12084603-how-to-copy-transcript), [App entry](https://app.cluely.com/) |
| `companion.browser.extension` | browser | No extension distribution link was found in the selected official navigation or targeted search. This is a bounded unknown, not proof no extension exists. |
| `sharing.summary-links` | desktop, mobile, sharing | Desktop guidance describes generating a public summary link; mobile guidance describes sharing a meeting note through messaging, email or a copied link. [Dashboard guide](https://support.cluely.com/en/articles/12101340-how-to-use-cluely-s-dashboard-app), [Mobile guide](https://support.cluely.com/en/articles/13008320-how-to-use-cluely-mobile) |
| `sync.account-records` | desktop, mobile, browser, sync | Official mobile marketing describes online notes and cross-device desktop synchronization. This establishes a documented product surface, not measured replication. [Mobile page](https://cluely.com/mobile), [App Store listing](https://apps.apple.com/us/app/cluely-ai-notetaker/id6755442775) |
| `sync.service-boundary` | mobile, browser, sync | Listing requires internet transcription and an account. Policy describes hosted processing and service providers. [App Store listing](https://apps.apple.com/us/app/cluely-ai-notetaker/id6755442775), [Privacy policy](https://cluely.com/privacy-policy) |

## Mobile capture boundary and conflicting dates

The mobile guide is dated 2026-02-06 but labels its capability list as
2025-12-05/v1. It excludes phone and mobile video-call capture and gives an older
iOS requirement. The current listing instead markets call use and requires
iOS 17+. Neither source proves how a current build acquires internal call audio.
No desktop screen-context relay to the phone is documented in the reviewed
pages. A microphone recording, a call made audible through a speaker, direct call
capture and synchronized desktop notes must remain separate test cases.
[Mobile guide](https://support.cluely.com/en/articles/13008320-how-to-use-cluely-mobile),
[App Store listing](https://apps.apple.com/us/app/cluely-ai-notetaker/id6755442775)

## Records and browser limits

Notes are explicitly advertised across desktop and mobile. The guide also says
mobile account deletion affects the desktop account, supporting a shared account
boundary. It does not specify which transcript, chat, file, mode or preference
fields replicate, or how conflicts are resolved.
[Mobile page](https://cluely.com/mobile),
[Mobile guide](https://support.cluely.com/en/articles/13008320-how-to-use-cluely-mobile)

Web meeting chat is explicit in the 2025-12-09 transcript article. Direct summary
editing is explicit in the 2025-10-25 desktop dashboard article; equivalent web
editing is unresolved. No authenticated browser view, geometry, browser capture
permission, extension package or current browser build was observed.
[Transcript guide](https://support.cluely.com/en/articles/12084603-how-to-copy-transcript),
[Dashboard guide](https://support.cluely.com/en/articles/12101340-how-to-use-cluely-s-dashboard-app)

## Publication, revocation and server limits

Publication is documented as a user-facing share action. The reviewed pages do
not establish the exact publication confirmation, link format, recipient access
rules, inclusion of transcripts/chat, expiration, update behavior or revocation.
Deleting a note or account must not be treated as evidence of link revocation.
No existing public note was retrieved and no link was created.

The current policy page is dated 2025-06-10. It describes uploaded customer data,
cloud/transcription providers and retention exceptions. These are policy claims,
not an audited storage map. The selected sources did not establish a sync-off
setting, local-only account mode, self-hosted server contract, replication API,
end-to-end key flow or guarantees that deletion immediately removes all copies.
[Privacy policy](https://cluely.com/privacy-policy)

The official status page names `app.cluely.com` and `platform.cluely.com`; host
listing does not identify which service stores or synchronizes each record.
Public fetches could not inspect the authenticated application, and the
`v2.cluely.com` fetch yielded no readable tool text. These are access/observation
limits, not feature absence.
[Service status](https://status.cluely.com/)

## Required next evidence

Pin an actual mobile package/device and authenticated browser build before
behavioral parity work. Use an original meeting fixture to compare record IDs
and fields across devices, then exercise edits, offline conflicts and deletion.
Create an explicitly disposable publication to measure access, changed content
and revocation. Verify any optional server or sync setting directly. Until those
checks exist, current implementation choices must not be reported as observed
reference contracts.
