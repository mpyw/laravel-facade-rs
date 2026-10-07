//! Procedural macros for [laravel-facade](https://docs.rs/laravel-facade).

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    FnArg, GenericArgument, GenericParam, Ident, ItemTrait, LitBool, Pat, Path, PathArguments, ReturnType, Signature,
    Token, TraitItem, TraitItemFn, Type, TypeParamBound, WherePredicate, parse_quote,
};

/// Methods generated on every facade. A trait method with the same name would clash.
const RESERVED: &[&str] = &[
    "get_facade_accessor",
    "get_facade_root",
    "try_get_facade_root",
    "swap",
    "is_fake",
    "clear_resolved_instance",
    "resolved",
    "fake",
    "should_receive",
    "expects",
];

/// Turns a shaku interface trait into a Laravel-style facade.
///
/// `#[extends_facade(Name)]` keeps the trait as is and generates:
///
/// | Item | Purpose |
/// |---|---|
/// | `struct Name;` | The facade, with the same visibility as the trait |
/// | `impl laravel_facade::ExtendsFacade for Name` | `type Accessor = dyn Trait` |
/// | One static method per trait method | `Name::method(args)` calls the facade root |
/// | `get_facade_root`, `swap`, `fake`, `should_receive`, ... | The `ExtendsFacade` API without importing the trait |
///
/// # Options
///
/// | Option | Default | Meaning |
/// |---|---|---|
/// | `cached = false` | `true` | Do not cache the resolved instance. Same as `static::$cached`. |
/// | `crate = path` | `::laravel_facade` | Path to the laravel-facade crate, for re-exports |
///
/// # Examples
///
/// ```
/// use laravel_facade::{Application, extends_facade};
/// use shaku::{Component, Interface, module};
///
/// #[extends_facade(Log)]
/// pub trait Logger: Interface {
///     /// Formats a log line.
///     fn format(&self, level: &str, message: &str) -> String;
/// }
///
/// #[derive(Component)]
/// #[shaku(interface = Logger)]
/// struct PlainLogger;
///
/// impl Logger for PlainLogger {
///     fn format(&self, level: &str, message: &str) -> String {
///         format!("[{level}] {message}")
///     }
/// }
///
/// module! {
///     AppModule {
///         components = [PlainLogger],
///         providers = []
///     }
/// }
///
/// let app = Application::builder(AppModule::builder().build())
///     .bind::<dyn Logger>()
///     .build();
/// laravel_facade::Facade::set_facade_application(app);
///
/// assert_eq!(Log::format("info", "booted"), "[info] booted");
/// ```
///
/// Async methods work with `#[async_trait]`. Put `#[extends_facade]` above it, so that
/// it sees `async fn` before the rewrite:
///
/// ```
/// use std::sync::Arc;
///
/// use async_trait::async_trait;
/// use laravel_facade::extends_facade;
/// use shaku::Interface;
///
/// #[extends_facade(Http)]
/// #[async_trait]
/// pub trait HttpClient: Interface {
///     async fn get(&self, url: &str) -> String;
/// }
///
/// struct EchoClient;
///
/// #[async_trait]
/// impl HttpClient for EchoClient {
///     async fn get(&self, url: &str) -> String {
///         format!("GET {url}")
///     }
/// }
///
/// Http::swap(Arc::new(EchoClient));
/// assert_eq!(pollster::block_on(Http::get("/users")), "GET /users");
/// ```
///
/// A method that returns a borrow of `self` cannot be forwarded.
/// The root `Arc` is dropped when the static method returns:
///
/// ```compile_fail
/// use laravel_facade::extends_facade;
/// use shaku::Interface;
///
/// #[extends_facade(Config)]
/// pub trait ConfigRepository: Interface {
///     fn name(&self) -> &str; // error: cannot forward a method that returns a borrow of `self`
/// }
/// ```
///
/// Name the lifetime of an argument instead, or return an owned value:
///
/// ```
/// use std::sync::Arc;
///
/// use laravel_facade::extends_facade;
/// use shaku::Interface;
///
/// #[extends_facade(Config)]
/// pub trait ConfigRepository: Interface {
///     fn name(&self) -> String;
///     fn first<'a>(&self, keys: &'a [String]) -> &'a str;
/// }
///
/// struct StaticConfig;
///
/// impl ConfigRepository for StaticConfig {
///     fn name(&self) -> String {
///         "laravel-facade".into()
///     }
///
///     fn first<'a>(&self, keys: &'a [String]) -> &'a str {
///         &keys[0]
///     }
/// }
///
/// Config::swap(Arc::new(StaticConfig));
/// assert_eq!(Config::name(), "laravel-facade");
/// assert_eq!(Config::first(&["app.name".into()]), "app.name");
/// ```
///
/// `&mut self` methods cannot be forwarded either, because the root is shared:
///
/// ```compile_fail
/// use laravel_facade::extends_facade;
/// use shaku::Interface;
///
/// #[extends_facade(Counter)]
/// pub trait CounterStore: Interface {
///     fn increment(&mut self); // error: cannot forward `&mut self` methods
/// }
/// ```
#[proc_macro_attribute]
pub fn extends_facade(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand_attribute(attr.into(), item.into()).into()
}

/// The macro body on `proc_macro2` types, so that unit tests can call it.
/// Code inside a real `proc_macro` runs in the compiler and is never measured.
fn expand_attribute(attr: TokenStream2, item: TokenStream2) -> TokenStream2 {
    let item: ItemTrait = match syn::parse2(item) {
        Ok(item) => item,
        Err(e) => return e.into_compile_error(),
    };
    // Keep the trait even on errors, so that they do not cascade.
    let expanded = syn::parse2::<Args>(attr)
        .and_then(|args| expand(&args, &item))
        .unwrap_or_else(syn::Error::into_compile_error);
    quote! {
        #item
        #expanded
    }
}

struct Args {
    name: Ident,
    cached: bool,
    krate: Path,
}

impl Parse for Args {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut args = Args {
            name: input.parse()?,
            cached: true,
            krate: parse_quote!(::laravel_facade),
        };
        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key = input.call(Ident::parse_any)?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "cached" => args.cached = input.parse::<LitBool>()?.value,
                "crate" => args.krate = input.parse()?,
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "unknown option; expected `cached` or `crate`",
                    ));
                }
            }
        }
        Ok(args)
    }
}

fn expand(args: &Args, item: &ItemTrait) -> syn::Result<TokenStream2> {
    let Args { name, cached, krate } = args;
    let vis = &item.vis;
    let tr = &item.ident;

    if !item.generics.params.is_empty() {
        return Err(syn::Error::new(
            item.generics.lt_token.unwrap_or_default().span,
            "#[extends_facade] does not support generic traits",
        ));
    }

    let mut errors: Option<syn::Error> = None;
    let mut methods = Vec::new();
    for trait_item in &item.items {
        let TraitItem::Fn(f) = trait_item else { continue };
        match forward(tr, krate, vis, f) {
            Ok(Some(method)) => methods.push(method),
            Ok(None) => {}
            Err(e) => match &mut errors {
                Some(errors) => errors.combine(e),
                None => errors = Some(e),
            },
        }
    }
    if let Some(errors) = errors {
        return Err(errors);
    }

    let doc = format!("Facade for [`{tr}`]. Generated by `#[extends_facade]`.");
    Ok(quote! {
        #[doc = #doc]
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
        #vis struct #name;

        impl #krate::ExtendsFacade for #name {
            type Accessor = dyn #tr;
            const CACHED: bool = #cached;
        }

        #[allow(dead_code)]
        impl #name {
            /// Returns the accessor name. See `ExtendsFacade::get_facade_accessor`.
            #vis fn get_facade_accessor() -> &'static str {
                <Self as #krate::ExtendsFacade>::get_facade_accessor()
            }

            /// Returns the instance behind the facade. See `ExtendsFacade::get_facade_root`.
            #vis fn get_facade_root() -> ::std::sync::Arc<dyn #tr> {
                <Self as #krate::ExtendsFacade>::get_facade_root()
            }

            /// See `ExtendsFacade::try_get_facade_root`.
            #vis fn try_get_facade_root() -> ::core::result::Result<::std::sync::Arc<dyn #tr>, #krate::Error> {
                <Self as #krate::ExtendsFacade>::try_get_facade_root()
            }

            /// Hot-swaps the instance behind the facade. See `ExtendsFacade::swap`.
            #vis fn swap(instance: ::std::sync::Arc<dyn #tr>) {
                <Self as #krate::ExtendsFacade>::swap(instance)
            }

            /// See `ExtendsFacade::is_fake`.
            #vis fn is_fake() -> bool {
                <Self as #krate::ExtendsFacade>::is_fake()
            }

            /// See `ExtendsFacade::clear_resolved_instance`.
            #vis fn clear_resolved_instance() {
                <Self as #krate::ExtendsFacade>::clear_resolved_instance()
            }

            /// See `ExtendsFacade::resolved`.
            #vis fn resolved(
                callback: impl Fn(&::std::sync::Arc<dyn #tr>, &#krate::Application) + Send + Sync + 'static,
            ) {
                <Self as #krate::ExtendsFacade>::resolved(callback)
            }

            /// Swaps in a fake and returns it, so that you can make assertions on it.
            /// After this, `is_fake()` returns `true`.
            #vis fn fake<__FacadeFake: #tr + #krate::Fake>(fake: __FacadeFake) -> ::std::sync::Arc<__FacadeFake> {
                let fake = ::std::sync::Arc::new(fake);
                #krate::__private::fake::<Self>(::std::sync::Arc::clone(&fake) as ::std::sync::Arc<dyn #tr>);
                fake
            }

            /// Adds expectations to a mock behind the facade, such as a mockall mock.
            ///
            /// The first call swaps in `__FacadeMock::default()`. Later calls with the
            /// same mock type add to the same mock, like Laravel's `shouldReceive()`.
            #vis fn should_receive<__FacadeMock: #tr + ::core::default::Default, __FacadeR>(
                expect: impl ::core::ops::FnOnce(&mut __FacadeMock) -> __FacadeR,
            ) -> __FacadeR {
                #krate::__private::should_receive::<Self, __FacadeMock, __FacadeR>(
                    |mock| -> ::std::sync::Arc<dyn #tr> { mock },
                    expect,
                )
            }

            /// Same as [`should_receive`](Self::should_receive).
            #vis fn expects<__FacadeMock: #tr + ::core::default::Default, __FacadeR>(
                expect: impl ::core::ops::FnOnce(&mut __FacadeMock) -> __FacadeR,
            ) -> __FacadeR {
                Self::should_receive(expect)
            }

            #(#methods)*
        }
    })
}

enum Receiver {
    /// `&self`
    Ref,
    /// `self: Arc<Self>`
    Arc,
}

/// Builds the static method for one trait method, or `None` to skip it.
fn forward(tr: &Ident, krate: &Path, vis: &syn::Visibility, f: &TraitItemFn) -> syn::Result<Option<TokenStream2>> {
    let sig = &f.sig;
    let ident = &sig.ident;

    // Not callable on `dyn Trait`, so there is nothing to forward to.
    if requires_sized_self(sig) {
        return Ok(None);
    }
    let Some(receiver) = sig.receiver() else {
        return Ok(None);
    };
    if sig
        .generics
        .params
        .iter()
        .any(|p| !matches!(p, GenericParam::Lifetime(_)))
    {
        return Ok(None);
    }

    if RESERVED.contains(&ident.to_string().as_str()) {
        return Err(syn::Error::new(
            ident.span(),
            format!("`{ident}` clashes with a method generated on every facade"),
        ));
    }
    if sig.generics.lifetimes().any(|l| l.lifetime.ident == "async_trait") {
        return Err(syn::Error::new(
            ident.span(),
            "#[extends_facade] must be placed above #[async_trait]",
        ));
    }

    let kind = if receiver.colon_token.is_none() {
        match (&receiver.reference, &receiver.mutability) {
            (Some(_), None) => Receiver::Ref,
            (Some(_), Some(_)) => {
                return Err(syn::Error::new(
                    receiver.self_token.span,
                    "#[extends_facade] cannot forward `&mut self` methods: the facade root is shared behind `Arc`",
                ));
            }
            _ => {
                return Err(syn::Error::new(
                    receiver.self_token.span,
                    "#[extends_facade] cannot forward `self` methods; add `where Self: Sized` to skip it",
                ));
            }
        }
    } else if is_arc_self(&receiver.ty) {
        Receiver::Arc
    } else if matches!(&*receiver.ty, Type::Reference(r) if r.mutability.is_none() && is_self(&r.elem)) {
        Receiver::Ref
    } else {
        return Err(syn::Error::new(
            receiver.self_token.span,
            "#[extends_facade] can only forward `&self` and `self: Arc<Self>` methods",
        ));
    };

    if let ReturnType::Type(_, ty) = &sig.output {
        let mut finder = ElidedLifetime(None);
        finder.visit_type(ty);
        if let Some(span) = finder.0 {
            return Err(syn::Error::new(
                span,
                "#[extends_facade] cannot forward a method that returns a borrow of `self`; return an owned value or name the lifetime of an argument",
            ));
        }
    }

    // Static signature: drop the receiver and give every argument a plain name.
    let mut static_sig: Signature = sig.clone();
    static_sig.inputs = Default::default();
    let mut arg_names = Vec::new();
    for (i, input) in sig.inputs.iter().enumerate() {
        let FnArg::Typed(pat_type) = input else { continue };
        let name = match &*pat_type.pat {
            Pat::Ident(p) if p.ident != "self" => p.ident.clone(),
            _ => format_ident!("__arg{i}"),
        };
        let ty = &pat_type.ty;
        static_sig.inputs.push(parse_quote!(#name: #ty));
        arg_names.push(name);
    }

    let root = quote!(<Self as #krate::ExtendsFacade>::get_facade_root());
    let target = quote!(<dyn #tr as #tr>::#ident);
    let mut call = match kind {
        Receiver::Ref => quote!(#target(&*__facade_root #(, #arg_names)*)),
        Receiver::Arc => quote!(#target(__facade_root #(, #arg_names)*)),
    };
    if sig.asyncness.is_some() {
        call = quote!(#call.await);
    }
    if sig.unsafety.is_some() {
        call = quote!(unsafe { #call });
    }

    let attrs = f.attrs.iter().filter(|a| {
        ["doc", "cfg", "deprecated", "must_use", "allow", "expect"]
            .iter()
            .any(|name| a.path().is_ident(name))
    });

    Ok(Some(quote_spanned! {sig.span()=>
        #(#attrs)*
        #vis #static_sig {
            let __facade_root = #root;
            #call
        }
    }))
}

fn requires_sized_self(sig: &Signature) -> bool {
    let Some(where_clause) = &sig.generics.where_clause else {
        return false;
    };
    where_clause.predicates.iter().any(|p| {
        let WherePredicate::Type(p) = p else { return false };
        is_self(&p.bounded_ty)
            && p.bounds
                .iter()
                .any(|b| matches!(b, TypeParamBound::Trait(t) if t.path.is_ident("Sized")))
    })
}

fn is_self(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.qself.is_none() && p.path.is_ident("Self"))
}

fn is_arc_self(ty: &Type) -> bool {
    let Type::Path(p) = ty else { return false };
    p.path.segments.last().is_some_and(|last| {
        last.ident == "Arc"
            && matches!(
                &last.arguments,
                PathArguments::AngleBracketed(args)
                    if matches!(args.args.first(), Some(GenericArgument::Type(ty)) if is_self(ty))
            )
    })
}

/// Finds `&T` without a lifetime, or `'_`, in a return type.
struct ElidedLifetime(Option<proc_macro2::Span>);

impl<'ast> Visit<'ast> for ElidedLifetime {
    fn visit_type_reference(&mut self, r: &'ast syn::TypeReference) {
        if r.lifetime.is_none() && self.0.is_none() {
            self.0 = Some(r.and_token.span);
        }
        syn::visit::visit_type_reference(self, r);
    }

    fn visit_lifetime(&mut self, l: &'ast syn::Lifetime) {
        if l.ident == "_" && self.0.is_none() {
            self.0 = Some(l.span());
        }
    }

    // `fn(&str) -> &str` and `dyn Fn(&str) -> &str` have their own elision scope.
    fn visit_type_bare_fn(&mut self, _: &'ast syn::TypeBareFn) {}
    fn visit_parenthesized_generic_arguments(&mut self, _: &'ast syn::ParenthesizedGenericArguments) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand(attr: &str, item: &str) -> String {
        expand_attribute(attr.parse().unwrap(), item.parse().unwrap()).to_string()
    }

    /// Expands a trait named `T` with one extra item, behind `#[extends_facade(F)]`.
    fn expand_method(method: &str) -> String {
        expand("F", &format!("pub trait T: Interface {{ {method} }}"))
    }

    #[track_caller]
    fn assert_error(output: &str, message: &str) {
        assert!(output.contains("compile_error"), "no error in: {output}");
        assert!(output.contains(message), "no `{message}` in: {output}");
    }

    #[track_caller]
    fn assert_no_error(output: &str) {
        assert!(!output.contains("compile_error"), "unexpected error in: {output}");
    }

    /// Returns `true` if the facade struct got a static method `name`.
    fn has_static_method(output: &str, name: &str) -> bool {
        let generated = &output[output.find("impl F {").unwrap()..];
        generated.contains(&format!("fn {name} (")) || generated.contains(&format!("fn {name} <"))
    }

    #[test]
    fn generates_the_facade() {
        let output = expand(
            "Cache",
            "pub trait CacheStore: Interface { fn get(&self, key: &str) -> Option<String>; }",
        );
        assert_no_error(&output);
        assert!(output.contains("pub trait CacheStore"));
        assert!(output.contains("pub struct Cache ;"));
        assert!(output.contains("Facade for [`CacheStore`]. Generated by `#[extends_facade]`."));
        assert!(output.contains("impl :: laravel_facade :: ExtendsFacade for Cache"));
        assert!(output.contains("type Accessor = dyn CacheStore"));
        assert!(output.contains("const CACHED : bool = true"));
        assert!(output.contains("pub fn get (key : & str) -> Option < String >"));
        assert!(output.contains("< dyn CacheStore as CacheStore > :: get (& * __facade_root , key)"));
        for name in RESERVED {
            assert!(output.contains(&format!("fn {name}")), "missing {name}");
        }
    }

    #[test]
    fn keeps_the_visibility_of_the_trait() {
        let output = expand("F", "pub(crate) trait T: Interface { fn f(&self); }");
        assert!(output.contains("pub (crate) struct F ;"));
        assert!(output.contains("pub (crate) fn f ()"));

        let output = expand("F", "trait T: Interface { fn f(&self); }");
        assert!(output.contains("struct F ;"));
        assert!(!output.contains("pub"));
    }

    #[test]
    fn parses_options() {
        let output = expand("F, cached = false, crate = ::my::facade,", "trait T: Interface {}");
        assert_no_error(&output);
        assert!(output.contains("const CACHED : bool = false"));
        assert!(output.contains("impl :: my :: facade :: ExtendsFacade for F"));
        assert!(!output.contains("laravel_facade"));

        assert_no_error(&expand("F,", "trait T: Interface {}"));
    }

    #[test]
    fn rejects_bad_options() {
        let item = "trait T: Interface {}";
        assert_error(
            &expand("F, cache = false", item),
            "unknown option; expected `cached` or `crate`",
        );
        assert_error(&expand("F, cached = 1", item), "expected boolean literal");
        assert_error(&expand("F, cached", item), "expected `=`");
        assert_error(&expand("F cached", item), "expected `,`");
        assert_error(&expand("F, 1 = 2", item), "expected ident");
        assert_error(&expand("F, crate = 1", item), "expected identifier");
        assert_error(&expand("", item), "expected identifier");
        // The trait is still emitted.
        assert!(expand("", item).contains("trait T"));
    }

    #[test]
    fn rejects_non_traits() {
        assert_error(&expand("F", "struct S;"), "expected `trait`");
    }

    #[test]
    fn rejects_generic_traits() {
        assert_error(
            &expand("F", "trait T<U>: Interface {}"),
            "does not support generic traits",
        );
        assert_error(
            &expand("F", "trait T<'a>: Interface {}"),
            "does not support generic traits",
        );
    }

    #[test]
    fn forwards_supported_receivers() {
        for (method, call) in [
            ("fn f(&self);", "(& * __facade_root)"),
            ("fn f(self: &Self);", "(& * __facade_root)"),
            ("fn f(self: Arc<Self>);", "(__facade_root)"),
            ("fn f(self: std::sync::Arc<Self>);", "(__facade_root)"),
        ] {
            let output = expand_method(method);
            assert_no_error(&output);
            assert!(
                output.contains(&format!("< dyn T as T > :: f {call}")),
                "{method}: {output}"
            );
        }
    }

    #[test]
    fn rejects_unsupported_receivers() {
        for (method, message) in [
            ("fn f(&mut self);", "cannot forward `&mut self` methods"),
            (
                "fn f(self);",
                "cannot forward `self` methods; add `where Self: Sized` to skip it",
            ),
            ("fn f(mut self);", "cannot forward `self` methods"),
            (
                "fn f(self: Box<Self>);",
                "can only forward `&self` and `self: Arc<Self>` methods",
            ),
            ("fn f(self: &mut Self);", "can only forward"),
            ("fn f(self: Arc<Box<Self>>);", "can only forward"),
            ("fn f(self: Arc<'static, Self>);", "can only forward"),
            ("fn f(self: Arc);", "can only forward"),
            ("fn f(self: (Self,));", "can only forward"),
            ("fn f(self: <Self as X>::Y);", "can only forward"),
        ] {
            assert_error(&expand_method(method), message);
        }
    }

    #[test]
    fn skips_methods_not_callable_on_dyn() {
        for method in [
            "fn new() -> Self where Self: Sized;",
            "fn f(self) where Self: Sized;",
            "fn f(&mut self) where Self: core::marker::Copy + Sized;",
            "fn f<U>(&self, u: U);",
            "fn f<const N: usize>(&self);",
            "fn assoc();",
            "const C: u8;",
            "type X;",
        ] {
            let output = expand_method(method);
            assert_no_error(&output);
            assert!(
                !has_static_method(&output, "f") && !has_static_method(&output, "new"),
                "{method}: {output}"
            );
        }
    }

    #[test]
    fn forwards_methods_with_other_where_clauses() {
        for method in [
            "fn f(&self) where Self: Clone;",
            "fn f<'a, 'b>(&self, x: &'a str) where 'a: 'b;",
            "fn f(&self) where Self: 'static;",
            "fn f(&self) where u8: Sized;",
        ] {
            let output = expand_method(method);
            assert_no_error(&output);
            assert!(has_static_method(&output, "f"), "{method}: {output}");
        }
    }

    #[test]
    fn forwards_async_and_unsafe_methods() {
        let output = expand_method("async fn f(&self) -> u8;");
        assert!(output.contains("pub async fn f () -> u8"));
        assert!(output.contains(":: f (& * __facade_root) . await"));

        let output = expand_method("unsafe fn f(&self);");
        assert!(output.contains("pub unsafe fn f ()"));
        assert!(output.contains("unsafe { < dyn T as T > :: f (& * __facade_root) }"));

        let output = expand_method("async unsafe fn f(&self);");
        assert!(output.contains("unsafe { < dyn T as T > :: f (& * __facade_root) . await }"));
    }

    #[test]
    fn rejects_async_trait_output() {
        let method = "fn get<'life0, 'async_trait>(&'life0 self) -> Pin<Box<dyn Future<Output = String> + Send + 'async_trait>> where 'life0: 'async_trait, Self: 'async_trait;";
        assert_error(&expand_method(method), "must be placed above #[async_trait]");
    }

    #[test]
    fn rejects_reserved_names() {
        for name in RESERVED {
            assert_error(
                &expand_method(&format!("fn {name}(&self);")),
                &format!("`{name}` clashes with a method generated on every facade"),
            );
        }
    }

    #[test]
    fn rejects_borrows_of_self_in_the_return_type() {
        for method in [
            "fn f(&self) -> &str;",
            "fn f(&self) -> Option<&str>;",
            "fn f(&self) -> Cow<'_, str>;",
            "fn f(&self) -> (u8, &[u8]);",
            "fn f(&self) -> &'static &str;",
        ] {
            assert_error(
                &expand_method(method),
                "cannot forward a method that returns a borrow of `self`",
            );
        }
    }

    #[test]
    fn allows_returns_with_their_own_lifetimes() {
        for method in [
            "fn f<'a>(&self, x: &'a str) -> &'a str;",
            "fn f(&self) -> &'static str;",
            "fn f(&self) -> fn(&str) -> &str;",
            "fn f(&self) -> Box<dyn Fn(&str) -> &str>;",
            "fn f(&self) -> Box<dyn for<'a> Fn(&'a str) -> &'a str>;",
            "fn f(&self);",
        ] {
            let output = expand_method(method);
            assert_no_error(&output);
            assert!(has_static_method(&output, "f"), "{method}: {output}");
        }
    }

    #[test]
    fn renames_argument_patterns() {
        let output = expand_method("fn f(&self, key: &str, _: u8, mut count: u8, (a, b): (u8, u8)) {}");
        assert_no_error(&output);
        assert!(output.contains("fn f (key : & str , __arg2 : u8 , count : u8 , __arg4 : (u8 , u8))"));
        assert!(output.contains(":: f (& * __facade_root , key , __arg2 , count , __arg4)"));
    }

    #[test]
    fn copies_documentation_attributes_only() {
        let output = expand_method(
            "/// Docs.\n#[cfg(unix)] #[deprecated] #[must_use] #[allow(unused)] #[expect(dead_code)] #[inline] fn f(&self) -> u8;",
        );
        let generated = &output[output.find("impl F {").unwrap()..];
        for attr in [
            "doc = \" Docs.\"",
            "cfg (unix)",
            "deprecated",
            "must_use",
            "allow (unused)",
            "expect (dead_code)",
        ] {
            assert!(generated.contains(attr), "missing {attr}: {generated}");
        }
        assert!(!generated.contains("inline"));
    }

    #[test]
    fn combines_errors_from_several_methods() {
        let output = expand_method("fn a(&mut self); fn b(&self) -> &str; fn swap(&self);");
        assert_error(&output, "cannot forward `&mut self` methods");
        assert_error(&output, "returns a borrow of `self`");
        assert_error(&output, "`swap` clashes");
        assert!(!output.contains("struct F"));
    }
}
