// src/typed_attrs.rs
//! Typed attributes — zero-cost compile-time attribute identity
//!
//! v10.1: Added enum, int, and bool attribute macros for compile-time safety.

pub trait TypedAttr: 'static {
    const NAME: &'static str;
    const IS_TAG: bool = false;
    const DEFAULT: Option<f64> = None;
}

/// Trait for enum-like attributes with named variants encoded as f64.
/// Provides compile-time checked conversions.
pub trait EnumAttr: TypedAttr {
    /// All variant names in order (index = f64 value)
    const VARIANTS: &'static [&'static str];

    /// Convert f64 to variant index, returns None if out of range
    fn from_f64(v: f64) -> Option<usize> {
        let idx = v as usize;
        if (v - idx as f64).abs() < 1e-9 && idx < Self::VARIANTS.len() {
            Some(idx)
        } else {
            None
        }
    }

    /// Convert variant index to f64
    fn to_f64(idx: usize) -> f64 {
        idx as f64
    }

    /// Check if a f64 value is a valid variant
    fn is_valid(v: f64) -> bool {
        Self::from_f64(v).is_some()
    }
}

#[macro_export]
macro_rules! define_attr {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name;
        impl $crate::typed_attrs::TypedAttr for $name {
            const NAME: &'static str = stringify!($name);
        }
    };
}

#[macro_export]
macro_rules! define_tag {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name;
        impl $crate::typed_attrs::TypedAttr for $name {
            const NAME: &'static str = stringify!($name);
            const IS_TAG: bool = true;
            const DEFAULT: Option<f64> = Some(1.0);
        }
    };
}

#[macro_export]
macro_rules! define_attrs {
    ($($name:ident),* $(,)?) => { $( $crate::define_attr!($name); )* };
}

#[macro_export]
macro_rules! define_tags {
    ($($name:ident),* $(,)?) => { $( $crate::define_tag!($name); )* };
}

/// Define an enum-like attribute with named variants.
/// Each variant is encoded as its index (0.0, 1.0, 2.0, ...).
///
/// # Example
/// ```
/// define_enum_attr!(AiState => Idle, Chase, Flee, Attack);
///
/// // In resonator:
/// let state_val = ai_state.get(ctx);
/// match AiState::from_f64(state_val) {
///     Some(0) => { /* Idle */ }
///     Some(1) => { /* Chase */ }
///     _ => {}
/// }
/// // Or use the generated constants:
/// if state_val == AiState::IDLE { /* ... */ }
/// ```
#[macro_export]
macro_rules! define_enum_attr {
    ($name:ident => $($variant:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name;

        impl $crate::typed_attrs::TypedAttr for $name {
            const NAME: &'static str = stringify!($name);
        }

        impl $crate::typed_attrs::EnumAttr for $name {
            const VARIANTS: &'static [&'static str] = &[
                $( stringify!($variant) ),+
            ];
        }

        #[allow(non_upper_case_globals, dead_code)]
        impl $name {
            define_enum_attr!(@variants 0usize, $($variant),+);
        }
    };
    (@variants $idx:expr, $variant:ident $(, $rest:ident)*) => {
        pub const $variant: f64 = $idx as f64;
        define_enum_attr!(@variants ($idx + 1usize), $($rest),*);
    };
    (@variants $idx:expr,) => {};
}

/// Define an integer-semantic attribute (still stored as f64, but with
/// convenience methods for i64 conversion).
///
/// # Example
/// ```
/// define_int_attr!(Population);
/// // write: world.write_typed::<Population>(entity, 1000.0);
/// // read as int: Population::as_i64(world.read_typed::<Population>(entity).unwrap())
/// ```
#[macro_export]
macro_rules! define_int_attr {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name;
        impl $crate::typed_attrs::TypedAttr for $name {
            const NAME: &'static str = stringify!($name);
        }
        #[allow(dead_code)]
        impl $name {
            #[inline(always)]
            pub fn as_i64(v: f64) -> i64 { v as i64 }
            #[inline(always)]
            pub fn from_i64(v: i64) -> f64 { v as f64 }
        }
    };
}

/// Define a boolean-semantic attribute (stored as f64: 0.0 = false, 1.0 = true).
///
/// # Example
/// ```
/// define_bool_attr!(IsAlive);
/// // In resonator:
/// if IsAlive::is_true(is_alive.get(ctx)) { /* ... */ }
/// ```
#[macro_export]
macro_rules! define_bool_attr {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name;
        impl $crate::typed_attrs::TypedAttr for $name {
            const NAME: &'static str = stringify!($name);
        }
        #[allow(dead_code)]
        impl $name {
            pub const TRUE: f64 = 1.0;
            pub const FALSE: f64 = 0.0;
            #[inline(always)]
            pub fn is_true(v: f64) -> bool { v >= 0.5 }
            #[inline(always)]
            pub fn is_false(v: f64) -> bool { v < 0.5 }
            #[inline(always)]
            pub fn from_bool(b: bool) -> f64 { if b { 1.0 } else { 0.0 } }
        }
    };
}