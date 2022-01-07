macro_rules! query {
    ($($tt:tt)*) => {
        postgres_query_macro::proc_macro_hack_query_dynamic!($($tt)*)
    };
}

pub(crate) use query;
