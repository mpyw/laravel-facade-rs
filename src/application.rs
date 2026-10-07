use std::any::{Any, TypeId, type_name};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, RwLock};

use shaku::{HasComponent, HasProvider, Interface, ModuleInterface};

use crate::Error;
use crate::sync::{read, write};

/// Type-erased value. Holds `Arc<I>`, `Resolver<I>` or `Callback<I>` for some `I`.
pub(crate) type Erased = Box<dyn Any + Send + Sync>;

type Resolver<I> = Box<dyn Fn() -> Result<Arc<I>, Error> + Send + Sync>;
type Callback<I> = Arc<dyn Fn(&Arc<I>, &Application) + Send + Sync>;

pub(crate) fn erase<I: ?Sized + Interface>(instance: Arc<I>) -> Erased {
    Box::new(instance)
}

pub(crate) fn unerase<I: ?Sized + Interface>(erased: &Erased) -> Arc<I> {
    Arc::clone(
        erased
            .downcast_ref::<Arc<I>>()
            .expect("laravel-facade: erased value has an unexpected type"),
    )
}

/// The container behind facades.
///
/// shaku modules are concrete types and cannot be looked up by interface at
/// runtime. `Application` fills that gap: it keeps one resolver per bound
/// interface, keyed by `TypeId`. This plays the role of Laravel's string keys.
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
/// let app = Application::builder(AppModule::builder().build())
///     .bind::<dyn CacheStore>()
///     .build();
///
/// laravel_facade::Facade::set_facade_application(app);
/// assert_eq!(Cache::get("key"), None);
/// ```
pub struct Application {
    bindings: BTreeMap<TypeId, Erased>,
    instances: RwLock<BTreeMap<TypeId, Erased>>,
    resolved: RwLock<BTreeSet<TypeId>>,
    after_resolving: RwLock<BTreeMap<TypeId, Vec<Erased>>>,
}

impl Application {
    /// Starts building an application around a shaku module.
    pub fn builder<M: ModuleInterface>(module: M) -> ApplicationBuilder<M> {
        ApplicationBuilder {
            module: Arc::new(module),
            bindings: BTreeMap::new(),
        }
    }

    /// Returns `true` if `I` has a binding or an instance. Same as `$app->bound()`.
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
    /// assert!(app.bound::<dyn CacheStore>());
    /// # laravel_facade::Facade::clear_resolved_instances(); Cache::swap(Arc::new(NullStore)); assert_eq!(Cache::get(""), None);
    /// ```
    pub fn bound<I: ?Sized + Interface>(&self) -> bool {
        let id = TypeId::of::<I>();
        self.bindings.contains_key(&id) || read(&self.instances).contains_key(&id)
    }

    /// Returns `true` if `I` was resolved or has an instance. Same as `$app->resolved()`.
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
    /// assert!(!app.resolved::<dyn CacheStore>());
    /// app.make::<dyn CacheStore>().unwrap();
    /// assert!(app.resolved::<dyn CacheStore>());
    /// # laravel_facade::Facade::clear_resolved_instances(); Cache::swap(Arc::new(NullStore)); assert_eq!(Cache::get(""), None);
    /// ```
    pub fn resolved<I: ?Sized + Interface>(&self) -> bool {
        let id = TypeId::of::<I>();
        read(&self.resolved).contains(&id) || read(&self.instances).contains_key(&id)
    }

    /// Resolves `I`. Same as `$app->make()`.
    ///
    /// An instance registered with [`instance`](Self::instance) wins over the
    /// binding, and does not fire `after_resolving` callbacks (as in Laravel).
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
    /// let cache = app.make::<dyn CacheStore>().unwrap();
    /// assert_eq!(cache.get("key"), None);
    /// # laravel_facade::Facade::clear_resolved_instances(); Cache::swap(Arc::new(NullStore)); assert_eq!(Cache::get(""), None);
    /// ```
    pub fn make<I: ?Sized + Interface>(&self) -> Result<Arc<I>, Error> {
        let id = TypeId::of::<I>();

        if let Some(instance) = read(&self.instances).get(&id) {
            return Ok(unerase(instance));
        }

        let resolver = self
            .bindings
            .get(&id)
            .and_then(|erased| erased.downcast_ref::<Resolver<I>>())
            .ok_or(Error::NotInstantiable {
                accessor: type_name::<I>(),
            })?;
        let instance = resolver()?;

        write(&self.resolved).insert(id);

        // Clone the callbacks out so they can call back into facades.
        let callbacks: Vec<Callback<I>> = read(&self.after_resolving)
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(|erased| erased.downcast_ref::<Callback<I>>().cloned())
            .collect();
        for callback in callbacks {
            callback(&instance, self);
        }

        Ok(instance)
    }

    /// Registers an existing instance as `I`. Same as `$app->instance()`.
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
    /// app.instance::<dyn CacheStore>(Arc::new(HitStore));
    /// assert_eq!(app.make::<dyn CacheStore>().unwrap().get("key").as_deref(), Some("hit"));
    ///
    /// app.forget_instance::<dyn CacheStore>();
    /// assert_eq!(app.make::<dyn CacheStore>().unwrap().get("key"), None);
    /// # laravel_facade::Facade::clear_resolved_instances(); Cache::swap(Arc::new(NullStore)); assert_eq!(Cache::get(""), None);
    /// ```
    pub fn instance<I: ?Sized + Interface>(&self, instance: Arc<I>) -> Arc<I> {
        let previous = write(&self.instances).insert(TypeId::of::<I>(), erase(Arc::clone(&instance)));
        drop(previous); // outside the lock: dropping a mock may panic
        instance
    }

    /// Removes an instance registered as `I`. Same as `$app->forgetInstance()`.
    pub fn forget_instance<I: ?Sized + Interface>(&self) {
        let previous = write(&self.instances).remove(&TypeId::of::<I>());
        drop(previous);
    }

    /// Registers a callback that runs each time `I` is built. Same as `$app->afterResolving()`.
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
    /// use std::sync::atomic::{AtomicUsize, Ordering};
    ///
    /// static BUILT: AtomicUsize = AtomicUsize::new(0);
    /// app.after_resolving::<dyn CacheStore>(|_cache, _app| {
    ///     BUILT.fetch_add(1, Ordering::SeqCst);
    /// });
    ///
    /// app.make::<dyn CacheStore>().unwrap();
    /// assert_eq!(BUILT.load(Ordering::SeqCst), 1);
    /// # laravel_facade::Facade::clear_resolved_instances(); Cache::swap(Arc::new(NullStore)); assert_eq!(Cache::get(""), None);
    /// ```
    pub fn after_resolving<I: ?Sized + Interface>(
        &self,
        callback: impl Fn(&Arc<I>, &Application) + Send + Sync + 'static,
    ) {
        let callback: Callback<I> = Arc::new(callback);
        write(&self.after_resolving)
            .entry(TypeId::of::<I>())
            .or_default()
            .push(Box::new(callback));
    }
}

impl fmt::Debug for Application {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Application")
            .field("bindings", &self.bindings.len())
            .field("instances", &read(&self.instances).len())
            .field("resolved", &read(&self.resolved).len())
            .finish_non_exhaustive()
    }
}

/// Builder returned by [`Application::builder`].
pub struct ApplicationBuilder<M> {
    module: Arc<M>,
    bindings: BTreeMap<TypeId, Erased>,
}

impl<M: ModuleInterface> ApplicationBuilder<M> {
    /// Binds a shaku component. Every resolution returns the same shared instance.
    pub fn bind<I: ?Sized + Interface>(self) -> Self
    where
        M: HasComponent<I>,
    {
        let module = Arc::clone(&self.module);
        self.insert::<I>(Box::new(move || Ok(HasComponent::<I>::resolve(&*module))))
    }

    /// Binds a shaku provider. Every resolution builds a new instance.
    ///
    /// Facades cache the first instance by default. Set `cached = false` on
    /// `#[extends_facade]` to get a fresh one on each call.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use std::sync::atomic::{AtomicUsize, Ordering};
    ///
    /// use laravel_facade::{Application, extends_facade};
    /// use shaku::{Interface, Module, Provider, module};
    ///
    /// #[extends_facade(Uuid, cached = false)]
    /// pub trait IdGenerator: Interface {
    ///     fn id(&self) -> usize;
    /// }
    ///
    /// struct Counter(usize);
    ///
    /// impl IdGenerator for Counter {
    ///     fn id(&self) -> usize {
    ///         self.0
    ///     }
    /// }
    ///
    /// impl<M: Module> Provider<M> for Counter {
    ///     type Interface = dyn IdGenerator;
    ///
    ///     fn provide(_: &M) -> Result<Box<dyn IdGenerator>, Box<dyn std::error::Error>> {
    ///         static NEXT: AtomicUsize = AtomicUsize::new(1);
    ///         Ok(Box::new(Counter(NEXT.fetch_add(1, Ordering::SeqCst))))
    ///     }
    /// }
    ///
    /// module! {
    ///     AppModule {
    ///         components = [],
    ///         providers = [Counter]
    ///     }
    /// }
    ///
    /// let app = Application::builder(AppModule::builder().build())
    ///     .bind_provider::<dyn IdGenerator>()
    ///     .build();
    /// laravel_facade::Facade::set_facade_application(app);
    ///
    /// assert_eq!(Uuid::id(), 1);
    /// assert_eq!(Uuid::id(), 2);
    /// ```
    pub fn bind_provider<I: ?Sized + Interface>(self) -> Self
    where
        M: HasProvider<I>,
    {
        let module = Arc::clone(&self.module);
        self.insert::<I>(Box::new(move || {
            HasProvider::<I>::provide(&*module)
                .map(Arc::from)
                .map_err(|e| Error::Provider {
                    accessor: type_name::<I>(),
                    message: e.to_string(),
                })
        }))
    }

    /// Finishes the application.
    pub fn build(self) -> Arc<Application> {
        Arc::new(Application {
            bindings: self.bindings,
            instances: RwLock::default(),
            resolved: RwLock::default(),
            after_resolving: RwLock::default(),
        })
    }

    fn insert<I: ?Sized + Interface>(mut self, resolver: Resolver<I>) -> Self {
        self.bindings.insert(TypeId::of::<I>(), Box::new(resolver));
        self
    }
}

impl<M> fmt::Debug for ApplicationBuilder<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApplicationBuilder")
            .field("module", &type_name::<M>())
            .field("bindings", &self.bindings.len())
            .finish()
    }
}
