use nu_protocol::Value;
use tokio::sync::broadcast;

#[derive(Clone, Debug)]
pub struct BusEvent {
    pub topic: String,
    pub value: Value,
}

pub struct Bus {
    sender: broadcast::Sender<BusEvent>,
}

impl Bus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { sender: tx }
    }

    pub fn publish(&self, topic: impl Into<String>, value: Value) {
        let _ = self.sender.send(BusEvent {
            topic: topic.into(),
            value,
        });
    }

    pub fn subscribe(&self, pattern: Option<String>) -> BusSubscription {
        BusSubscription {
            rx: self.sender.subscribe(),
            matcher: pattern.map(GlobMatcher::new),
        }
    }
}

pub struct BusSubscription {
    rx: broadcast::Receiver<BusEvent>,
    matcher: Option<GlobMatcher>,
}

impl BusSubscription {
    pub async fn recv(&mut self) -> Option<BusEvent> {
        loop {
            match self.rx.recv().await {
                Ok(ev) => {
                    if self.matches(&ev.topic) {
                        return Some(ev);
                    }
                }
                // On overflow we terminate the subscription rather than skip
                // events. UI state may be inconsistent if events were missed;
                // better to end the stream and let the client reconnect.
                Err(broadcast::error::RecvError::Lagged(_)) => return None,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }

    fn matches(&self, topic: &str) -> bool {
        match &self.matcher {
            None => true,
            Some(m) => m.matches(topic),
        }
    }
}

#[derive(Clone, Debug)]
pub struct GlobMatcher {
    parts: Vec<GlobPart>,
}

#[derive(Clone, Debug)]
enum GlobPart {
    Literal(String),
    Star,
}

impl GlobMatcher {
    pub fn new(pattern: impl Into<String>) -> Self {
        let pattern = pattern.into();
        let mut parts = Vec::new();
        let mut buf = String::new();
        for ch in pattern.chars() {
            if ch == '*' {
                if !buf.is_empty() {
                    parts.push(GlobPart::Literal(std::mem::take(&mut buf)));
                }
                if !matches!(parts.last(), Some(GlobPart::Star)) {
                    parts.push(GlobPart::Star);
                }
            } else {
                buf.push(ch);
            }
        }
        if !buf.is_empty() {
            parts.push(GlobPart::Literal(buf));
        }
        Self { parts }
    }

    pub fn matches(&self, s: &str) -> bool {
        match_parts(&self.parts, s)
    }
}

fn match_parts(parts: &[GlobPart], s: &str) -> bool {
    match parts.split_first() {
        None => s.is_empty(),
        Some((GlobPart::Literal(lit), rest)) => match s.strip_prefix(lit.as_str()) {
            Some(remainder) => match_parts(rest, remainder),
            None => false,
        },
        Some((GlobPart::Star, rest)) => {
            if rest.is_empty() {
                return true;
            }
            for (i, _) in s.char_indices() {
                if match_parts(rest, &s[i..]) {
                    return true;
                }
            }
            match_parts(rest, "")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nu_protocol::Span;

    fn v(s: &str) -> Value {
        Value::string(s, Span::test_data())
    }

    #[tokio::test]
    async fn sub_no_pattern_receives_everything() {
        let bus = Bus::new(64);
        let mut sub = bus.subscribe(None);
        bus.publish("a.foo", v("1"));
        bus.publish("b.bar", v("2"));
        let e1 = sub.recv().await.unwrap();
        let e2 = sub.recv().await.unwrap();
        assert_eq!(e1.topic, "a.foo");
        assert_eq!(e1.value.as_str().unwrap(), "1");
        assert_eq!(e2.topic, "b.bar");
        assert_eq!(e2.value.as_str().unwrap(), "2");
    }

    #[tokio::test]
    async fn sub_glob_filters_to_matching_topics() {
        let bus = Bus::new(64);
        let mut sub = bus.subscribe(Some("tab-abc.*".into()));
        bus.publish("tab-abc.compose.close", v("yes"));
        bus.publish("tab-xyz.compose.close", v("no"));
        bus.publish("tab-abc.editor.open", v("yes"));
        let e1 = sub.recv().await.unwrap();
        assert_eq!(e1.topic, "tab-abc.compose.close");
        let e2 = sub.recv().await.unwrap();
        assert_eq!(e2.topic, "tab-abc.editor.open");
    }

    #[tokio::test]
    async fn sub_terminates_on_lag() {
        let bus = Bus::new(2);
        let mut sub = bus.subscribe(None);
        for i in 0..10 {
            bus.publish("t", v(&i.to_string()));
        }
        assert!(sub.recv().await.is_none());
    }

    #[test]
    fn glob_star_matches_dotted_segments() {
        let m = GlobMatcher::new("a.*");
        assert!(m.matches("a.b"));
        assert!(m.matches("a.b.c"));
        assert!(m.matches("a."));
        assert!(!m.matches("a"));
        assert!(!m.matches("b.a"));
    }

    #[test]
    fn glob_bare_star_matches_anything() {
        let m = GlobMatcher::new("*");
        assert!(m.matches(""));
        assert!(m.matches("anything"));
        assert!(m.matches("with.dots.too"));
    }

    #[test]
    fn glob_literal_only_exact_match() {
        let m = GlobMatcher::new("exact.topic");
        assert!(m.matches("exact.topic"));
        assert!(!m.matches("exact.topic.suffix"));
        assert!(!m.matches("prefix.exact.topic"));
    }
}
