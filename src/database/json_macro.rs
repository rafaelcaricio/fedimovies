/// Implements FromSql trait for any de-serializable type
macro_rules! json_from_sql {
    ($t:ty) => {
        impl<'a> postgres_types::FromSql<'a> for $t {
            fn from_sql(
                ty: &postgres_types::Type,
                raw: &'a [u8],
            ) -> Result<$t, Box<dyn std::error::Error + Sync + Send>> {
                let postgres_types::Json(json_value) =
                    postgres_types::Json::<serde_json::Value>::from_sql(ty, raw)?;
                let value: $t = serde_json::from_value(json_value)?;
                Ok(value)
            }

            postgres_types::accepts!(JSON, JSONB);
        }
    }
}

/// Implements ToSql trait for any serializable type
macro_rules! json_to_sql {
    ($t:ty) => {
        impl postgres_types::ToSql for $t {
            fn to_sql(
                &self,
                ty: &postgres_types::Type,
                out: &mut postgres_types::private::BytesMut,
            ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
                let value = serde_json::to_value(self)?;
                postgres_types::Json(value).to_sql(ty, out)
            }

            postgres_types::accepts!(JSON, JSONB);
            postgres_types::to_sql_checked!();
        }
    }
}

pub(crate) use {json_from_sql, json_to_sql};
