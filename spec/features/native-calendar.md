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
the rest of that page. Timed events show their UTC date and time. All-day events
retain the calendar dates and exclusive end. Calendar-local dates and reminders,
additional pages or providers, a verified live account, OS vault prompts and a
matched reference run remain open.

The local worker cases cover refresh rotation, explicit reopen, stale data,
expired grants, repeated disconnect and stale result retirement. The headless
app cases exercise the explicit buttons, show-more limit and date labels. They
use controlled data and do not launch a browser or authorize an account.
