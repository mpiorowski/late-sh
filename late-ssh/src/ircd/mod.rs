pub mod auth;
pub mod conn;
pub mod motd;
pub mod proj;
pub mod registry;
pub mod replies;
pub mod serve;
#[cfg(test)]
mod serve_test;

#[cfg(test)]
mod proj_test;

#[cfg(test)]
mod registry_test;

#[cfg(test)]
mod replies_test;
