use async_graphql::{ErrorExtensions, FieldError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("error")]
    E(String),

    #[error("db error, not found")]
    DBNotFound(sqlx::Error),

    #[error("db error")]
    DB(sqlx::Error),

    #[error("unauthorized")]
    Unauthorized(String),

    #[allow(unused)]
    #[error("forbidden")]
    Forbidden(String),

    #[error("bad request")]
    BadRequest(String),

    #[error("hex error")]
    Hex(#[from] hex::FromHexError),
}
impl AppError {
    pub fn is_db_not_found(&self) -> bool {
        matches!(*self, Self::DBNotFound(_))
    }
}
impl From<&str> for AppError {
    fn from(s: &str) -> AppError {
        AppError::E(s.to_string())
    }
}
impl From<String> for AppError {
    fn from(s: String) -> AppError {
        AppError::E(s)
    }
}
impl From<sqlx::Error> for AppError {
    fn from(s: sqlx::Error) -> AppError {
        match s {
            sqlx::Error::RowNotFound => AppError::DBNotFound(s),
            _ => AppError::DB(s),
        }
    }
}
impl ErrorExtensions for AppError {
    fn extend(&self) -> FieldError {
        self.extend_with(|err, e| match err {
            AppError::E(s) => {
                e.set("code", "500");
                e.set("error", s.clone());
            }
            AppError::DB(_) => e.set("code", 500),
            AppError::DBNotFound(_) => e.set("code", 404),
            AppError::Unauthorized(s) => {
                e.set("code", 401);
                e.set("error", s.clone());
            }
            AppError::Forbidden(s) => {
                e.set("code", 403);
                e.set("error", s.clone());
            }
            AppError::BadRequest(s) => {
                e.set("code", 400);
                e.set("error", s.clone());
            }
            AppError::Hex(_) => e.set("code", 500),
        })
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
