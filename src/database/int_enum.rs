macro_rules! int_enum_from_sql {
    ($t:ty) => {
        impl<'a> postgres_types::FromSql<'a> for $t {
            fn from_sql(
                _: &postgres_types::Type,
                raw: &'a [u8],
            ) -> Result<$t, Box<dyn std::error::Error + Sync + Send>> {
                let int_value = postgres_protocol::types::int2_from_sql(raw)?;
                let value = <$t>::try_from(int_value)?;
                Ok(value)
            }

            postgres_types::accepts!(INT2);
        }
    }
}

macro_rules! int_enum_to_sql {
    ($t:ty) => {
        impl postgres_types::ToSql for $t {
            fn to_sql(
                &self, _: &postgres_types::Type,
                out: &mut postgres_types::private::BytesMut,
            ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
                let int_value: i16 = self.into();
                postgres_protocol::types::int2_to_sql(int_value, out);
                Ok(postgres_types::IsNull::No)
            }

            postgres_types::accepts!(INT2);
            postgres_types::to_sql_checked!();
        }
    }
}

pub(crate) use {int_enum_from_sql, int_enum_to_sql};
