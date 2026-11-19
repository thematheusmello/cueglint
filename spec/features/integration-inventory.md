# Desktop integration inventory

The pinned macOS 2.1.28 account showed a connected Calendar settings row and
upcoming events in History. The recorded session covered an already authorized
account. Provider choice, calendar toggles, initial consent, account failure,
alerts and briefs still need a controlled scenario. [Observed reference](reference-ui.md)

The public help pages retrieved on November 19, 2026 describe workflows dated
2025. Treat their claims as discovery until the pinned build is exercised.

| Surface | Documented workflow | Cueglint coverage |
| --- | --- | --- |
| Calendar setup | Connect Google Calendar, manage connected calendars, choose which calendars appear and refresh events. The September 2025 guide lists Google Calendar for direct linking. | Primary-calendar OAuth, one bounded upcoming page, refresh, revoke, local disconnect and timed/all-day rows. Multi-calendar selection, remote pagination and live-account testing remain open. [Cluely settings help](https://support.cluely.com/en/articles/12418681-how-to-use-cluely-s-settings) |
| Meeting alerts | The linked calendar triggers a prompt to start a session. | An opt-in Cueglint banner uses the current page and an explicit Start action. OS notification delivery, native sleep/wake and live account verification remain open. [Cluely dashboard help](https://support.cluely.com/en/articles/12101340-how-to-use-cluely-s-dashboard-app) |
| Pre-call briefs | The dashboard guide describes calendar-backed briefs using upcoming meeting context and research about participants. | Invitation data scope, the entry point and outputs need observation. Core behavior can use intentionally selected Calendar and local context. Enabling outside research requires a separate provider choice and source attribution. [Cluely dashboard help](https://support.cluely.com/en/articles/12101340-how-to-use-cluely-s-dashboard-app) |
| Conferencing compatibility | The public help describes capture behavior in Zoom, Meet, Teams, Webex and Slack Huddle under particular versions and sharing routes. | Evaluate these in the platform capture matrix using controlled recipient-side results. [Cluely compatibility help](https://support.cluely.com/en/articles/12364416-what-apps-are-supported-for-undetectability) |

Keep calendar accounts, conferencing compatibility, publishing links and sync
tracked through their existing contracts. Additional integrations can enter the
inventory when a current first-party source or pinned observation establishes
an actual user workflow.
