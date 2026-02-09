use tokio::sync::mpsc;

/// Wraps a cross.stream store, providing engine configuration and topic-based script loading.
///
/// When the `cross-stream` feature is disabled, `Store` is an empty struct that is never
/// constructed. All methods exist as stubs so callers compile without `#[cfg]` annotations.
#[cfg(feature = "cross-stream")]
pub struct Store {
    inner: xs::store::Store,
    path: std::path::PathBuf,
}

#[cfg(not(feature = "cross-stream"))]
pub struct Store {
    _private: (), // prevent construction
}

// --- cross-stream implementation ---

#[cfg(feature = "cross-stream")]
impl Store {
    /// Create the store and spawn the API server and optional services.
    pub async fn init(path: std::path::PathBuf, services: bool, expose: Option<String>) -> Self {
        let inner = xs::store::Store::new(path.clone());

        // API server
        let store_for_api = inner.clone();
        tokio::spawn(async move {
            let engine = xs::nu::Engine::new().expect("Failed to create xs nu::Engine");
            if let Err(e) = xs::api::serve(store_for_api, engine, expose).await {
                eprintln!("Store API server error: {e}");
            }
        });

        // Services (handlers, generators, commands)
        if services {
            let store_for_handlers = inner.clone();
            tokio::spawn(async move {
                let engine = xs::nu::Engine::new().expect("Failed to create xs nu::Engine");
                if let Err(e) = xs::handlers::serve(store_for_handlers, engine).await {
                    eprintln!("Handlers serve error: {e}");
                }
            });

            let store_for_generators = inner.clone();
            tokio::spawn(async move {
                let engine = xs::nu::Engine::new().expect("Failed to create xs nu::Engine");
                if let Err(e) = xs::generators::serve(store_for_generators, engine).await {
                    eprintln!("Generators serve error: {e}");
                }
            });

            let store_for_commands = inner.clone();
            tokio::spawn(async move {
                let engine = xs::nu::Engine::new().expect("Failed to create xs nu::Engine");
                if let Err(e) = xs::commands::serve(store_for_commands, engine).await {
                    eprintln!("Commands serve error: {e}");
                }
            });
        }

        Self { inner, path }
    }

    /// Add store commands (.cat, .append, .cas, .last, etc.) to the engine.
    pub fn configure_engine(&self, engine: &mut crate::Engine) -> Result<(), crate::Error> {
        engine.add_store_commands(&self.inner)
    }

    /// Load initial script from a topic, send it through `tx`, and optionally watch for updates.
    ///
    /// Sends the initial script (or a placeholder if the topic is empty) through `tx`.
    /// If `watch` is true, spawns a background task that sends updated scripts through `tx`.
    /// If `watch` is false, `tx` is dropped after the initial send.
    pub async fn topic_source(&self, topic: &str, watch: bool, tx: mpsc::Sender<String>) {
        let store_path = self.path.display().to_string();

        let (initial, last_id) = match self.read_topic_content(topic) {
            Some((content, id)) => (content, Some(id)),
            None => (placeholder_closure(topic, &store_path), None),
        };

        tx.send(initial).await.expect("channel closed unexpectedly");

        if watch {
            spawn_topic_watcher(self.inner.clone(), topic.to_string(), last_id, tx);
        }
    }

    fn read_topic_content(&self, topic: &str) -> Option<(String, scru128::Scru128Id)> {
        let frame = self.inner.last(topic)?;
        let id = frame.id;
        let hash = frame.hash?;
        let bytes = self.inner.cas_read_sync(&hash).ok()?;
        let content = String::from_utf8(bytes).ok()?;
        Some((content, id))
    }
}

#[cfg(feature = "cross-stream")]
fn placeholder_closure(topic: &str, store_path: &str) -> String {
    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>http-nu - waiting for topic</title>
<style>
body {{ font-family: monospace; max-width: 640px; margin: 40px auto; padding: 0 20px; background: #1a1a2e; color: #c8c8d0; }}
h1 {{ color: #e0e0e0; font-size: 1.4em; }}
code {{ background: #16213e; padding: 2px 6px; border-radius: 3px; }}
pre {{ background: #16213e; padding: 16px; border-radius: 6px; overflow-x: auto; line-height: 1.5; }}
.waiting {{ color: #a78bfa; }}
</style>
</head>
<body>
<h1>http-nu</h1>
<p class="waiting">Waiting for topic <code>{topic}</code> ...</p>
<p>Append a handler closure to start serving:</p>
<pre>xs append {store_path}/sock {topic} &lt;&lt;'EOF'
{{|req|
  "hello, world"
}}
EOF</pre>
<p>With <code>-w</code>, the server will automatically reload when the topic is updated.</p>
</body>
</html>"#
    );

    let escaped = html.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "{{|req| \"{escaped}\" | metadata set --content-type text/html --merge {{'http.response': {{status: 503}}}} }}"
    )
}

#[cfg(feature = "cross-stream")]
fn spawn_topic_watcher(
    store: xs::store::Store,
    topic: String,
    after: Option<scru128::Scru128Id>,
    tx: mpsc::Sender<String>,
) {
    tokio::spawn(async move {
        let options = xs::store::ReadOptions::builder()
            .follow(xs::store::FollowOption::On)
            .topic(topic.clone())
            .maybe_after(after)
            .build();

        let mut receiver = store.read(options).await;

        while let Some(frame) = receiver.recv().await {
            if frame.topic != topic {
                continue;
            }

            let Some(hash) = frame.hash else {
                continue;
            };

            let content = match store.cas_read(&hash).await {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Error decoding topic content: {e}");
                        continue;
                    }
                },
                Err(e) => {
                    eprintln!("Error reading topic content: {e}");
                    continue;
                }
            };

            if tx.send(content).await.is_err() {
                break;
            }
        }
    });
}

// --- stubs when cross-stream is disabled ---

#[cfg(not(feature = "cross-stream"))]
impl Store {
    pub fn configure_engine(&self, _engine: &mut crate::Engine) -> Result<(), crate::Error> {
        unreachable!("Store is never constructed without cross-stream feature")
    }

    pub async fn topic_source(&self, _topic: &str, _watch: bool, _tx: mpsc::Sender<String>) {
        unreachable!("Store is never constructed without cross-stream feature")
    }
}
