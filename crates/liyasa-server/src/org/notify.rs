//! Notification routing (ORG-21).
//!
//! Two things are worth stating because getting either wrong produces a
//! product that reports success while being wrong:
//!
//! * A channel a user asked for that the organization has not configured an
//!   endpoint for produces a **skip with a reason**, not a delivery to nowhere
//!   and not a silent drop. [`Routed::skipped`] is part of the return value
//!   rather than a log line.
//! * A per-project preference **replaces** the user's default for that
//!   project, the same way ORG-02's project role replaces the organization
//!   role. Turning a noisy project down would otherwise be impossible.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// What is being announced (ORG-21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    Deployment,
    VerificationDrift,
    ReviewRequest,
    Comment,
    AutomationRun,
    UsageAlert,
}

impl Event {
    pub const ALL: &'static [Event] = &[
        Event::Deployment,
        Event::VerificationDrift,
        Event::ReviewRequest,
        Event::Comment,
        Event::AutomationRun,
        Event::UsageAlert,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Event::Deployment => "deployment",
            Event::VerificationDrift => "verificationDrift",
            Event::ReviewRequest => "reviewRequest",
            Event::Comment => "comment",
            Event::AutomationRun => "automationRun",
            Event::UsageAlert => "usageAlert",
        }
    }

    pub fn parse(text: &str) -> Option<Event> {
        Event::ALL
            .iter()
            .copied()
            .find(|e| e.as_str().eq_ignore_ascii_case(text))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Channel {
    Email,
    Slack,
    Teams,
    Discord,
    Webhook,
}

impl Channel {
    pub const ALL: &'static [Channel] = &[
        Channel::Email,
        Channel::Slack,
        Channel::Teams,
        Channel::Discord,
        Channel::Webhook,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Email => "email",
            Channel::Slack => "slack",
            Channel::Teams => "teams",
            Channel::Discord => "discord",
            Channel::Webhook => "webhook",
        }
    }

    pub fn parse(text: &str) -> Option<Channel> {
        Channel::ALL
            .iter()
            .copied()
            .find(|c| c.as_str().eq_ignore_ascii_case(text))
    }

    /// Email goes to the member's own address; every other channel needs an
    /// endpoint the organization configured.
    pub fn needs_endpoint(self) -> bool {
        self != Channel::Email
    }
}

/// One user's choices (ORG-21). The default applies to every project; a
/// per-project entry replaces it for that project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    default: BTreeMap<Event, BTreeSet<Channel>>,
    per_project: BTreeMap<(String, Event), BTreeSet<Channel>>,
}

impl Preferences {
    /// Everything by email, which is what a member who has never opened the
    /// preferences page should get.
    pub fn standard() -> Self {
        let mut preferences = Preferences::default();
        for event in Event::ALL {
            preferences.set(*event, [Channel::Email]);
        }
        preferences
    }

    pub fn set(&mut self, event: Event, channels: impl IntoIterator<Item = Channel>) {
        self.default.insert(event, channels.into_iter().collect());
    }

    pub fn set_for_project(
        &mut self,
        project: &str,
        event: Event,
        channels: impl IntoIterator<Item = Channel>,
    ) {
        self.per_project
            .insert((project.to_owned(), event), channels.into_iter().collect());
    }

    pub fn clear_project(&mut self, project: &str, event: Event) {
        self.per_project.remove(&(project.to_owned(), event));
    }

    /// The only way to read a preference, so no caller can consult the default
    /// and forget the override.
    pub fn channels(&self, project: Option<&str>, event: Event) -> BTreeSet<Channel> {
        if let Some(project) = project
            && let Some(channels) = self.per_project.get(&(project.to_owned(), event))
        {
            return channels.clone();
        }
        self.default.get(&event).cloned().unwrap_or_default()
    }

    pub fn overrides(&self) -> impl Iterator<Item = (&str, Event, &BTreeSet<Channel>)> {
        self.per_project
            .iter()
            .map(|((project, event), channels)| (project.as_str(), *event, channels))
    }
}

/// Where a non-email channel sends. An organization configures at most one
/// endpoint per channel.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoints {
    endpoints: BTreeMap<Channel, String>,
}

impl Endpoints {
    pub fn set(&mut self, channel: Channel, target: impl Into<String>) {
        self.endpoints.insert(channel, target.into());
    }

    pub fn get(&self, channel: Channel) -> Option<&str> {
        self.endpoints.get(&channel).map(String::as_str)
    }

    pub fn configured(&self) -> impl Iterator<Item = (Channel, &str)> {
        self.endpoints.iter().map(|(c, t)| (*c, t.as_str()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscriber {
    pub user: String,
    pub email: String,
    pub preferences: Preferences,
}

impl Subscriber {
    pub fn new(user: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            user: user.into(),
            email: email.into(),
            preferences: Preferences::standard(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    pub user: String,
    pub channel: Channel,
    pub event: Event,
    pub project: Option<String>,
    /// The address or URL this goes to.
    pub target: String,
    pub subject: String,
}

/// A channel somebody asked for that nothing will be sent on, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skipped {
    pub user: String,
    pub channel: Channel,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Routed {
    pub deliveries: Vec<Delivery>,
    /// Present whenever a preference could not be honoured. An empty vector is
    /// the claim that every subscriber who asked to hear about this will.
    pub skipped: Vec<Skipped>,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub event: Event,
    pub project: Option<String>,
    pub subject: String,
}

impl Notification {
    pub fn new(event: Event, subject: impl Into<String>) -> Self {
        Self {
            event,
            project: None,
            subject: subject.into(),
        }
    }

    pub fn in_project(mut self, project: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self
    }
}

/// Why an email delivery was skipped. `concat!` rather than a wrapped
/// literal: a Rust line continuation inside this string was silently
/// collapsed into a run of spaces once already, and the only thing that
/// noticed was a human reading the file.
pub const NO_MAIL_SENDER: &str = concat!(
    "no mail sender is available on this instance; ",
    "if a `mail` block is configured, its startup diagnostic says why it could not be used"
);

/// Whether this instance can send email at all.
///
/// Not part of [`Endpoints`] on purpose. The sender lives on `AuthState`,
/// built from the site's `mail` block, and a copy of "do we have one" stored
/// beside the organization's own endpoints would be a second source of truth
/// that drifts the first time mail is reconfigured. The caller reads the live
/// one and passes it in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mailer {
    Configured,
    Absent,
}

/// ORG-21, in one function.
pub fn route(
    subscribers: &[Subscriber],
    endpoints: &Endpoints,
    mailer: Mailer,
    notification: &Notification,
) -> Routed {
    let mut routed = Routed::default();
    for subscriber in subscribers {
        let channels = subscriber
            .preferences
            .channels(notification.project.as_deref(), notification.event);
        for channel in channels {
            let target = if channel.needs_endpoint() {
                match endpoints.get(channel) {
                    Some(target) => target.to_owned(),
                    None => {
                        routed.skipped.push(Skipped {
                            user: subscriber.user.clone(),
                            channel,
                            reason: format!(
                                "this organization has no `{}` endpoint configured",
                                channel.as_str()
                            ),
                        });
                        continue;
                    }
                }
            } else if mailer == Mailer::Absent {
                // Email needs no per-organization endpoint, which used to mean
                // it could never be skipped — so an instance that cannot send
                // produced a delivery to an address nothing would ever send
                // to. A reported skip and a silent drop are the same outcome
                // for the reader and opposite outcomes for the operator.
                //
                // "Available", not "configured": two different states reach
                // here identically. A site with no `mail` block has no sender,
                // and a block that exists and cannot work raises E0816 at
                // startup and also leaves the sender unset. Naming the block
                // would be wrong in the second case, and the startup
                // diagnostic is where the difference is recorded.
                routed.skipped.push(Skipped {
                    user: subscriber.user.clone(),
                    channel,
                    reason: NO_MAIL_SENDER.to_owned(),
                });
                continue;
            } else {
                subscriber.email.clone()
            };
            routed.deliveries.push(Delivery {
                user: subscriber.user.clone(),
                channel,
                event: notification.event,
                project: notification.project.clone(),
                target,
                subject: notification.subject.clone(),
            });
        }
    }
    routed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoints() -> Endpoints {
        let mut endpoints = Endpoints::default();
        endpoints.set(Channel::Slack, "https://hooks.slack.com/services/T/B/X");
        endpoints
    }

    #[test]
    fn a_member_who_never_opened_the_preferences_page_is_emailed_about_everything() {
        let subscribers = [Subscriber::new("u1", "ana@acme.com")];
        for event in Event::ALL {
            let routed = route(
                &subscribers,
                &Endpoints::default(),
                Mailer::Configured,
                &Notification::new(*event, "something happened"),
            );
            assert_eq!(routed.deliveries.len(), 1, "{}", event.as_str());
            assert_eq!(routed.deliveries[0].target, "ana@acme.com");
            assert!(routed.skipped.is_empty());
        }
    }

    #[test]
    fn a_per_project_preference_replaces_the_default_for_that_project() {
        // ORG-21's "per-user and per-project". Replacing rather than adding is
        // what makes it possible to turn one noisy project down.
        let mut subscriber = Subscriber::new("u1", "ana@acme.com");
        subscriber
            .preferences
            .set(Event::Deployment, [Channel::Email, Channel::Slack]);
        subscriber
            .preferences
            .set_for_project("noisy", Event::Deployment, []);

        let quiet = route(
            &[subscriber.clone()],
            &endpoints(),
            Mailer::Configured,
            &Notification::new(Event::Deployment, "deployed").in_project("noisy"),
        );
        assert!(quiet.deliveries.is_empty());
        assert!(quiet.skipped.is_empty(), "nobody asked, so nothing is owed");

        let loud = route(
            &[subscriber],
            &endpoints(),
            Mailer::Configured,
            &Notification::new(Event::Deployment, "deployed").in_project("docs"),
        );
        assert_eq!(loud.deliveries.len(), 2);
    }

    #[test]
    fn an_unconfigured_channel_is_reported_rather_than_delivered_to_nowhere() {
        // The defect this is about: a preference for Teams with no Teams
        // endpoint, routed as a success, is a notification nobody receives and
        // nobody knows was lost.
        let mut subscriber = Subscriber::new("u1", "ana@acme.com");
        subscriber
            .preferences
            .set(Event::UsageAlert, [Channel::Teams, Channel::Email]);
        let routed = route(
            &[subscriber],
            &endpoints(),
            Mailer::Configured,
            &Notification::new(Event::UsageAlert, "80% of the credit pool"),
        );
        assert_eq!(routed.deliveries.len(), 1);
        assert_eq!(routed.deliveries[0].channel, Channel::Email);
        assert_eq!(routed.skipped.len(), 1);
        assert_eq!(routed.skipped[0].channel, Channel::Teams);
        assert!(
            routed.skipped[0].reason.contains("teams"),
            "{}",
            routed.skipped[0].reason
        );
    }

    #[test]
    fn a_site_with_no_mail_sender_reports_the_skip_rather_than_dropping_it() {
        // ORG-21 delivers by email by default, and email needs no
        // per-organization endpoint — which used to mean it could never be
        // skipped. On a site with no `mail` block that produced a `Delivery`
        // to an address nothing would ever send to: a silent drop wearing a
        // success.
        let subscribers = [Subscriber::new("u1", "ana@acme.com")];
        let routed = route(
            &subscribers,
            &Endpoints::default(),
            Mailer::Absent,
            &Notification::new(Event::Deployment, "deployed"),
        );
        assert!(routed.deliveries.is_empty());
        assert_eq!(routed.skipped.len(), 1);
        assert_eq!(routed.skipped[0].channel, Channel::Email);
        let reason = &routed.skipped[0].reason;
        assert_eq!(reason, NO_MAIL_SENDER);
        assert!(
            !reason.contains("  "),
            "a wrapped literal collapsed into spaces once already: {reason:?}"
        );
        assert!(
            !reason.contains("has no `mail` block"),
            "two states reach here identically, no block and a block that raised E0816, \
             so the reason must not claim which: {reason}"
        );

        // The same instance still delivers on a channel that has an endpoint,
        // so an absent mailer stops email and nothing else.
        let mut subscriber = Subscriber::new("u1", "ana@acme.com");
        subscriber
            .preferences
            .set(Event::Deployment, [Channel::Email, Channel::Slack]);
        let routed = route(
            &[subscriber],
            &endpoints(),
            Mailer::Absent,
            &Notification::new(Event::Deployment, "deployed"),
        );
        assert_eq!(routed.deliveries.len(), 1);
        assert_eq!(routed.deliveries[0].channel, Channel::Slack);
        assert_eq!(routed.skipped.len(), 1);
    }

    #[test]
    fn email_needs_no_endpoint_and_every_other_channel_does() {
        assert!(!Channel::Email.needs_endpoint());
        for channel in Channel::ALL.iter().filter(|c| **c != Channel::Email) {
            assert!(channel.needs_endpoint(), "{}", channel.as_str());
        }
    }

    #[test]
    fn each_subscriber_is_routed_on_their_own_preferences() {
        let mut quiet = Subscriber::new("u1", "ana@acme.com");
        quiet.preferences.set(Event::Comment, []);
        let loud = Subscriber::new("u2", "bo@acme.com");
        let routed = route(
            &[quiet, loud],
            &endpoints(),
            Mailer::Configured,
            &Notification::new(Event::Comment, "a comment"),
        );
        assert_eq!(routed.deliveries.len(), 1);
        assert_eq!(routed.deliveries[0].user, "u2");
    }

    #[test]
    fn the_six_events_and_five_channels_org_21_names_round_trip() {
        assert_eq!(Event::ALL.len(), 6);
        assert_eq!(Channel::ALL.len(), 5);
        for event in Event::ALL {
            assert_eq!(Event::parse(event.as_str()), Some(*event));
        }
        for channel in Channel::ALL {
            assert_eq!(Channel::parse(channel.as_str()), Some(*channel));
        }
        assert_eq!(Channel::parse("SLACK"), Some(Channel::Slack));
        assert_eq!(Channel::parse("sms"), None);
    }

    #[test]
    fn clearing_a_project_override_restores_the_default() {
        let mut preferences = Preferences::standard();
        preferences.set_for_project("noisy", Event::Deployment, []);
        assert!(
            preferences
                .channels(Some("noisy"), Event::Deployment)
                .is_empty()
        );
        preferences.clear_project("noisy", Event::Deployment);
        assert_eq!(
            preferences.channels(Some("noisy"), Event::Deployment),
            [Channel::Email].into()
        );
    }
}
