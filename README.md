<p align="center">
  <img src="https://raw.githubusercontent.com/mpyw/laravel-facade-rs/main/assets/logo.svg" alt="laravel-facade logo" width="160">
</p>

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

Each row shows the same thing in Laravel and in laravel-facade.

<table>
<thead>
<tr><th>What</th><th>Laravel</th><th>laravel-facade</th></tr>
</thead>
<tbody>
<tr>
<td>Define a facade. The trait is the accessor. The macro writes <code>impl ExtendsFacade for Cache</code> with <code>type Accessor = dyn CacheStore</code>.</td>
<td>

```php
class Cache extends Facade
{
    protected static function getFacadeAccessor()
    {
        return 'cache';
    }
}
```

</td>
<td>

```rs
#[extends_facade(Cache)]
pub trait CacheStore: Interface {
    fn get(&self, key: &str) -> Option<String>;
    fn put(&self, key: &str, value: &str);
}
```

</td>
</tr>
<tr>
<td>Do not cache the resolved instance. Each call resolves the accessor again.</td>
<td>

```php
class Uuid extends Facade
{
    protected static $cached = false;

    protected static function getFacadeAccessor()
    {
        return 'uuid';
    }
}
```

</td>
<td>

```rs
#[extends_facade(Uuid, cached = false)]
pub trait IdGenerator: Interface {
    fn id(&self) -> usize;
}
```

</td>
</tr>
<tr>
<td>Call the root through the facade. Laravel forwards with <code>__callStatic()</code> at runtime. The macro generates one static method per trait method.</td>
<td>

```php
Cache::put('framework', 'Laravel');

$value = Cache::get('framework');
```

</td>
<td>

```rs
Cache::put("framework", "Laravel");

let value = Cache::get("framework");
```

</td>
</tr>
<tr>
<td>Bind the accessors in the container. A shaku component is shared. A shaku provider builds a new instance each time.</td>
<td>

```php
$app->singleton('cache', fn () => new ArrayStore);

$app->bind('uuid', fn () => new UuidGenerator);
```

</td>
<td>

```rs
let app = Application::builder(AppModule::builder().build())
    .bind::<dyn CacheStore>()
    .bind_provider::<dyn IdGenerator>()
    .build();
```

</td>
</tr>
<tr>
<td>Set the application behind all facades. Laravel does it while booting. Here you call it yourself.</td>
<td>

```php
Facade::setFacadeApplication($app);

$app = Facade::getFacadeApplication();

Facade::setFacadeApplication(null);
```

</td>
<td>

```rs
Facade::set_facade_application(app);

let app = Facade::get_facade_application();

Facade::set_facade_application(None);
```

</td>
</tr>
<tr>
<td>Get the root instance. Without an application, it panics with <code>A facade root has not been set.</code> The <code>try_</code> version returns an <code>Error</code> instead.</td>
<td>

```php
$store = Cache::getFacadeRoot();
```

</td>
<td>

```rs
let store: Arc<dyn CacheStore> = Cache::get_facade_root();

let store = Cache::try_get_facade_root()?;
```

</td>
</tr>
<tr>
<td>Hot-swap the root. It also replaces the instance in the application.</td>
<td>

```php
Cache::swap(new ArrayStore);
```

</td>
<td>

```rs
Cache::swap(Arc::new(ArrayStore::default()));
```

</td>
</tr>
<tr>
<td>Swap in a fake. A type marked with <code>Fake</code> makes <code>is_fake()</code> return <code>true</code>. <code>fake()</code> returns the fake for assertions.</td>
<td>

```php
class CacheFake implements Fake
{
    // ...
}

Cache::swap(new CacheFake);

Cache::isFake(); // true
```

</td>
<td>

```rs
impl Fake for CacheFake {}

let fake = Cache::fake(CacheFake::default());

Cache::is_fake(); // true
```

</td>
</tr>
<tr>
<td>Set expectations on a mock. The first call swaps in <code>MockCacheStore::default()</code>. Later calls add to the same mock.</td>
<td>

```php
Cache::shouldReceive('get')
    ->with('key')
    ->once()
    ->andReturn('value');
```

</td>
<td>

```rs
Cache::should_receive(|mock: &mut MockCacheStore| {
    mock.expect_get()
        .with(eq("key"))
        .times(1)
        .return_const(Some("value".to_owned()));
});
```

</td>
</tr>
<tr>
<td>Same as <code>shouldReceive()</code>, with the other name.</td>
<td>

```php
Cache::expects('get')
    ->andReturn('value');
```

</td>
<td>

```rs
Cache::expects(|mock: &mut MockCacheStore| {
    mock.expect_get()
        .return_const(Some("value".to_owned()));
});
```

</td>
</tr>
<tr>
<td>Run a callback when the root is resolved. If it was already resolved, the callback runs right away too.</td>
<td>

```php
Cache::resolved(function ($cache, $app) {
    // ...
});
```

</td>
<td>

```rs
Cache::resolved(|cache, app| {
    // ...
});
```

</td>
</tr>
<tr>
<td>Clear cached roots. Laravel names the accessor with a string. Here the facade or the accessor type names it.</td>
<td>

```php
Cache::clearResolvedInstance('cache');

Facade::clearResolvedInstance('cache');

Facade::clearResolvedInstances();
```

</td>
<td>

```rs
Cache::clear_resolved_instance();

Facade::clear_resolved_instance::<dyn CacheStore>();

Facade::clear_resolved_instances();
```

</td>
</tr>
<tr>
<td>Spies and partial mocks. mockall has neither, so they are not supported.</td>
<td>

```php
Cache::spy();

Cache::partialMock();
```

</td>
<td>Not supported</td>
</tr>
<tr>
<td>Short names for facades. Laravel registers aliases. Rust has <code>use</code>.</td>
<td>

```php
// config/app.php
'aliases' => Facade::defaultAliases()->merge([
    'Cache' => Illuminate\Support\Facades\Cache::class,
])->toArray(),
```

</td>
<td>

```rs
use crate::facades::Cache;
```

</td>
</tr>
</tbody>
</table>

## What the macro can forward

The macro looks at each trait method. It forwards it, skips it, or stops with a compile error.

<table>
<thead>
<tr><th>Trait method</th><th>Example</th><th>Result</th><th>Static method or error</th></tr>
</thead>
<tbody>
<tr>
<td>A <code>&amp;self</code> method</td>
<td>

```rs
fn get(&self, key: &str) -> Option<String>;
```

</td>
<td>Forwarded</td>
<td>

```rs
pub fn get(key: &str) -> Option<String>
```

</td>
</tr>
<tr>
<td>A <code>self: Arc&lt;Self&gt;</code> method. The facade passes its root <code>Arc</code>.</td>
<td>

```rs
fn shared(self: Arc<Self>) -> usize;
```

</td>
<td>Forwarded</td>
<td>

```rs
pub fn shared() -> usize
```

</td>
</tr>
<tr>
<td>A method that returns a borrow of an argument, with a named lifetime</td>
<td>

```rs
fn first<'a>(&self, keys: &'a [String]) -> &'a str;
```

</td>
<td>Forwarded</td>
<td>

```rs
pub fn first<'a>(keys: &'a [String]) -> &'a str
```

</td>
</tr>
<tr>
<td>An <code>async fn</code> under <code>#[async_trait]</code>, with <code>#[extends_facade]</code> above it</td>
<td>

```rs
#[extends_facade(Http)]
#[async_trait]
pub trait HttpClient: Interface {
    async fn get(&self, url: &str) -> String;
}
```

</td>
<td>Forwarded</td>
<td>

```rs
pub async fn get(url: &str) -> String
```

</td>
</tr>
<tr>
<td>A method with <code>where Self: Sized</code>. It is not callable on <code>dyn Trait</code>, so there is nothing to forward to.</td>
<td>

```rs
fn new() -> Self
where
    Self: Sized;
```

</td>
<td>Skipped</td>
<td>No static method</td>
</tr>
<tr>
<td>A method that returns a borrow of <code>self</code>. The borrow would outlive the root <code>Arc</code>.</td>
<td>

```rs
fn name(&self) -> &str;
```

</td>
<td>Compile error</td>
<td>

```text
#[extends_facade] cannot forward a method that returns
a borrow of `self`; return an owned value or name the
lifetime of an argument
```

</td>
</tr>
<tr>
<td>A <code>&amp;mut self</code> method. The root is shared.</td>
<td>

```rs
fn increment(&mut self);
```

</td>
<td>Compile error</td>
<td>

```text
#[extends_facade] cannot forward `&mut self` methods:
the facade root is shared behind `Arc`
```

</td>
</tr>
<tr>
<td>A <code>self</code> method without <code>where Self: Sized</code></td>
<td>

```rs
fn into_inner(self) -> String;
```

</td>
<td>Compile error</td>
<td>

```text
#[extends_facade] cannot forward `self` methods;
add `where Self: Sized` to skip it
```

</td>
</tr>
<tr>
<td>Any other receiver, such as <code>Box&lt;Self&gt;</code></td>
<td>

```rs
fn boxed(self: Box<Self>);
```

</td>
<td>Compile error</td>
<td>

```text
#[extends_facade] can only forward `&self` and
`self: Arc<Self>` methods
```

</td>
</tr>
<tr>
<td>A method named like a generated one</td>
<td>

```rs
fn swap(&self);
```

</td>
<td>Compile error</td>
<td>

```text
`swap` clashes with a method generated on every facade
```

</td>
</tr>
<tr>
<td>A generic trait</td>
<td>

```rs
#[extends_facade(Repo)]
pub trait Repository<T>: Interface {
    fn find(&self, id: u64) -> Option<T>;
}
```

</td>
<td>Compile error</td>
<td>

```text
#[extends_facade] does not support generic traits
```

</td>
</tr>
<tr>
<td><code>#[extends_facade]</code> below <code>#[async_trait]</code>. It would see the rewritten methods.</td>
<td>

```rs
#[async_trait]
#[extends_facade(Http)]
pub trait HttpClient: Interface {
    async fn get(&self, url: &str) -> String;
}
```

</td>
<td>Compile error</td>
<td>

```text
#[extends_facade] must be placed above #[async_trait]
```

</td>
</tr>
</tbody>
</table>

## License

MIT
