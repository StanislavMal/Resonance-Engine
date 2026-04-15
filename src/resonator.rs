// src/resonator.rs
//! Resonator system with struct-based design for hot reload

use crate::context::NodeContext;
use crate::storage::FieldIndex;
use crate::typed_attrs::TypedAttr;
use std::marker::PhantomData;

/// Core resonator trait
pub trait Resonator: Send + Sync + 'static {
    fn apply(&self, ctx: &mut NodeContext);
}

/// Factory trait for creating resonators from field maps
pub trait ResonatorFactory: Resonator + Sized {
    fn create(map: &FieldMap) -> Self;
}

pub type DynResonator = dyn Resonator;

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

    #[inline(always)]
    pub fn set(&self, ctx: &mut NodeContext, value: f64) {
        ctx.set(self.index, value);
    }

    #[inline(always)]
    pub fn set_unchecked(&self, ctx: &mut NodeContext, value: f64) {
        ctx.set_unchecked(self.index, value);
    }

    #[inline(always)]
    pub fn modify(&self, ctx: &mut NodeContext, f: impl FnOnce(f64) -> f64) {
        ctx.modify(self.index, f);
    }

    #[inline(always)]
    pub fn add(&self, ctx: &mut NodeContext, amount: f64) {
        ctx.add(self.index, amount);
    }

    #[inline(always)]
    pub fn add_unchecked(&self, ctx: &mut NodeContext, amount: f64) {
        let old = ctx.get(self.index);
        ctx.set_unchecked(self.index, old + amount);
    }

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

    pub fn try_bind<A: TypedAttr>(&self) -> Option<BoundField<A>> {
        let id = self.interner.find(A::NAME)?;
        self.fields.get(&id).map(|&idx| BoundField::new(idx))
    }

    pub fn field_index(&self, name: &str) -> Option<FieldIndex> {
        let id = self.interner.find(name)?;
        self.fields.get(&id).copied()
    }
}

/// Macro for binding multiple fields
#[macro_export]
macro_rules! bind_fields {
    ($map:expr, $($name:ident : $Type:ty),+ $(,)?) => {
        $( let $name = $map.bind::<$Type>(); )+
    };
}

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

pub trait ResonatorExt: Resonator + Sized {
    fn chain<R: Resonator>(self, next: R) -> Chain<Self, R> {
        Chain { a: self, b: next }
    }
}

impl<T: Resonator + Sized> ResonatorExt for T {}

// ─── Helper macro for quick struct-based resonators ───

#[macro_export]
macro_rules! define_resonator {
    (
        $name:ident {
            $( $field:ident : $Type:ty ),* $(,)?
        }
        |$this:ident, $ctx:ident| $body:expr
    ) => {
        pub struct $name {
            $( pub $field: $crate::resonator::BoundField<$Type>, )*
        }

        impl $crate::resonator::Resonator for $name {
            fn apply(&self, $ctx: &mut $crate::context::NodeContext) {
                let $this = self;
                $body
            }
        }

        impl $crate::resonator::ResonatorFactory for $name {
            fn create(map: &$crate::resonator::FieldMap) -> Self {
                Self {
                    $( $field: map.bind::<$Type>(), )*
                }
            }
        }
    };
}