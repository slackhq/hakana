#[macro_export]
macro_rules! interned_strings {
    ($($name:ident, $id:expr => $value:expr),* $(,)?) => {
        impl StrId {
            $(
                pub const $name: StrId = StrId($id);
            )*
        }

        impl Default for Interner {
            fn default() -> Self {
                let mut interner = Interner {
                    map: IndexSet::default(),
                };

                $(
                    interner.intern($value.to_string());
                )*

                interner
            }
        }
    };
}
