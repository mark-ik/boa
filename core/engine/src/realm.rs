//! Boa's implementation of ECMAScript's `Realm Records`
//!
//! Conceptually, a realm consists of a set of intrinsic objects, an ECMAScript global environment,
//! all of the ECMAScript code that is loaded within the scope of that global environment,
//! and other associated state and resources.
//!
//! A realm is represented in this implementation as a Realm struct with the fields specified from the spec.

use std::{any::TypeId, cell::Cell};

use crate::{
    Context, HostDefined, JsNativeError, JsObject, JsResult, JsString,
    class::Class,
    context::{
        HostHooks,
        intrinsics::{Intrinsics, StandardConstructor},
    },
    environments::DeclarativeEnvironment,
    js_string,
    module::Module,
    object::shape::RootShape,
};
use boa_ast::scope::Scope;
use boa_engine::JsValue;
use boa_engine::property::{Attribute, PropertyDescriptor, PropertyKey};
use boa_gc::{Finalize, Gc, GcRef, GcRefCell, GcRefMut, Trace};
use rustc_hash::FxHashMap;

/// Representation of a Realm.
///
/// In the specification these are called Realm Records.
#[derive(Clone, Trace, Finalize)]
pub struct Realm {
    inner: Gc<Inner>,
}

impl Eq for Realm {}

impl PartialEq for Realm {
    fn eq(&self, other: &Self) -> bool {
        Gc::ptr_eq(&self.inner, &other.inner)
    }
}

impl std::fmt::Debug for Realm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Realm")
            .field("intrinsics", &self.inner.intrinsics)
            .field("environment", &self.inner.environment)
            .field("global_object", &self.inner.global_object)
            .field("global_this", &self.inner.global_this)
            .finish()
    }
}

#[derive(Trace, Finalize)]
struct Inner {
    intrinsics: Intrinsics,

    /// The global declarative environment of this realm.
    environment: Gc<DeclarativeEnvironment>,

    /// The global scope of this realm.
    /// This is directly related to the global declarative environment.
    // Safety: Nothing in `Scope` needs tracing.
    #[unsafe_ignore_trace]
    scope: Scope,

    global_object: JsObject,

    /// The global `this` value of this realm.
    ///
    /// Mutable only until the realm's global `this` is fixed; see
    /// [`Realm::finish_global_this_initialization`].
    global_this: GcRefCell<JsObject>,

    /// Whether this realm's global `this` value is fixed.
    ///
    /// Set either by [`Realm::finish_global_this_initialization`] or by the
    /// first time code runs in this realm, whichever happens first. See that
    /// method for why both of those close the window.
    // Safety: `Cell<bool>` holds nothing that needs tracing.
    #[unsafe_ignore_trace]
    global_this_fixed: Cell<bool>,
    template_map: GcRefCell<FxHashMap<u64, JsObject>>,
    loaded_modules: GcRefCell<FxHashMap<JsString, Module>>,
    host_classes: GcRefCell<FxHashMap<TypeId, StandardConstructor>>,

    host_defined: GcRefCell<HostDefined>,
}

impl Realm {
    /// Create a new [`Realm`].
    #[inline]
    pub fn create(hooks: &dyn HostHooks, root_shape: &RootShape) -> JsResult<Self> {
        let intrinsics = Intrinsics::uninit(root_shape).ok_or_else(|| {
            JsNativeError::typ().with_message("failed to create the realm intrinsics")
        })?;

        let global_object = hooks.create_global_object(&intrinsics);
        let global_this = hooks
            .create_global_this(&intrinsics)
            .unwrap_or_else(|| global_object.clone());
        let environment = Gc::new(DeclarativeEnvironment::global());
        let scope = Scope::new_global();

        let realm = Self {
            inner: Gc::new(Inner {
                intrinsics,
                environment,
                scope,
                global_object,
                global_this: GcRefCell::new(global_this),
                global_this_fixed: Cell::new(false),
                template_map: GcRefCell::default(),
                loaded_modules: GcRefCell::default(),
                host_classes: GcRefCell::default(),
                host_defined: GcRefCell::default(),
            }),
        };

        realm.initialize();

        Ok(realm)
    }

    /// Gets the intrinsics of this `Realm`.
    #[inline]
    #[must_use]
    pub fn intrinsics(&self) -> &Intrinsics {
        &self.inner.intrinsics
    }

    /// Returns an immutable reference to the [`ECMAScript specification`][spec] defined
    /// [`\[\[\HostDefined]\]`][`HostDefined`] field of the [`Realm`].
    ///
    /// [spec]: https://tc39.es/ecma262/#table-realm-record-fields
    ///
    /// # Panics
    ///
    /// Panics if [`HostDefined`] field is mutably borrowed.
    #[inline]
    #[must_use]
    pub fn host_defined(&self) -> GcRef<'_, HostDefined> {
        self.inner.host_defined.borrow()
    }

    /// Returns a mutable reference to [`ECMAScript specification`][spec] defined
    /// [`\[\[\HostDefined]\]`][`HostDefined`] field of the [`Realm`].
    ///
    /// [spec]: https://tc39.es/ecma262/#table-realm-record-fields
    ///
    /// # Panics
    ///
    /// Panics if [`HostDefined`] field is borrowed.
    #[inline]
    #[must_use]
    pub fn host_defined_mut(&self) -> GcRefMut<'_, HostDefined> {
        self.inner.host_defined.borrow_mut()
    }

    /// Checks if this `Realm` has the class `C` registered into its class map.
    #[must_use]
    pub fn has_class<C: Class>(&self) -> bool {
        self.inner
            .host_classes
            .borrow()
            .contains_key(&TypeId::of::<C>())
    }

    /// Gets the constructor and prototype of the class `C` if it is registered in the class map.
    #[must_use]
    pub fn get_class<C: Class>(&self) -> Option<StandardConstructor> {
        self.inner
            .host_classes
            .borrow()
            .get(&TypeId::of::<C>())
            .cloned()
    }

    pub(crate) fn environment(&self) -> &Gc<DeclarativeEnvironment> {
        &self.inner.environment
    }

    /// Returns the scope of this realm.
    #[must_use]
    pub fn scope(&self) -> &Scope {
        &self.inner.scope
    }

    pub(crate) fn global_object(&self) -> &JsObject {
        &self.inner.global_object
    }

    pub(crate) fn global_this(&self) -> JsObject {
        self.inner.global_this.borrow().clone()
    }

    /// Records that code has run in this realm, fixing its global `this`.
    pub(crate) fn fix_global_this(&self) {
        self.inner.global_this_fixed.set(true);
    }

    /// Finishes this realm's global `this` initialization, installing
    /// `global_this` as the realm's `[[GlobalThisValue]]`.
    ///
    /// Realm creation is in two halves for a host that needs this. The first is
    /// [`HostHooks::create_global_this`], which runs *during* realm creation,
    /// before any [`Context`] exists; a host whose global `this` can only be
    /// built with a live context - an exotic object whose traps call back into
    /// the engine, say - has nothing to return there. This method is the second
    /// half, and the host calls it as soon as the context exists.
    ///
    /// It is deliberately not a setter. The window closes the moment anything
    /// could have observed the original value, and it closes for good:
    ///
    /// - after any code has run in this realm, because a call frame caches the
    ///   `this` it was entered with ([`CallFrameFlags::THIS_VALUE_CACHED`]) and
    ///   an already-running script may hold the old object outright; and
    /// - after a successful call, because the point is to finish an
    ///   initialization, not to keep a realm's identity swappable.
    ///
    /// Outside that window it fails with a `TypeError` and changes nothing.
    ///
    /// Within it, one field is enough. Every other read of the global `this` -
    /// the `This` opcode, the `this` substituted into a sloppy-mode call, the
    /// value `CheckReturn` falls back to, and the global environment record,
    /// which holds no `this` of its own and defers here - reads this field
    /// live. The one exception is the `globalThis` property that
    /// [`SetDefaultGlobalBindings`][spec] wrote as a snapshot of the original
    /// value, and this method redefines it.
    ///
    /// The global *object* is untouched: `var` bindings and everything the host
    /// installed still land on it, which is what a forwarding global `this`
    /// wants to forward to.
    ///
    /// [`CallFrameFlags::THIS_VALUE_CACHED`]: crate::vm::CallFrameFlags
    /// [spec]: https://tc39.es/ecma262/#sec-setdefaultglobalbindings
    pub fn finish_global_this_initialization(
        &self,
        global_this: JsObject,
        context: &mut Context,
    ) -> JsResult<()> {
        if self.inner.global_this_fixed.get() {
            return Err(JsNativeError::typ()
                .with_message(concat!(
                    "the global `this` value of this realm is already fixed: it can be ",
                    "initialized only once, and only before any code runs in the realm",
                ))
                .into());
        }
        self.inner.global_this_fixed.set(true);
        *self.inner.global_this.borrow_mut() = global_this.clone();
        self.global_object().define_property_or_throw(
            js_string!("globalThis"),
            PropertyDescriptor::builder()
                .value(global_this)
                .writable(true)
                .enumerable(false)
                .configurable(true),
            context,
        )?;
        Ok(())
    }

    pub(crate) fn loaded_modules(&self) -> &GcRefCell<FxHashMap<JsString, Module>> {
        &self.inner.loaded_modules
    }

    /// Resizes the number of bindings on the global environment.
    pub(crate) fn resize_global_env(&self) {
        let binding_number = self.scope().num_bindings();
        let env = self
            .environment()
            .kind()
            .as_global()
            .expect("Realm should only store global environments");
        let mut bindings = env.bindings().borrow_mut();

        if bindings.len() < binding_number as usize {
            bindings.resize(binding_number as usize, None);
        }
    }

    pub(crate) fn push_template(&self, site: u64, template: JsObject) {
        self.inner.template_map.borrow_mut().insert(site, template);
    }

    pub(crate) fn lookup_template(&self, site: u64) -> Option<JsObject> {
        self.inner.template_map.borrow().get(&site).cloned()
    }

    /// Register a property on the global object of this realm.
    ///
    /// It will return an error if the property is already defined.
    pub fn register_property<K, V>(
        &self,
        key: K,
        value: V,
        attribute: Attribute,
        context: &mut Context,
    ) -> JsResult<()>
    where
        K: Into<PropertyKey>,
        V: Into<JsValue>,
    {
        self.global_object().define_property_or_throw(
            key,
            PropertyDescriptor::builder()
                .value(value)
                .writable(attribute.writable())
                .enumerable(attribute.enumerable())
                .configurable(attribute.configurable()),
            context,
        )?;
        Ok(())
    }

    /// Register a class `C` in this realm.
    pub fn register_class<C: Class>(&self, spec: StandardConstructor) {
        self.inner
            .host_classes
            .borrow_mut()
            .insert(TypeId::of::<C>(), spec);
    }

    /// Unregister a class `C` in this realm.
    #[must_use]
    pub fn unregister_class<C: Class>(&self) -> Option<StandardConstructor> {
        self.inner
            .host_classes
            .borrow_mut()
            .remove(&TypeId::of::<C>())
    }

    pub(crate) fn addr(&self) -> *const () {
        let ptr: *const _ = &raw const *self.inner;
        ptr.cast()
    }
}

#[cfg(test)]
mod tests {
    use crate::{Context, JsObject, JsValue, Source, js_string, property::PropertyKey};

    fn marked_object(context: &mut Context) -> JsObject {
        let object = JsObject::with_object_proto(context.intrinsics());
        object
            .set(
                PropertyKey::from(js_string!("marker")),
                JsValue::from(42),
                true,
                context,
            )
            .expect("marker");
        object
    }

    /// Within its window, `finish_global_this_initialization` reaches every way
    /// a script can name the global `this`: the `globalThis` binding, the
    /// `this` of global code, and the global environment record's this binding
    /// (which a sloppy-mode call substitutes for `undefined`).
    #[test]
    fn finished_global_this_is_what_every_lookup_resolves_to() {
        let mut context = Context::default();
        let replacement = marked_object(&mut context);
        let realm = context.realm().clone();
        realm
            .finish_global_this_initialization(replacement.clone(), &mut context)
            .expect("the window is open on a fresh context");

        let expected = JsValue::from(replacement);
        for source in [
            "globalThis",
            "this",
            "(function () { return this; })()",
            "(0, eval)('this')",
        ] {
            let observed = context
                .eval(Source::from_bytes(source))
                .expect("evaluation threw");
            assert_eq!(observed, expected, "`{source}`");
        }

        // Coherent, not merely equal: all three agree, and the global object is
        // still the one `var` writes to and the proxy would forward to.
        let coherent = context
            .eval(Source::from_bytes(
                "globalThis === this                  && (function () { return this; })() === globalThis                  && globalThis.marker === 42",
            ))
            .expect("evaluation threw");
        assert_eq!(coherent, JsValue::from(true));
    }

    /// The window closes once code has run in the realm, and closes for good
    /// after a successful call. Both refusals leave the realm untouched.
    #[test]
    fn finishing_global_this_is_refused_outside_its_window() {
        let mut context = Context::default();
        context
            .eval(Source::from_bytes("var ran = 1;"))
            .expect("evaluation threw");
        let replacement = marked_object(&mut context);
        let realm = context.realm().clone();
        let error = realm
            .finish_global_this_initialization(replacement, &mut context)
            .expect_err("a script has already run in this realm");
        assert!(
            error.to_string().contains("already fixed"),
            "unexpected error: {error}"
        );
        let unchanged = context
            .eval(Source::from_bytes(
                "globalThis === this && globalThis.marker",
            ))
            .expect("evaluation threw");
        assert_eq!(unchanged, JsValue::undefined());

        // And a second call, even with nothing having run in between.
        let mut context = Context::default();
        let first = marked_object(&mut context);
        let second = marked_object(&mut context);
        let realm = context.realm().clone();
        realm
            .finish_global_this_initialization(first.clone(), &mut context)
            .expect("the first call is the initialization");
        realm
            .finish_global_this_initialization(second, &mut context)
            .expect_err("the initialization only happens once");
        assert_eq!(
            context
                .eval(Source::from_bytes("this"))
                .expect("evaluation threw"),
            JsValue::from(first)
        );
    }
}
