use std::fmt;

/// Errors raised while resolving a facade root.
///
/// The messages follow Laravel's wording where an equivalent exists.
///
/// # Examples
///
/// ```
/// # use std::sync::Arc;
/// # use laravel_facade::{Application, facade};
/// # use shaku::{Interface, module};
/// # #[facade(Cache)] pub trait CacheStore: Interface { fn get(&self, key: &str) -> Option<String>; }
/// # struct NullStore;
/// # impl<M: shaku::Module> shaku::Component<M> for NullStore { type Interface = dyn CacheStore; type Parameters = (); fn build(_: &mut shaku::ModuleBuildContext<M>, _: ()) -> Box<dyn CacheStore> { Box::new(NullStore) } }
/// # impl CacheStore for NullStore { fn get(&self, _: &str) -> Option<String> { None } }
/// # module! { AppModule { components = [NullStore], providers = [] } }
/// # { use shaku::HasComponent; let module = AppModule::builder().build(); assert_eq!(HasComponent::<dyn CacheStore>::resolve(&module).get(""), None); }
/// use laravel_facade::Error;
///
/// let err = Cache::try_get_facade_root().err().unwrap();
/// assert_eq!(err, Error::FacadeRootNotSet);
/// assert_eq!(err.to_string(), "A facade root has not been set.");
/// # laravel_facade::clear_resolved_instances(); Cache::swap(Arc::new(NullStore)); assert_eq!(Cache::get(""), None);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// No application was set with [`set_facade_application`](crate::set_facade_application),
    /// and nothing was swapped in.
    FacadeRootNotSet,
    /// The application has no binding for the accessor.
    NotInstantiable {
        /// Type name of the accessor, such as `dyn app::CacheStore`.
        accessor: &'static str,
    },
    /// A shaku provider returned an error.
    Provider {
        /// Type name of the accessor.
        accessor: &'static str,
        /// The provider error, rendered with `Display`.
        /// shaku provider errors are not `Send`, so only the message is kept.
        message: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FacadeRootNotSet => f.write_str("A facade root has not been set."),
            Self::NotInstantiable { accessor } => write!(f, "Target [{accessor}] is not instantiable."),
            Self::Provider { accessor, message } => write!(f, "Unable to provide [{accessor}]: {message}"),
        }
    }
}

impl std::error::Error for Error {}
