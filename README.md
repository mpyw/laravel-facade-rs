# facade-rs

[![CI](https://github.com/mpyw/facade-rs/actions/workflows/test.yaml/badge.svg)](https://github.com/mpyw/facade-rs/actions/workflows/test.yaml)
[![codecov](https://codecov.io/gh/mpyw/facade-rs/graph/badge.svg)](https://codecov.io/gh/mpyw/facade-rs)
[![crates.io](https://img.shields.io/crates/v/facade-rs.svg)](https://crates.io/crates/facade-rs)
[![docs.rs](https://docs.rs/facade-rs/badge.svg)](https://docs.rs/facade-rs)

Laravel-style facades for the [shaku](https://docs.rs/shaku) DI container.

```text
Cache::get("key")
```

Yes, in Rust. This is a joke crate. It still follows
[`Illuminate\Support\Facades\Facade`](https://github.com/illuminate/support/blob/master/Facades/Facade.php)
as closely as Rust allows.

## Installation

```toml
[dependencies]
facade-rs = "0.1"
shaku = "0.6"
```

> [!NOTE]
> shaku's derive macros expand to `::shaku` paths. You need `shaku` as a direct dependency.

## Quick start

Put `#[facade(Name)]` on a shaku interface trait. It generates a unit struct `Name` with one static method per trait method.

```rust
use std::collections::HashMap;
use std::sync::Mutex;

use facade_rs::{Application, facade, set_facade_application};
use shaku::{Component, Interface, module};

#[facade(Cache)]
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
    set_facade_application(app);

    Cache::put("framework", "Laravel");
    assert_eq!(Cache::get("framework").as_deref(), Some("Laravel"));
}
```

> [!IMPORTANT]
> A shaku module cannot be searched by interface at runtime.
> So `Application` must know each interface up front.
> Call `bind::<dyn Trait>()` for components and `bind_provider::<dyn Trait>()` for providers.
> The `TypeId` of `dyn Trait` plays the role of Laravel's string key.

## Testing

A swapped instance wins over the application. You do not even need an application in tests.

```rust
use std::sync::Mutex;

use facade_rs::{Fake, clear_resolved_instances, facade};
use mockall::automock;
use mockall::predicate::eq;
use shaku::Interface;

#[facade(Mail)]
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
    clear_resolved_instances();
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
> facade_rs::clear_resolved_instances();
> facade_rs::set_facade_application(None);
> ```

## Laravel mapping

| Laravel | facade-rs |
| --- | --- |
| `getFacadeAccessor()` | `type Accessor = dyn Trait`. The macro sets it. |
| `__callStatic()` | One generated static method per trait method |
| `static::$cached` | `#[facade(Name, cached = false)]` |
| `setFacadeApplication()` / `getFacadeApplication()` | `set_facade_application()` / `get_facade_application()` |
| `getFacadeRoot()` | `Name::get_facade_root()`. It panics with `A facade root has not been set.` |
| `swap()` | `Name::swap()`. It also calls `Application::instance()`. |
| `isFake()` | `Name::is_fake()`, with the `Fake` marker trait |
| `shouldReceive()` / `expects()` | `Name::should_receive()` / `Name::expects()` |
| `resolved()` | `Name::resolved(\|root, app\| ...)` |
| `clearResolvedInstance()` / `clearResolvedInstances()` | `Name::clear_resolved_instance()` / `clear_resolved_instances()` |
| `spy()` / `partialMock()` | Not supported. mockall has no spies. |
| `defaultAliases()` | `use app::facades::Cache;` |

## What the macro can forward

| Trait method | Result |
| --- | --- |
| `fn f(&self, ...)` | Forwarded |
| `fn f(self: Arc<Self>, ...)` | Forwarded |
| `async fn f(&self, ...)` with `#[async_trait]` | Forwarded as `async fn`. Put `#[facade]` above `#[async_trait]`. |
| `fn f<'a>(&self, x: &'a str) -> &'a str` | Forwarded |
| Methods with `where Self: Sized` | Skipped. They are not callable on `dyn Trait`. |
| `fn f(&self) -> &str` | Compile error. The borrow would outlive the root `Arc`. |
| `fn f(&mut self)` | Compile error. The root is shared. |
| A method named like a generated one, such as `swap` | Compile error |
| A generic trait | Compile error |

## License

MIT
