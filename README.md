# laravel-facade

[![CI](https://github.com/mpyw/laravel-facade-rs/actions/workflows/test.yaml/badge.svg)](https://github.com/mpyw/laravel-facade-rs/actions/workflows/test.yaml)
[![codecov](https://codecov.io/gh/mpyw/laravel-facade-rs/graph/badge.svg)](https://codecov.io/gh/mpyw/laravel-facade-rs)
[![crates.io](https://img.shields.io/crates/v/laravel-facade.svg)](https://crates.io/crates/laravel-facade)
[![docs.rs](https://docs.rs/laravel-facade/badge.svg)](https://docs.rs/laravel-facade)

Laravel facades for Rust, on top of the [shaku](https://docs.rs/shaku) DI container.

This is a joke crate. It still follows
[`Illuminate\Support\Facades\Facade`](https://github.com/illuminate/support/blob/master/Facades/Facade.php)
as closely as Rust allows.

In Laravel, you write this:

```php
use Illuminate\Support\Facades\Cache;

Cache::put('framework', 'Laravel');
Cache::get('framework'); // 'Laravel'
```

With laravel-facade, you write this:

```rust
use std::collections::HashMap;
use std::sync::Mutex;

use laravel_facade::{Application, Facade, extends_facade};
use shaku::{Component, Interface, module};

// class Cache extends Facade
#[extends_facade(Cache)]
pub trait CacheStore: Interface {
    fn get(&self, key: &str) -> Option<String>;
    fn put(&self, key: &str, value: &str);
}

#[derive(Component)]
#[shaku(interface = CacheStore)]
struct ArrayStore {
    #[shaku(default)]
    items: Mutex<HashMap<String, String>>,
}

impl CacheStore for ArrayStore {
    fn get(&self, key: &str) -> Option<String> {
        self.items.lock().unwrap().get(key).cloned()
    }

    fn put(&self, key: &str, value: &str) {
        self.items.lock().unwrap().insert(key.into(), value.into());
    }
}

module! {
    AppModule {
        components = [ArrayStore],
        providers = []
    }
}

fn main() {
    let app = Application::builder(AppModule::builder().build())
        .bind::<dyn CacheStore>()
        .build();
    Facade::set_facade_application(app);

    Cache::put("framework", "Laravel");
    assert_eq!(Cache::get("framework").as_deref(), Some("Laravel"));
}
```

## Installation

```toml
[dependencies]
laravel-facade = "0.1"
shaku = "0.6"
```

> [!NOTE]
> shaku's derive macros expand to `::shaku` paths. You need `shaku` as a direct dependency.

## How it works

`#[extends_facade(Cache)]` keeps the trait as is. It adds these items next to it:

| Generated item | Purpose |
| --- | --- |
| `struct Cache;` | The facade. It has the same visibility as the trait. |
| `impl ExtendsFacade for Cache` | Sets `type Accessor = dyn CacheStore`. This is `getFacadeAccessor()`. |
| `Cache::get(..)`, `Cache::put(..)` | One static method per trait method. This is `__callStatic()`. |
| `Cache::swap(..)`, `Cache::fake(..)`, ... | The facade API. You do not need to import a trait for it. |

`Application` is the container behind all facades.
A shaku module cannot be searched by interface at runtime.
So `Application` must know each interface up front:

| Binding | Use it for | Each resolution returns |
| --- | --- | --- |
| `bind::<dyn Trait>()` | shaku components | The same shared instance |
| `bind_provider::<dyn Trait>()` | shaku providers | A new instance |

> [!NOTE]
> The `TypeId` of `dyn Trait` plays the role of Laravel's string key.

> [!TIP]
> A facade caches the first instance it resolves, like `static::$cached` in Laravel.
> Use `#[extends_facade(Uuid, cached = false)]` to get a fresh provider instance on each call.

## Testing

A swapped instance wins over the application. You do not even need an application in tests.

```rust
use std::sync::Mutex;

use laravel_facade::{Facade, Fake, extends_facade};
use mockall::automock;
use mockall::predicate::eq;
use shaku::Interface;

#[extends_facade(Mail)]
#[automock]
pub trait Mailer: Interface {
    fn send(&self, to: &str, body: &str) -> bool;
}

fn send_welcome(to: &str) -> bool {
    Mail::send(to, "Welcome!")
}

#[derive(Default)]
struct MailFake {
    sent: Mutex<Vec<String>>,
}

impl Fake for MailFake {}

impl Mailer for MailFake {
    fn send(&self, to: &str, _body: &str) -> bool {
        self.sent.lock().unwrap().push(to.into());
        true
    }
}

fn main() {
    // Mail::fake()
    let fake = Mail::fake(MailFake::default());
    assert!(send_welcome("taylor@example.com"));
    assert!(Mail::is_fake());
    assert_eq!(*fake.sent.lock().unwrap(), ["taylor@example.com"]);

    // Mail::shouldReceive('send')->with(...)->once()->andReturn(false)
    Mail::should_receive(|mock: &mut MockMailer| {
        mock.expect_send()
            .with(eq("taylor@example.com"), eq("Welcome!"))
            .times(1)
            .return_const(false);
    });
    assert!(!send_welcome("taylor@example.com"));

    // Mockery::close(): drops the mock, and mockall checks `times(1)`.
    Facade::clear_resolved_instances();
}
```

| Method | Swaps in | `is_fake()` |
| --- | --- | --- |
| `Mail::swap(Arc<dyn Mailer>)` | Any instance | `false` |
| `Mail::fake(T)` | A `T: Mailer + Fake`. It returns `Arc<T>` for assertions. | `true` |
| `Mail::should_receive(\|m: &mut M\| ...)` | `M::default()`, such as a mockall mock | `false` |

> [!TIP]
> Like Laravel, a second `should_receive()` with the same mock type adds to the same mock.
> `expects()` is an alias.

> [!WARNING]
> Facade state is global, as in Laravel. But Rust runs the tests of one binary in parallel threads.
> Use [serial_test](https://docs.rs/serial_test) for tests that touch facades.
> Reset the state at the end of each test:
>
> ```rust
> use laravel_facade::Facade;
>
> Facade::clear_resolved_instances();
> Facade::set_facade_application(None);
> ```

## Laravel mapping

| Laravel | laravel-facade |
| --- | --- |
| `abstract class Facade` | `enum Facade {}`. It has no values, like an abstract class. |
| `class Cache extends Facade` | `#[extends_facade(Cache)]` |
| `getFacadeAccessor()` | `type Accessor = dyn CacheStore`. The macro sets it. |
| `__callStatic()` | One static method per trait method |
| `static::$cached` | `#[extends_facade(Cache, cached = false)]` |
| `Facade::setFacadeApplication($app)` | `Facade::set_facade_application(app)` |
| `Facade::getFacadeApplication()` | `Facade::get_facade_application()` |
| `Cache::getFacadeRoot()` | `Cache::get_facade_root()`. It panics with `A facade root has not been set.` |
| `Cache::swap($instance)` | `Cache::swap(instance)`. It also calls `Application::instance()`. |
| `Cache::isFake()` | `Cache::is_fake()`, with the `Fake` marker trait |
| `Cache::shouldReceive()` / `Cache::expects()` | `Cache::should_receive(..)` / `Cache::expects(..)` |
| `Cache::resolved($callback)` | `Cache::resolved(\|root, app\| ...)` |
| `Cache::clearResolvedInstance()` | `Cache::clear_resolved_instance()` |
| `Facade::clearResolvedInstance($name)` | `Facade::clear_resolved_instance::<dyn CacheStore>()` |
| `Facade::clearResolvedInstances()` | `Facade::clear_resolved_instances()` |
| `Cache::spy()` / `Cache::partialMock()` | Not supported. mockall has no spies. |
| `Facade::defaultAliases()` | `use app::facades::Cache;` |

## What the macro can forward

| Trait method | Result |
| --- | --- |
| `fn f(&self, ...)` | Forwarded |
| `fn f(self: Arc<Self>, ...)` | Forwarded |
| `fn f<'a>(&self, x: &'a str) -> &'a str` | Forwarded |
| `async fn f(&self, ...)` with `#[async_trait]` | Forwarded as `async fn`. Put `#[extends_facade]` above `#[async_trait]`. |
| A method with `where Self: Sized` | Skipped. It is not callable on `dyn Trait`. |
| `fn f(&self) -> &str` | Compile error. The borrow would outlive the root `Arc`. |
| `fn f(&mut self)` | Compile error. The root is shared. |
| A method named like a generated one, such as `swap` | Compile error |
| A generic trait | Compile error |

## License

MIT
