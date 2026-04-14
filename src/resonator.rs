// src/resonator.rs
//! Resonator trait + typed field bindings
//!
//! BoundField<A> — zero-cost typed accessor, resolved at build time.

use crate::context::NodeContext;
use crate::storage::FieldIndex;
use crate::typed_attrs::TypedAttr;
use std::marker::PhantomData;
use std::sync::Arc;

/// Core resonator trait — implement for custom resonators with state.
pub trait Resonator: Send + Sync + 'static {
    fn apply(&self, ctx: &mut NodeContext);
}

/// Blanket impl for closures: `Fn(&mut NodeContext)` is a Resonator.
impl<F> Resonator for F
where
    F: Fn(&mut NodeContext) + Send + Sync + 'static,
{
    #[inline(always)]
    fn apply(&self, ctx: &mut NodeContext) {
        self(ctx);
    }
}

/// Type-erased resonator for storage in Archetype
pub type DynResonator = dyn Resonator;

/// Typed field binding — resolved at entity build time, zero-cost at runtime.
///
/// `BoundField<Health>` compiles down to a single `FieldIndex` (2 bytes).
/// Every `get`/`set` is one pointer-arithmetic operation after inlining.
#[derive(Clone, Copy)]
pub struct BoundField<A: TypedAttr> {
    pub index: FieldIndex,
    _marker: PhantomData<A>,
}

impl<A: TypedAttr> BoundField<A> {
    pub fn new(index: FieldIndex) -> Self {
        Self {
            index,
            _marker: PhantomData,
        }
    }

    #[inline(always)]
    pub fn get(&self, ctx: &NodeContext) -> f64 {
        ctx.get(self.index)
    }

    /// Write with epsilon check — no-op if value unchanged
    #[inline(always)]
    pub fn set(&self, ctx: &mut NodeContext, value: f64) {
        ctx.set(self.index, value);
    }

    /// Write without epsilon check — always marks dirty
    #[inline(always)]
    pub fn set_unchecked(&self, ctx: &mut NodeContext, value: f64) {
        ctx.set_unchecked(self.index, value);
    }

    /// Modify via closure
    #[inline(always)]
    pub fn modify(&self, ctx: &mut NodeContext, f: impl FnOnce(f64) -> f64) {
        ctx.modify(self.index, f);
    }

    /// Add with epsilon check
    #[inline(always)]
    pub fn add(&self, ctx: &mut NodeContext, amount: f64) {
        ctx.add(self.index, amount);
    }

    /// Add without epsilon check
    #[inline(always)]
    pub fn add_unchecked(&self, ctx: &mut NodeContext, amount: f64) {
        let old = ctx.get(self.index);
        ctx.set_unchecked(self.index, old + amount);
    }

    /// Clamp to range
    #[inline(always)]
    pub fn clamp(&self, ctx: &mut NodeContext, min: f64, max: f64) {
        let v = ctx.get(self.index).clamp(min, max);
        ctx.set(self.index, v);
    }
}

impl<A: TypedAttr> std::fmt::Debug for BoundField<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BoundField<{}>({:?})", A::NAME, self.index)
    }
}

/// FieldMap — given to resonator factory during build.
///
/// Maps attribute names to field indices within the archetype schema.
/// Used to create `BoundField`s during resonator construction.
pub struct FieldMap {
    fields: std::collections::HashMap<crate::interning::InternedStr, FieldIndex>,
    interner: crate::interning::StringInterner,
}

impl FieldMap {
    pub fn new(
        fields: std::collections::HashMap<crate::interning::InternedStr, FieldIndex>,
        interner: crate::interning::StringInterner,
    ) -> Self {
        Self { fields, interner }
    }

    /// Bind typed attribute — panics if not in schema.
    pub fn bind<A: TypedAttr>(&self) -> BoundField<A> {
        let id = self
            .interner
            .find(A::NAME)
            .unwrap_or_else(|| panic!("Attribute '{}' not interned", A::NAME));
        let idx = self
            .fields
            .get(&id)
            .unwrap_or_else(|| panic!("Attribute '{}' not in archetype schema", A::NAME));
        BoundField::new(*idx)
    }

    /// Try to bind — returns None if not in schema.
    pub fn try_bind<A: TypedAttr>(&self) -> Option<BoundField<A>> {
        let id = self.interner.find(A::NAME)?;
        self.fields.get(&id).map(|&idx| BoundField::new(idx))
    }

    /// Get field index by string name (for dynamic access).
    pub fn field_index(&self, name: &str) -> Option<FieldIndex> {
        let id = self.interner.find(name)?;
        self.fields.get(&id).copied()
    }
}

/// Convenience macro for binding multiple fields at once.
#[macro_export]
macro_rules! bind_fields {
    ($map:expr, $($name:ident : $Type:ty),+ $(,)?) => {
        $( let $name = $map.bind::<$Type>(); )+
    };
}

/// Macro that creates a resonator closure with automatic `&FieldMap` type annotation.
///
/// Creates a `move` closure to properly capture variables from the surrounding scope.
///
/// # Example
/// ```ignore
/// .on(resonator!(map => {
///     bind_fields!(map, px: PosX, vx: VelX);
///     move |ctx: &mut NodeContext| {
///         px.set_unchecked(ctx, px.get(ctx) + vx.get(ctx));
///     }
/// }))
/// ```
#[macro_export]
macro_rules! resonator {
    ($map:ident => $body:expr) => {
        move |$map: &$crate::resonator::FieldMap| {
            $body
        }
    };
}

/// Resonator composition: chain two resonators sequentially
pub struct Chain<A, B> {
    a: A,
    b: B,
}

impl<A: Resonator, B: Resonator> Resonator for Chain<A, B> {
    #[inline(always)]
    fn apply(&self, ctx: &mut NodeContext) {
        self.a.apply(ctx);
        self.b.apply(ctx);
    }
}

/// Extension trait for chaining resonators
pub trait ResonatorExt: Resonator + Sized {
    fn chain<R: Resonator>(self, next: R) -> Chain<Self, R> {
        Chain { a: self, b: next }
    }
}

impl<T: Resonator + Sized> ResonatorExt for T {}