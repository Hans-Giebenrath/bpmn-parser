// Macro to generate all four Index implementations

macro_rules! impl_index {
    ($index_type:ty, $element_type:ty, $argument:ident) => {
        // Index for Vec
        impl std::ops::Index<$index_type> for Vec<$element_type> {
            type Output = $element_type;

            fn index(&self, $argument: $index_type) -> &Self::Output {
                &self[$argument.0]
            }
        }

        // IndexMut for Vec
        impl std::ops::IndexMut<$index_type> for Vec<$element_type> {
            fn index_mut(&mut self, $argument: $index_type) -> &mut Self::Output {
                &mut self[$argument.0]
            }
        }

        // Index for slice
        impl std::ops::Index<$index_type> for [$element_type] {
            type Output = $element_type;

            fn index(&self, $argument: $index_type) -> &Self::Output {
                &self[$argument.0]
            }
        }

        // IndexMut for slice
        impl std::ops::IndexMut<$index_type> for [$element_type] {
            fn index_mut(&mut self, $argument: $index_type) -> &mut Self::Output {
                &mut self[$argument.0]
            }
        }
    };
}

pub(crate) use impl_index;

#[allow(unused_macros)]
macro_rules! dbg_compact {
    () => {
        eprintln!("[{}:{}]", file!(), line!());
    };
    ($val:expr $(,)?) => {{
        let tmp = &$val;
        eprintln!("[{}:{}] {} = {:?}", file!(), line!(), stringify!($val), tmp);
        tmp
    }};
    ($($val:expr),+ $(,)?) => {{
        ($(dbg_compact!($val)),+,)
    }};
}

#[allow(unused_imports)]
pub(crate) use dbg_compact;
