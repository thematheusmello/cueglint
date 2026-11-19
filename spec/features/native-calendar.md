# Native Calendar setup and upcoming events

The pinned reference showed a connected Calendar account with an account-management
row in Settings. History showed an upcoming section with a refresh control, date
tiles and a show-more action. First authorization, failed connections, reminder
scheduling and the empty-account state remain unobserved.
[Reference observations](reference-ui.md)

Cueglint's Calendar settings accept a personal Google Desktop OAuth client ID.
An explicit check reads the existing connection from this device. Connect or
Reconnect opens Google authorization in the default browser. The bounded worker
uses a random localhost port, PKCE and state acknowledgment. Refresh fetches
one page of up to 50 owned events on the primary calendar for the next 30 days.
The desktop asks for the documented read-only events scope. The documented
Desktop exchange and refresh are available with a client ID.
[Google Desktop authorization](https://developers.google.com/identity/protocols/oauth2/native-app) ·
[Google event listing](https://developers.google.com/workspace/calendar/api/v3/reference/events/list)

The configured client ID and scoped refresh token use this OS user's native
credential store. Access tokens and event pages stay in memory. Restart and restore
clear the event page and require an explicit refresh. Calendar event data stays
outside Assistant context, logs, session storage and backups. On Windows the
browser launch uses `ShellExecuteW` in its own initialized COM apartment.
[Windows launcher documentation](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecutew)

A transient refresh failure can leave an identified stale page. Invalid grants
require reconnection. Disconnect reports Google revocation and local credential
removal separately. If revocation fails, the user may retry it or explicitly remove
the saved authorization from this device. Restore and cancellation retire pending
job results before publishing new state.

History shows three events after the first page has loaded. **Show more** reveals
the rest of that page. Timed rows show the calendar date and time from each
endpoint's returned offset. UTC endpoints show UTC. A range crossing midnight
or a daylight-saving change displays both dates, times and offsets. All-day
row dates retain their exclusive end. Additional pages or providers, a verified
live account, OS vault prompts and a matched reference run remain open.
[Google Calendar time-zone behavior](https://developers.google.com/workspace/calendar/api/concepts/events-calendars#time_zones)

The local worker cases cover refresh rotation, explicit reopen, stale data,
expired grants, repeated disconnect and stale result retirement. The headless
app cases exercise the explicit buttons, show-more limit and date labels. They
use controlled data and do not launch a browser or authorize an account.

## In-app meeting reminders

The Calendar settings opt-in is off by default. Reminders appear in the visible
Cueglint window during the five minutes before a timed event. The scheduler
uses the event's UTC instant from the latest successful Calendar page. The
scheduler retains the provider's fractional timestamp as elapsed UTC duration
since the Unix epoch. It converts the current platform wall clock at the input
boundary. The banner displays the offset supplied by Calendar. All-day events
have no start instant for this purpose. Reminders never fetch events or start a session on
their own. Load the Calendar page explicitly after restart or reconnection.

The banner stays actionable until the event starts, the user dismisses it,
the current page removes or moves the event, or the page becomes stale or
disconnected. On wake the schedule reconciles against current UTC. It skips
any events that already started. A changed start instant is a new occurrence.
The opt-in and a bounded ledger of up to 256 shown occurrence digests use the
existing device-local UI preferences. Normal saves and shutdown retain the
ledger across restart. Event IDs, titles, dates and Calendar data stay out of
the logical backup. A new authorization receives its own opaque grant scope.
Local reads recognize existing saved refresh tokens, and the next successful
refresh stores their scope alongside the token. Token rotation keeps that
scope. The worker publishes it only with the corresponding event page.

**Start** passes the event's display title to the existing meeting start
action. The selected mode, capture settings and permission handling follow
that ordinary lifecycle. Start waits for active recording and Highlights work
to finish. An immediate submission refusal keeps the current reminder available.
After a successful queue submission or a deferred notes save, the ordinary
meeting action handles later errors. A Calendar refresh cancellation or backup
review temporarily hides the banner. The app revalidates the page and due time
before showing it again.

Background OS notification delivery, native sleep/wake observation and live
Google account verification remain open.
