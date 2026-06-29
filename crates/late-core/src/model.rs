#[macro_export]
macro_rules! model {
    (
          table = $table:expr;
          params = $params_name:ident;
          struct $name:ident {
              @generated
              $(pub $gen_field:ident : $gen_type:ty),* $(,)?;
              @data
              $(pub $data_field:ident : $data_type:ty),* $(,)?
          }
      ) => {
          #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
          pub struct $name {
              pub id: uuid::Uuid,
              pub created: chrono::DateTime<chrono::Utc>,
              pub updated: chrono::DateTime<chrono::Utc>,
              $(pub $gen_field : $gen_type,)*
              $(pub $data_field : $data_type,)*
          }

          #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
          pub struct $params_name {
              $(pub $data_field : $data_type,)*
          }

          impl From<tokio_postgres::Row> for $name {
              fn from(row: tokio_postgres::Row) -> Self {
                  Self {
                      id: row.get("id"),
                      created: row.get("created"),
                      updated: row.get("updated"),
                      $($gen_field: row.get(stringify!($gen_field)),)*
                      $($data_field: row.get(stringify!($data_field)),)*
                  }
              }
          }

          impl $name {
              pub const TABLE: &'static str = $table;

              pub async fn all(client: &tokio_postgres::Client) -> anyhow::Result<Vec<Self>> {
                  let query = format!("SELECT * FROM {} ORDER BY created DESC", Self::TABLE);
                  let rows = client.query(&query, &[]).await?;
                  Ok(rows.into_iter().map(Self::from).collect())
              }

              pub async fn get(
                  client: &tokio_postgres::Client,
                  id: uuid::Uuid,
              ) -> anyhow::Result<Option<Self>> {
                  let query = format!("SELECT * FROM {} WHERE id = $1", Self::TABLE);
                  let row = client.query_opt(&query, &[&id]).await?;
                  Ok(row.map(Self::from))
              }

              pub async fn create(
                  client: &tokio_postgres::Client,
                  params: $params_name,
              ) -> anyhow::Result<Self> {
                  let columns = [$(stringify!($data_field)),*];
                  let columns_list = columns.join(", ");
                  let placeholders = (1..=columns.len())
                      .map(|i| format!("${}", i))
                      .collect::<Vec<_>>()
                      .join(", ");
                  let query = format!(
                      "INSERT INTO {} ({}) VALUES ({}) RETURNING *",
                      Self::TABLE,
                      columns_list,
                      placeholders
                  );
                  let row = client.query_one(&query, &[
                      $(&params.$data_field),*
                  ]).await?;
                  Ok(Self::from(row))
              }

              pub async fn update(
                  client: &tokio_postgres::Client,
                  id: uuid::Uuid,
                  params: $params_name,
              ) -> anyhow::Result<Self> {
                  let columns = [$(stringify!($data_field)),*];
                  let set_list = columns
                      .iter()
                      .enumerate()
                      .map(|(i, column)| format!("{} = ${}", column, i + 1))
                      .collect::<Vec<_>>()
                      .join(", ");
                  let query = format!(
                      "UPDATE {} SET {}, updated = current_timestamp WHERE id = ${} RETURNING *",
                      Self::TABLE,
                      set_list,
                      columns.len() + 1
                  );
                  let row = client.query_one(&query, &[
                      $(&params.$data_field),*,
                      &id
                  ]).await?;
                  Ok(Self::from(row))
              }

              pub async fn delete(
                  client: &tokio_postgres::Client,
                  id: uuid::Uuid,
              ) -> anyhow::Result<u64> {
                  let query = format!("DELETE FROM {} WHERE id = $1", Self::TABLE);
                  let count = client.execute(&query, &[&id]).await?;
                  Ok(count)
              }
          }
    };

    (
          table = $table:expr;
          params = $params_name:ident;
          struct $name:ident {
              @data
              $(pub $data_field:ident : $data_type:ty),* $(,)?
          }
      ) => {
          $crate::model! {
              table = $table;
              params = $params_name;
              struct $name {
                  @generated;
                  @data
                  $(pub $data_field : $data_type),*
              }
          }
    };
}

#[macro_export]
macro_rules! user_scoped_model {
    (
          table = $table:expr;
          user_field = $user_field:ident;
          params = $params_name:ident;
          struct $name:ident {
              @generated
              $(pub $gen_field:ident : $gen_type:ty),* $(,)?;
              @data
              $(pub $data_field:ident : $data_type:ty),* $(,)?
          }
      ) => {
          $crate::model! {
              table = $table;
              params = $params_name;
              struct $name {
                  @generated
                  $(pub $gen_field : $gen_type),*;
                  @data
                  $(pub $data_field : $data_type),*
              }
          }

          impl $name {
              pub async fn find_by_user_id(
                  client: &tokio_postgres::Client,
                  user_id: uuid::Uuid,
              ) -> anyhow::Result<Option<Self>> {
                  let query = format!(
                      "SELECT * FROM {} WHERE {} = $1",
                      Self::TABLE,
                      stringify!($user_field)
                  );
                  let row = client.query_opt(&query, &[&user_id]).await?;
                  Ok(row.map(Self::from))
              }

              pub async fn list_by_user_id(
                  client: &tokio_postgres::Client,
                  user_id: uuid::Uuid,
              ) -> anyhow::Result<Vec<Self>> {
                  let query = format!(
                      "SELECT * FROM {} WHERE {} = $1 ORDER BY created DESC",
                      Self::TABLE,
                      stringify!($user_field)
                  );
                  let rows = client.query(&query, &[&user_id]).await?;
                  Ok(rows.into_iter().map(Self::from).collect())
              }

              pub async fn get_by_user_id(
                  client: &tokio_postgres::Client,
                  user_id: uuid::Uuid,
                  id: uuid::Uuid,
              ) -> anyhow::Result<Option<Self>> {
                  let query = format!(
                      "SELECT * FROM {} WHERE {} = $1 AND id = $2",
                      Self::TABLE,
                      stringify!($user_field)
                  );
                  let row = client.query_opt(&query, &[&user_id, &id]).await?;
                  Ok(row.map(Self::from))
              }

              pub async fn create_by_user_id(
                  client: &tokio_postgres::Client,
                  user_id: uuid::Uuid,
                  mut params: $params_name,
              ) -> anyhow::Result<Self> {
                  params.$user_field = user_id;
                  Self::create(client, params).await
              }

              pub async fn update_by_user_id(
                  client: &tokio_postgres::Client,
                  user_id: uuid::Uuid,
                  id: uuid::Uuid,
                  mut params: $params_name,
              ) -> anyhow::Result<Option<Self>> {
                  params.$user_field = user_id;
                  let columns = [$(stringify!($data_field)),*];
                  let set_list = columns
                      .iter()
                      .enumerate()
                      .map(|(i, column)| format!("{} = ${}", column, i + 1))
                      .collect::<Vec<_>>()
                      .join(", ");
                  let query = format!(
                      "UPDATE {} SET {}, updated = current_timestamp WHERE {} = ${} AND id = ${} RETURNING *",
                      Self::TABLE,
                      set_list,
                      stringify!($user_field),
                      columns.len() + 1,
                      columns.len() + 2
                  );
                  let row = client
                      .query_opt(&query, &[
                          $(&params.$data_field),*,
                          &user_id,
                          &id
                      ])
                      .await?;
                  Ok(row.map(Self::from))
              }

              pub async fn delete_by_user_id(
                  client: &tokio_postgres::Client,
                  user_id: uuid::Uuid,
                  id: uuid::Uuid,
              ) -> anyhow::Result<u64> {
                  let query = format!(
                      "DELETE FROM {} WHERE {} = $1 AND id = $2",
                      Self::TABLE,
                      stringify!($user_field)
                  );
                  let count = client.execute(&query, &[&user_id, &id]).await?;
                  Ok(count)
              }
          }
    };

    (
          table = $table:expr;
          user_field = $user_field:ident;
          params = $params_name:ident;
          struct $name:ident {
              @data
              $(pub $data_field:ident : $data_type:ty),* $(,)?
          }
      ) => {
          $crate::user_scoped_model! {
              table = $table;
              user_field = $user_field;
              params = $params_name;
              struct $name {
                  @generated;
                  @data
                  $(pub $data_field : $data_type),*
              }
          }
    };
}
