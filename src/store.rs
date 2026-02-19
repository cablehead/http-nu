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
    pub async fn init(
        path: std::path::PathBuf,
        services: bool,
        expose: Option<String>,
    ) -> Result<Self, xs::store::StoreError> {
        let inner = xs::store::Store::new(path.clone())?;

        // API server
        let store_for_api = inner.clone();
        tokio::spawn(async move {
            let engine = xs::nu::Engine::new().expect("Failed to create xs nu::Engine");
            if let Err(e) = xs::api::serve(store_for_api, engine, expose).await {
                eprintln!("Store API server error: {e}");
            }
        });

        // Processors (actor, service, action) -- each gets its own subscription
        if services {
            let s = inner.clone();
            tokio::spawn(async move {
                if let Err(e) = xs::processor::actor::run(s).await {
                    eprintln!("Actor processor error: {e}");
                }
            });

            let s = inner.clone();
            tokio::spawn(async move {
                if let Err(e) = xs::processor::service::run(s).await {
                    eprintln!("Service processor error: {e}");
                }
            });

            let s = inner.clone();
            tokio::spawn(async move {
                if let Err(e) = xs::processor::action::run(s).await {
                    eprintln!("Action processor error: {e}");
                }
            });
        }

        Ok(Self { inner, path })
    }

    /// Add store commands (.cat, .append, .cas, .last, etc.) to the engine.
    pub fn configure_engine(&self, engine: &mut crate::Engine) -> Result<(), crate::Error> {
        engine.add_store_commands(&self.inner)?;
        engine.add_store_mj_commands(&self.inner)
    }

    /// Load a handler closure from a store topic, enrich with VFS modules from
    /// the stream, and send the resulting engine through `tx`.
    ///
    /// If `watch` is true, spawns a background task that reloads on topic updates.
    pub async fn topic_source(
        &self,
        topic: &str,
        watch: bool,
        base_engine: crate::Engine,
        tx: mpsc::Sender<crate::Engine>,
    ) {
        let store_path = self.path.display().to_string();

        let (initial_script, last_id) = match self.read_topic_content(topic) {
            Some((content, id)) => (content, Some(id)),
            None => (placeholder_closure(topic, &store_path), None),
        };

        let enriched = enrich_engine(&base_engine, &self.inner, last_id.as_ref());
        if let Some(engine) = crate::engine::script_to_engine(&enriched, &initial_script, None) {
            tx.send(engine).await.expect("channel closed unexpectedly");
        }

        if watch {
            spawn_topic_watcher(
                self.inner.clone(),
                topic.to_string(),
                last_id,
                base_engine,
                tx,
            );
        }
    }

    fn read_topic_content(&self, topic: &str) -> Option<(String, scru128::Scru128Id)> {
        let options = xs::store::ReadOptions::builder()
            .follow(xs::store::FollowOption::Off)
            .topic(topic.to_string())
            .last(1_usize)
            .build();
        let frame = self.inner.read_sync(options).last()?;
        let id = frame.id;
        let hash = frame.hash?;
        let bytes = self.inner.cas_read_sync(&hash).ok()?;
        let content = String::from_utf8(bytes).ok()?;
        Some((content, id))
    }
}

/// Clone the base engine and load VFS modules from the stream.
#[cfg(feature = "cross-stream")]
fn enrich_engine(
    base: &crate::Engine,
    store: &xs::store::Store,
    as_of: Option<&scru128::Scru128Id>,
) -> crate::Engine {
    let mut engine = base.clone();
    if let Some(id) = as_of {
        let modules = store.nu_modules_at(id);
        if let Err(e) = xs::nu::load_modules(&mut engine.state, store, &modules) {
            eprintln!("Error loading stream modules: {e}");
        }
    }
    engine
}

#[cfg(feature = "cross-stream")]
fn placeholder_closure(topic: &str, store_path: &str) -> String {
    include_str!("../examples/topic-placeholder.nu")
        .replace("__TOPIC__", topic)
        .replace("__STORE_PATH__", store_path)
}

#[cfg(feature = "cross-stream")]
fn spawn_topic_watcher(
    store: xs::store::Store,
    topic: String,
    after: Option<scru128::Scru128Id>,
    base_engine: crate::Engine,
    tx: mpsc::Sender<crate::Engine>,
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

            let script = match store.cas_read(&hash).await {
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

            let enriched = enrich_engine(&base_engine, &store, Some(&frame.id));
            if let Some(engine) = crate::engine::script_to_engine(&enriched, &script, None) {
                if tx.send(engine).await.is_err() {
                    break;
                }
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

    pub async fn topic_source(
        &self,
        _topic: &str,
        _watch: bool,
        _base_engine: crate::Engine,
        _tx: mpsc::Sender<crate::Engine>,
    ) {
        unreachable!("Store is never constructed without cross-stream feature")
    }
}
