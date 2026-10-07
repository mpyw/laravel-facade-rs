use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use laravel_facade::{Application, Error, Facade, Fake, extends_facade};
use mockall::automock;
use mockall::predicate::eq;
use serial_test::serial;
use shaku::{Component, Interface, Provider, module};

#[extends_facade(Cache)]
#[automock]
pub trait CacheStore: Interface {
    /// Gets a value.
    fn get(&self, key: &str) -> Option<String>;
    fn put(&self, key: &str, value: String);
}

/// Covers the receiver and signature shapes that mockall cannot mock.
#[extends_facade(Strings)]
pub trait StringTools: Interface {
    // Eliding 'a would tie the result to `self`, which a facade cannot forward.
    #[allow(clippy::needless_lifetimes)]
    fn pick<'a>(&self, keys: &'a [String]) -> &'a str;
    fn shared(self: Arc<Self>) -> usize;
    fn generic<T: ToString>(&self, _value: T)
    where
        Self: Sized,
    {
    }
    fn constructor() -> Self
    where
        Self: Sized;
}

#[derive(Component)]
#[shaku(interface = StringTools)]
struct StringToolsImpl;

impl StringTools for StringToolsImpl {
    fn pick<'a>(&self, keys: &'a [String]) -> &'a str {
        &keys[0]
    }
    fn shared(self: Arc<Self>) -> usize {
        Arc::strong_count(&self)
    }
    fn constructor() -> Self {
        Self
    }
}

#[derive(Component, Default)]
#[shaku(interface = CacheStore)]
struct ArrayStore {
    #[shaku(default)]
    items: Mutex<HashMap<String, String>>,
}

impl CacheStore for ArrayStore {
    fn get(&self, key: &str) -> Option<String> {
        self.items.lock().unwrap().get(key).cloned()
    }
    fn put(&self, key: &str, value: String) {
        self.items.lock().unwrap().insert(key.to_owned(), value);
    }
}

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

#[extends_facade(Uuid, cached = false)]
pub trait IdGenerator: Interface {
    fn id(&self) -> usize;
}

struct CountingGenerator {
    id: usize,
}

impl<M: shaku::Module> Provider<M> for CountingGenerator {
    type Interface = dyn IdGenerator;

    fn provide(_: &M) -> Result<Box<dyn IdGenerator>, Box<dyn std::error::Error>> {
        Ok(Box::new(CountingGenerator {
            id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
        }))
    }
}

impl IdGenerator for CountingGenerator {
    fn id(&self) -> usize {
        self.id
    }
}

#[extends_facade(Http)]
#[async_trait]
pub trait HttpClient: Interface {
    async fn get(&self, url: &str) -> String;
}

#[derive(Component)]
#[shaku(interface = HttpClient)]
struct FakeHttp;

#[async_trait]
impl HttpClient for FakeHttp {
    async fn get(&self, url: &str) -> String {
        format!("GET {url}")
    }
}

#[extends_facade(Unbound)]
pub trait NotBound: Interface {
    fn nothing(&self);
}

module! {
    AppModule {
        components = [ArrayStore, FakeHttp, StringToolsImpl],
        providers = [CountingGenerator]
    }
}

fn boot() -> Arc<Application> {
    Facade::clear_resolved_instances();
    let app = Application::builder(AppModule::builder().build())
        .bind::<dyn CacheStore>()
        .bind::<dyn HttpClient>()
        .bind::<dyn StringTools>()
        .bind_provider::<dyn IdGenerator>()
        .build();
    Facade::set_facade_application(Arc::clone(&app));
    app
}

fn teardown() {
    Facade::clear_resolved_instances();
    Facade::set_facade_application(None);
}

#[test]
#[serial]
fn forwards_static_calls_to_the_component() {
    boot();

    Cache::put("foo", "bar".into());
    assert_eq!(Cache::get("foo"), Some("bar".into()));
    assert_eq!(Strings::pick(&["a".into(), "b".into()]), "a");
    // One in the module, one cached by the facade, one passed as `self`.
    assert_eq!(Strings::shared(), 3);

    teardown();
}

#[test]
#[serial]
fn panics_without_an_application() {
    teardown();

    assert_eq!(Cache::try_get_facade_root().err(), Some(Error::FacadeRootNotSet));
    let panic = std::panic::catch_unwind(|| Cache::get("foo")).unwrap_err();
    assert_eq!(
        panic.downcast_ref::<String>().unwrap(),
        "A facade root has not been set."
    );
}

#[test]
#[serial]
fn reports_an_unbound_accessor() {
    boot();

    assert_eq!(
        Unbound::try_get_facade_root().err().unwrap().to_string(),
        "Target [dyn facade::NotBound] is not instantiable.",
    );

    teardown();
}

#[test]
#[serial]
fn caches_the_resolved_instance() {
    let app = boot();

    let a = Cache::get_facade_root();
    let b = Cache::get_facade_root();
    assert!(Arc::ptr_eq(&a, &b));
    assert!(app.resolved::<dyn CacheStore>());

    teardown();
}

#[test]
#[serial]
fn does_not_cache_when_cached_is_false() {
    let base = NEXT_ID.load(Ordering::SeqCst);
    boot();

    assert_eq!(Uuid::id(), base);
    assert_eq!(Uuid::id(), base + 1);

    teardown();
}

#[test]
#[serial]
fn swaps_the_instance_in_the_facade_and_the_application() {
    let app = boot();

    let store = Arc::new(ArrayStore::default());
    store.put("swapped", "yes".into());
    Cache::swap(store);

    assert_eq!(Cache::get("swapped"), Some("yes".into()));
    assert_eq!(app.make::<dyn CacheStore>().unwrap().get("swapped"), Some("yes".into()));
    assert!(!Cache::is_fake());

    Cache::clear_resolved_instance();
    app.forget_instance::<dyn CacheStore>();
    assert_eq!(Cache::get("swapped"), None);

    teardown();
}

#[derive(Default)]
struct CacheFake {
    puts: Mutex<Vec<String>>,
}

impl Fake for CacheFake {}

impl CacheStore for CacheFake {
    fn get(&self, _: &str) -> Option<String> {
        None
    }
    fn put(&self, key: &str, _: String) {
        self.puts.lock().unwrap().push(key.to_owned());
    }
}

#[test]
#[serial]
fn fakes_the_facade() {
    boot();

    let fake = Cache::fake(CacheFake::default());
    Cache::put("a", "1".into());
    Cache::put("b", "2".into());

    assert!(Cache::is_fake());
    assert_eq!(*fake.puts.lock().unwrap(), ["a", "b"]);

    teardown();
}

#[test]
#[serial]
fn adds_expectations_to_the_same_mock() {
    boot();

    Cache::should_receive(|mock: &mut MockCacheStore| {
        mock.expect_get()
            .with(eq("foo"))
            .times(1)
            .return_const(Some("mocked".to_owned()));
    });
    Cache::expects(|mock: &mut MockCacheStore| {
        mock.expect_put()
            .with(eq("bar"), eq("baz".to_owned()))
            .times(1)
            .return_const(());
    });

    assert_eq!(Cache::get("foo"), Some("mocked".into()));
    Cache::put("bar", "baz".into());
    assert!(!Cache::is_fake());

    teardown();
}

#[test]
#[serial]
fn verifies_mock_expectations_on_teardown() {
    boot();

    Cache::should_receive(|mock: &mut MockCacheStore| {
        mock.expect_get().times(1).return_const(None);
    });

    let panic = std::panic::catch_unwind(teardown).unwrap_err();
    let message = panic.downcast_ref::<String>().unwrap();
    assert!(message.contains("MockCacheStore::get"), "{message}");

    teardown();
}

#[test]
#[serial]
fn runs_resolved_callbacks() {
    boot();
    let calls = Arc::new(AtomicUsize::new(0));

    let counter = Arc::clone(&calls);
    Cache::resolved(move |cache, _app| {
        cache.put("seen", "1".into());
        counter.fetch_add(1, Ordering::SeqCst);
    });
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    assert_eq!(Cache::get("seen"), Some("1".into()));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Already resolved: runs right away.
    let counter = Arc::clone(&calls);
    Cache::resolved(move |_, _| {
        counter.fetch_add(10, Ordering::SeqCst);
    });
    assert_eq!(calls.load(Ordering::SeqCst), 11);

    teardown();
}

#[test]
#[serial]
fn forwards_async_trait_methods() {
    boot();

    assert_eq!(
        pollster::block_on(Http::get("https://example.com")),
        "GET https://example.com"
    );

    teardown();
}

#[test]
#[serial]
fn exposes_the_application() {
    let app = boot();

    assert!(Arc::ptr_eq(&Facade::get_facade_application().unwrap(), &app));
    assert!(app.bound::<dyn CacheStore>());
    assert!(!app.bound::<dyn NotBound>());
    assert_eq!(Cache::get_facade_accessor(), "dyn facade::CacheStore");

    teardown();
}

#[test]
#[serial]
fn reports_provider_errors() {
    Facade::clear_resolved_instances();
    let module = AppModule::builder()
        .with_provider_override::<dyn IdGenerator>(Box::new(|_| Err("database is down".into())))
        .build();
    Facade::set_facade_application(Application::builder(module).bind_provider::<dyn IdGenerator>().build());

    let error = Uuid::try_get_facade_root().err().unwrap();
    assert_eq!(
        error,
        Error::Provider {
            accessor: "dyn facade::IdGenerator",
            message: "database is down".into(),
        },
    );
    assert_eq!(
        error.to_string(),
        "Unable to provide [dyn facade::IdGenerator]: database is down"
    );

    teardown();
}

#[test]
#[serial]
fn refuses_expectations_while_the_mock_is_held() {
    boot();

    Cache::should_receive(|mock: &mut MockCacheStore| {
        mock.expect_get().return_const(None);
    });
    let held = Cache::get_facade_root();

    let panic = std::panic::catch_unwind(|| {
        Cache::should_receive(|mock: &mut MockCacheStore| {
            mock.expect_put().return_const(());
        });
    })
    .unwrap_err();
    assert_eq!(
        panic.downcast_ref::<String>().unwrap(),
        "laravel-facade: cannot add expectations to the [dyn facade::CacheStore] mock while its root is still held elsewhere",
    );

    drop(held);
    teardown();
}

#[test]
#[serial]
fn replaces_a_mock_of_another_type() {
    boot();

    Cache::should_receive(|mock: &mut MockCacheStore| {
        mock.expect_get().return_const(Some("first".to_owned()));
    });
    Cache::should_receive(|fake: &mut ArrayStore| fake.put("key", "second".into()));

    assert_eq!(Cache::get("key"), Some("second".into()));

    teardown();
}

#[test]
#[serial]
fn formats_with_debug() {
    let builder = Application::builder(AppModule::builder().build()).bind::<dyn CacheStore>();
    assert_eq!(
        format!("{builder:?}"),
        "ApplicationBuilder { module: \"facade::AppModule\", bindings: 1 }",
    );

    let app = builder.build();
    app.make::<dyn CacheStore>().unwrap();
    assert_eq!(
        format!("{app:?}"),
        "Application { bindings: 1, instances: 0, resolved: 1, .. }",
    );
    assert_eq!(format!("{:?}", Cache), "Cache");
}

#[test]
#[serial]
fn panics_on_resolved_without_an_application() {
    teardown();

    let panic = std::panic::catch_unwind(|| Cache::resolved(|_, _| {})).unwrap_err();
    assert_eq!(
        panic.downcast_ref::<String>().unwrap(),
        "A facade root has not been set."
    );
}
