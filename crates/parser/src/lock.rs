use std::{
    ops::{Deref, DerefMut},
    time::{Duration, Instant},
};

use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use tower_lsp::{lsp_types::MessageType, Client};

pub struct LoggedRwLock<T> {
    inner: RwLock<T>,
    client: Client,
}

pub struct LoggedReadGuard<'a, T> {
    guard: RwLockReadGuard<'a, T>,
    start: Instant,
    alias: String,
    client: Client,
}

impl<'a, T> Deref for LoggedReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.guard
    }
}

pub struct LoggedWriteGuard<'a, T> {
    guard: RwLockWriteGuard<'a, T>,
    start: Instant,
    alias: String,
    client: Client,
}

impl<'a, T> Deref for LoggedWriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.guard
    }
}

impl<'a, T> DerefMut for LoggedWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.guard
    }
}

impl<T> LoggedRwLock<T> {
    pub fn new(client: Client, data: T) -> Self {
        Self {
            client,
            inner: RwLock::new(data),
        }
    }
}

impl<T> LoggedRwLock<T> {
    pub async fn read<'a>(&'a self, alias: impl Into<String>) -> LoggedReadGuard<'a, T> {
        let alias = alias.into();
        let wait_start = Instant::now();
        #[cfg(feature = "log-all")]
        self.client
            .log_message(MessageType::INFO, format!("[READ][{}] Aquire", alias))
            .await;
        let guard = self.inner.read().await;
        let waited = wait_start.elapsed();

        #[cfg(feature = "log-all")]
        self.client
            .log_message(
                MessageType::INFO,
                format!("[READ][{}] Waited: {:?}", alias, waited),
            )
            .await;
        #[cfg(not(feature = "log-all"))]
        if waited > Duration::from_millis(100) {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("[READ][{}] Waited: {:?}", alias, waited),
                )
                .await;
        }

        LoggedReadGuard {
            guard,
            client: self.client.clone(),
            start: Instant::now(),
            alias,
        }
    }

    pub async fn write<'a>(&'a self, alias: impl Into<String>) -> LoggedWriteGuard<'a, T> {
        let alias = alias.into();
        let wait_start = Instant::now();
        #[cfg(feature = "log-all")]
        self.client
            .log_message(MessageType::INFO, format!("[WRITE][{}] Aquire", alias))
            .await;
        let guard = self.inner.write().await;
        let waited = wait_start.elapsed();

        #[cfg(feature = "log-all")]
        self.client
            .log_message(
                MessageType::INFO,
                format!("[WRITE][{}] Waited: {:?}", alias, waited),
            )
            .await;
        #[cfg(not(feature = "log-all"))]
        if waited > Duration::from_millis(100) {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("[WRITE][{}] Waited: {:?}", alias, waited),
                )
                .await;
        }

        LoggedWriteGuard {
            guard,
            client: self.client.clone(),
            start: Instant::now(),
            alias,
        }
    }
}

impl<'a, T> Drop for LoggedReadGuard<'a, T> {
    fn drop(&mut self) {
        #[cfg(feature = "log-all")]
        {
            let held = self.start.elapsed();
            let alias = self.alias.clone();
            let client = self.client.clone();
            tokio::spawn(async move {
                client
                    .log_message(
                        MessageType::INFO,
                        format!("[READ][{}] Held for: {:?}", alias, held),
                    )
                    .await;
            });
        }
        #[cfg(not(feature = "log-all"))]
        {
            let held = self.start.elapsed();
            if held > Duration::from_millis(100) {
                let alias = self.alias.clone();
                let client = self.client.clone();
                tokio::spawn(async move {
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("[READ][{}] Held for: {:?}", alias, held),
                        )
                        .await;
                });
            }
        }
    }
}

impl<'a, T> Drop for LoggedWriteGuard<'a, T> {
    fn drop(&mut self) {
        #[cfg(feature = "log-all")]
        {
            let held = self.start.elapsed();
            let alias = self.alias.clone();
            let client = self.client.clone();
            tokio::spawn(async move {
                client
                    .log_message(
                        MessageType::INFO,
                        format!("[WRITE][{}] Held for: {:?}", alias, held),
                    )
                    .await;
            });
        }
        #[cfg(not(feature = "log-all"))]
        {
            let held = self.start.elapsed();
            if held > Duration::from_millis(100) {
                let alias = self.alias.clone();
                let client = self.client.clone();
                tokio::spawn(async move {
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("[WRITE][{}] Held for: {:?}", alias, held),
                        )
                        .await;
                });
            }
        }
    }
}
