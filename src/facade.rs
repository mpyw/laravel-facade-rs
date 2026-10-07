use std::any::{TypeId, type_name};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use shaku::Interface;

use crate::application::{Erased, erase, unerase};
use crate::sync::{read, write};
use crate::{Application, Error};

/// `static::$app`
static APP: RwLock<Option<Arc<Application>>> = RwLock::new(None);

/// `static::$resolvedInstance`
static RESOLVED_INSTANCES: RwLock<BTreeMap<TypeId, Resolved>> = RwLock::new(BTreeMap::new());

struct Resolved {
    /// `Arc<F::Accessor>`
    instance: Erased,
    kind: Kind,
}

enum Kind {
    Plain,
    /// Set by `fake()`. `instanceof Fake` in Laravel.
    Fake,
    /// Set by `should_receive()`. `instanceof LegacyMockInterface` in Laravel.
    /// Holds the concrete `Arc<M>` so that expectations can be added later.
    Mock(Erased),
}

/// Marker for test doubles. Same as `Illuminate\Support\Testing\Fakes\Fake`.
///
/// A type swapped in with the generated `fake()` method must implement it,
/// and then `is_fake()` returns `true`.
///
/// # Examples
///
/// ```
/// # use std::sync::Arc;
/// # use laravel_facade::{Application, extends_facade};
/// # use shaku::{Interface, module};
/// # #[extends_facade(Cache)] pub trait CacheStore: Interface { fn get(&self, key: &str) -> Option<String>; }
/// # struct NullStore;
/// # impl<M: shaku::Module> shaku::Component<M> for NullStore { type Interface = dyn CacheStore; type Parameters = (); fn build(_: &mut shaku::ModuleBuildContext<M>, _: ()) -> Box<dyn CacheStore> { Box::new(NullStore) } }
/// # impl CacheStore for NullStore { fn get(&self, _: &str) -> Option<String> { None } }
/// # module! { AppModule { components = [NullStore], providers = [] } }
/// # { use shaku::HasComponent; let module = AppModule::builder().build(); assert_eq!(HasComponent::<dyn CacheStore>::resolve(&module).get(""), None); }
/// use laravel_facade::Fake;
///
/// struct CacheFake;
/// impl Fake for CacheFake {}
/// impl CacheStore for CacheFake {
///     fn get(&self, _: &str) -> Option<String> {
///         Some("fake".into())
///     }
/// }
///
/// let fake = Cache::fake(CacheFake);
/// assert!(Cache::is_fake());
/// assert_eq!(Cache::get("key").as_deref(), Some("fake"));
/// # drop(fake);
/// ```
pub trait Fake {}

/// The base of all facades. Same as `Illuminate\Support\Facades\Facade`.
///
/// It holds the static methods that Laravel calls on the base class, such as
/// `Facade::setFacadeApplication()`. It has no values, like an abstract class.
///
/// Each facade struct implements [`ExtendsFacade`] instead.
#[allow(missing_debug_implementations)]
pub enum Facade {}

impl Facade {
    /// Sets the application behind all facades. Same as `Facade::setFacadeApplication()`.
    ///
    /// Pass `None` to unset it. The old application is dropped after the lock is
    /// released, so mock expectations held by it are checked here.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use laravel_facade::{Application, extends_facade};
    /// # use shaku::{Interface, module};
    /// # #[extends_facade(Cache)] pub trait CacheStore: Interface { fn get(&self, key: &str) -> Option<String>; }
    /// # struct NullStore;
    /// # impl<M: shaku::Module> shaku::Component<M> for NullStore { type Interface = dyn CacheStore; type Parameters = (); fn build(_: &mut shaku::ModuleBuildContext<M>, _: ()) -> Box<dyn CacheStore> { Box::new(NullStore) } }
    /// # impl CacheStore for NullStore { fn get(&self, _: &str) -> Option<String> { None } }
    /// # module! { AppModule { components = [NullStore], providers = [] } }
    /// # { use shaku::HasComponent; let module = AppModule::builder().build(); assert_eq!(HasComponent::<dyn CacheStore>::resolve(&module).get(""), None); }
    /// # let app = Application::builder(AppModule::builder().build()).bind::<dyn CacheStore>().build();
    /// laravel_facade::Facade::set_facade_application(Arc::clone(&app));
    /// assert!(laravel_facade::Facade::get_facade_application().is_some());
    ///
    /// laravel_facade::Facade::set_facade_application(None);
    /// assert!(laravel_facade::Facade::get_facade_application().is_none());
    /// # laravel_facade::Facade::clear_resolved_instances(); Cache::swap(Arc::new(NullStore)); assert_eq!(Cache::get(""), None);
    /// ```
    pub fn set_facade_application(app: impl Into<Option<Arc<Application>>>) {
        let previous = std::mem::replace(&mut *write(&APP), app.into());
        drop(previous);
    }

    /// Returns the application behind all facades. Same as `Facade::getFacadeApplication()`.
    pub fn get_facade_application() -> Option<Arc<Application>> {
        read(&APP).clone()
    }

    /// Clears all resolved instances. Same as `Facade::clearResolvedInstances()`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use laravel_facade::{Application, extends_facade};
    /// # use shaku::{Interface, module};
    /// # #[extends_facade(Cache)] pub trait CacheStore: Interface { fn get(&self, key: &str) -> Option<String>; }
    /// # struct NullStore;
    /// # impl<M: shaku::Module> shaku::Component<M> for NullStore { type Interface = dyn CacheStore; type Parameters = (); fn build(_: &mut shaku::ModuleBuildContext<M>, _: ()) -> Box<dyn CacheStore> { Box::new(NullStore) } }
    /// # impl CacheStore for NullStore { fn get(&self, _: &str) -> Option<String> { None } }
    /// # module! { AppModule { components = [NullStore], providers = [] } }
    /// # { use shaku::HasComponent; let module = AppModule::builder().build(); assert_eq!(HasComponent::<dyn CacheStore>::resolve(&module).get(""), None); }
    /// # let app = Application::builder(AppModule::builder().build()).bind::<dyn CacheStore>().build();
    /// # laravel_facade::Facade::set_facade_application(Arc::clone(&app));
    /// let first = Cache::get_facade_root();
    /// laravel_facade::Facade::clear_resolved_instances();
    /// // The component is a singleton in the module, so it is the same instance.
    /// assert!(Arc::ptr_eq(&first, &Cache::get_facade_root()));
    /// # laravel_facade::Facade::clear_resolved_instances(); Cache::swap(Arc::new(NullStore)); assert_eq!(Cache::get(""), None);
    /// ```
    pub fn clear_resolved_instances() {
        let previous = std::mem::take(&mut *write(&RESOLVED_INSTANCES));
        drop(previous);
    }

    /// Clears the cached instance of one accessor. Same as `Facade::clearResolvedInstance($name)`.
    ///
    /// [`ExtendsFacade::clear_resolved_instance`] calls this with its own accessor.
    pub fn clear_resolved_instance<I: ?Sized + Interface>() {
        let previous = write(&RESOLVED_INSTANCES).remove(&TypeId::of::<I>());
        drop(previous);
    }
}

/// A facade: a static proxy to a shaku interface. Same as `class Cache extends Facade`.
///
/// You do not implement this by hand. `#[extends_facade(Name)]` on the interface
/// trait generates the struct, this impl, and the static methods.
///
/// Each provided method here is also generated as an inherent method on the
/// facade struct, so `Cache::swap(..)` works without importing this trait.
///
/// # Examples
///
/// ```
/// # use std::sync::Arc;
/// # use laravel_facade::{Application, extends_facade};
/// # use shaku::{Interface, module};
/// # #[extends_facade(Cache)] pub trait CacheStore: Interface { fn get(&self, key: &str) -> Option<String>; }
/// # struct NullStore;
/// # impl<M: shaku::Module> shaku::Component<M> for NullStore { type Interface = dyn CacheStore; type Parameters = (); fn build(_: &mut shaku::ModuleBuildContext<M>, _: ()) -> Box<dyn CacheStore> { Box::new(NullStore) } }
/// # impl CacheStore for NullStore { fn get(&self, _: &str) -> Option<String> { None } }
/// # module! { AppModule { components = [NullStore], providers = [] } }
/// # { use shaku::HasComponent; let module = AppModule::builder().build(); assert_eq!(HasComponent::<dyn CacheStore>::resolve(&module).get(""), None); }
/// use laravel_facade::ExtendsFacade;
///
/// // Generic code can take any facade.
/// fn accessor_of<F: ExtendsFacade>() -> &'static str {
///     F::get_facade_accessor()
/// }
///
/// assert!(accessor_of::<Cache>().ends_with("CacheStore"));
/// # laravel_facade::Facade::clear_resolved_instances(); Cache::swap(Arc::new(NullStore)); assert_eq!(Cache::get(""), None);
/// ```
pub trait ExtendsFacade {
    /// The interface behind the facade. Same as `getFacadeAccessor()`.
    type Accessor: ?Sized + Interface;

    /// Whether the resolved instance is cached. Same as `static::$cached`.
    const CACHED: bool = true;

    /// Returns the accessor name, for messages.
    fn get_facade_accessor() -> &'static str {
        type_name::<Self::Accessor>()
    }

    /// Returns the instance behind the facade. Same as `getFacadeRoot()`.
    ///
    /// # Panics
    ///
    /// Panics with "A facade root has not been set." when there is no
    /// application, like `__callStatic` throws `RuntimeException`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use laravel_facade::{Application, extends_facade};
    /// # use shaku::{Interface, module};
    /// # #[extends_facade(Cache)] pub trait CacheStore: Interface { fn get(&self, key: &str) -> Option<String>; }
    /// # struct NullStore;
    /// # impl<M: shaku::Module> shaku::Component<M> for NullStore { type Interface = dyn CacheStore; type Parameters = (); fn build(_: &mut shaku::ModuleBuildContext<M>, _: ()) -> Box<dyn CacheStore> { Box::new(NullStore) } }
    /// # impl CacheStore for NullStore { fn get(&self, _: &str) -> Option<String> { None } }
    /// # module! { AppModule { components = [NullStore], providers = [] } }
    /// # { use shaku::HasComponent; let module = AppModule::builder().build(); assert_eq!(HasComponent::<dyn CacheStore>::resolve(&module).get(""), None); }
    /// # let app = Application::builder(AppModule::builder().build()).bind::<dyn CacheStore>().build();
    /// # laravel_facade::Facade::set_facade_application(Arc::clone(&app));
    /// let cache = Cache::get_facade_root();
    /// assert_eq!(cache.get("key"), None);
    /// # laravel_facade::Facade::clear_resolved_instances(); Cache::swap(Arc::new(NullStore)); assert_eq!(Cache::get(""), None);
    /// ```
    fn get_facade_root() -> Arc<Self::Accessor> {
        Self::try_get_facade_root().unwrap_or_else(|e| panic!("{e}"))
    }

    /// Non-panicking version of [`get_facade_root`](Self::get_facade_root).
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use laravel_facade::{Application, extends_facade};
    /// # use shaku::{Interface, module};
    /// # #[extends_facade(Cache)] pub trait CacheStore: Interface { fn get(&self, key: &str) -> Option<String>; }
    /// # struct NullStore;
    /// # impl<M: shaku::Module> shaku::Component<M> for NullStore { type Interface = dyn CacheStore; type Parameters = (); fn build(_: &mut shaku::ModuleBuildContext<M>, _: ()) -> Box<dyn CacheStore> { Box::new(NullStore) } }
    /// # impl CacheStore for NullStore { fn get(&self, _: &str) -> Option<String> { None } }
    /// # module! { AppModule { components = [NullStore], providers = [] } }
    /// # { use shaku::HasComponent; let module = AppModule::builder().build(); assert_eq!(HasComponent::<dyn CacheStore>::resolve(&module).get(""), None); }
    /// use laravel_facade::Error;
    ///
    /// assert_eq!(Cache::try_get_facade_root().err(), Some(Error::FacadeRootNotSet));
    /// # laravel_facade::Facade::clear_resolved_instances(); Cache::swap(Arc::new(NullStore)); assert_eq!(Cache::get(""), None);
    /// ```
    fn try_get_facade_root() -> Result<Arc<Self::Accessor>, Error> {
        resolve_facade_instance::<Self::Accessor>(Self::CACHED)
    }

    /// Hot-swaps the instance behind the facade. Same as `swap()`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use laravel_facade::{Application, extends_facade};
    /// # use shaku::{Interface, module};
    /// # #[extends_facade(Cache)] pub trait CacheStore: Interface { fn get(&self, key: &str) -> Option<String>; }
    /// # struct NullStore;
    /// # impl<M: shaku::Module> shaku::Component<M> for NullStore { type Interface = dyn CacheStore; type Parameters = (); fn build(_: &mut shaku::ModuleBuildContext<M>, _: ()) -> Box<dyn CacheStore> { Box::new(NullStore) } }
    /// # impl CacheStore for NullStore { fn get(&self, _: &str) -> Option<String> { None } }
    /// # module! { AppModule { components = [NullStore], providers = [] } }
    /// # { use shaku::HasComponent; let module = AppModule::builder().build(); assert_eq!(HasComponent::<dyn CacheStore>::resolve(&module).get(""), None); }
    /// # let app = Application::builder(AppModule::builder().build()).bind::<dyn CacheStore>().build();
    /// struct HitStore;
    /// impl CacheStore for HitStore {
    ///     fn get(&self, _: &str) -> Option<String> {
    ///         Some("hit".into())
    ///     }
    /// }
    ///
    /// Cache::swap(Arc::new(HitStore));
    /// assert_eq!(Cache::get("key").as_deref(), Some("hit"));
    /// assert!(!Cache::is_fake());
    /// ```
    fn swap(instance: Arc<Self::Accessor>) {
        swap_with_kind(instance, Kind::Plain);
    }

    /// Returns `true` if a fake was swapped in with `fake()`. Same as `isFake()`.
    fn is_fake() -> bool {
        matches!(
            read(&RESOLVED_INSTANCES).get(&TypeId::of::<Self::Accessor>()),
            Some(Resolved { kind: Kind::Fake, .. })
        )
    }

    /// Clears the cached instance of this facade. Same as `clearResolvedInstance()`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use laravel_facade::{Application, extends_facade};
    /// # use shaku::{Interface, module};
    /// # #[extends_facade(Cache)] pub trait CacheStore: Interface { fn get(&self, key: &str) -> Option<String>; }
    /// # struct NullStore;
    /// # impl<M: shaku::Module> shaku::Component<M> for NullStore { type Interface = dyn CacheStore; type Parameters = (); fn build(_: &mut shaku::ModuleBuildContext<M>, _: ()) -> Box<dyn CacheStore> { Box::new(NullStore) } }
    /// # impl CacheStore for NullStore { fn get(&self, _: &str) -> Option<String> { None } }
    /// # module! { AppModule { components = [NullStore], providers = [] } }
    /// # { use shaku::HasComponent; let module = AppModule::builder().build(); assert_eq!(HasComponent::<dyn CacheStore>::resolve(&module).get(""), None); }
    /// # let app = Application::builder(AppModule::builder().build()).bind::<dyn CacheStore>().build();
    /// # laravel_facade::Facade::set_facade_application(Arc::clone(&app));
    /// struct HitStore;
    /// impl CacheStore for HitStore {
    ///     fn get(&self, _: &str) -> Option<String> {
    ///         Some("hit".into())
    ///     }
    /// }
    ///
    /// Cache::swap(Arc::new(HitStore));
    /// assert_eq!(Cache::get("key").as_deref(), Some("hit"));
    ///
    /// Cache::clear_resolved_instance();
    /// app.forget_instance::<dyn CacheStore>();
    /// assert_eq!(Cache::get("key"), None);
    /// ```
    fn clear_resolved_instance() {
        Facade::clear_resolved_instance::<Self::Accessor>();
    }

    /// Runs a callback when the facade root is resolved. Same as `resolved()`.
    ///
    /// If it was already resolved, the callback runs right away too.
    ///
    /// # Panics
    ///
    /// Panics if no application is set.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use laravel_facade::{Application, extends_facade};
    /// # use shaku::{Interface, module};
    /// # #[extends_facade(Cache)] pub trait CacheStore: Interface { fn get(&self, key: &str) -> Option<String>; }
    /// # struct NullStore;
    /// # impl<M: shaku::Module> shaku::Component<M> for NullStore { type Interface = dyn CacheStore; type Parameters = (); fn build(_: &mut shaku::ModuleBuildContext<M>, _: ()) -> Box<dyn CacheStore> { Box::new(NullStore) } }
    /// # impl CacheStore for NullStore { fn get(&self, _: &str) -> Option<String> { None } }
    /// # module! { AppModule { components = [NullStore], providers = [] } }
    /// # { use shaku::HasComponent; let module = AppModule::builder().build(); assert_eq!(HasComponent::<dyn CacheStore>::resolve(&module).get(""), None); }
    /// # let app = Application::builder(AppModule::builder().build()).bind::<dyn CacheStore>().build();
    /// # laravel_facade::Facade::set_facade_application(Arc::clone(&app));
    /// Cache::resolved(|cache, _app| {
    ///     println!("resolved: {:?}", cache.get("key"));
    /// });
    /// Cache::get("key"); // prints "resolved: None"
    /// ```
    fn resolved(callback: impl Fn(&Arc<Self::Accessor>, &Application) + Send + Sync + 'static) {
        let app = Facade::get_facade_application().unwrap_or_else(|| panic!("{}", Error::FacadeRootNotSet));
        let callback = Arc::new(callback);

        if app.resolved::<Self::Accessor>() {
            callback(&Self::get_facade_root(), &app);
        }

        app.after_resolving::<Self::Accessor>(move |service, app| callback(service, app));
    }
}

fn resolve_facade_instance<I: ?Sized + Interface>(cached: bool) -> Result<Arc<I>, Error> {
    let id = TypeId::of::<I>();

    if let Some(resolved) = read(&RESOLVED_INSTANCES).get(&id) {
        return Ok(unerase(&resolved.instance));
    }

    let app = Facade::get_facade_application().ok_or(Error::FacadeRootNotSet)?;
    let instance = app.make::<I>()?;

    if !cached {
        return Ok(instance);
    }

    // Another thread may have resolved or swapped it meanwhile; keep theirs.
    let mut resolved = write(&RESOLVED_INSTANCES);
    let entry = resolved.entry(id).or_insert_with(|| Resolved {
        instance: erase(instance),
        kind: Kind::Plain,
    });
    Ok(unerase(&entry.instance))
}

fn swap_with_kind<I: ?Sized + Interface>(instance: Arc<I>, kind: Kind) {
    let previous = write(&RESOLVED_INSTANCES).insert(
        TypeId::of::<I>(),
        Resolved {
            instance: erase(Arc::clone(&instance)),
            kind,
        },
    );
    drop(previous);

    if let Some(app) = Facade::get_facade_application() {
        app.instance(instance);
    }
}

/// Support code for `#[extends_facade]`. Not public API.
#[doc(hidden)]
pub mod __private {
    use super::*;

    pub fn fake<F: ExtendsFacade + ?Sized>(instance: Arc<F::Accessor>) {
        swap_with_kind(instance, Kind::Fake);
    }

    pub fn should_receive<F, M, R>(upcast: fn(Arc<M>) -> Arc<F::Accessor>, expect: impl FnOnce(&mut M) -> R) -> R
    where
        F: ExtendsFacade + ?Sized,
        M: Interface + Default,
    {
        let id = TypeId::of::<F::Accessor>();

        // isMock(): reuse the current mock if it has the same type.
        let current = {
            let mut resolved = write(&RESOLVED_INSTANCES);
            match resolved.get(&id) {
                Some(Resolved {
                    kind: Kind::Mock(mock), ..
                }) if mock.is::<Arc<M>>() => resolved.remove(&id),
                _ => None,
            }
        };
        let mut mock = match current {
            Some(Resolved {
                instance,
                kind: Kind::Mock(mock),
            }) => {
                drop(instance);
                *mock.downcast::<Arc<M>>().expect("checked above")
            }
            _ => Arc::new(M::default()),
        };

        // swap() also stored a clone in the application.
        if let Some(app) = Facade::get_facade_application() {
            app.forget_instance::<F::Accessor>();
        }

        let mock_mut = Arc::get_mut(&mut mock).unwrap_or_else(|| {
            panic!(
                "laravel-facade: cannot add expectations to the [{}] mock while its root is still held elsewhere",
                type_name::<F::Accessor>(),
            )
        });
        let result = expect(mock_mut);

        swap_with_kind(upcast(Arc::clone(&mock)), Kind::Mock(Box::new(mock)));
        result
    }
}
