//! Dedicated tokio runtime for database operations.
//!
//! This module provides a separate tokio runtime for sea-orm database operations.
//! This is required because:
//!
//! 1. sea-orm with `runtime-tokio-rustls` creates tokio-based futures
//! 2. We cannot call `block_on` on the same runtime from within itself
//! 3. `KVStoreSync` trait requires synchronous methods (called from LDK threads)
//! 4. `RlnDatabase` methods are sync and may be called from various contexts

use std::sync::LazyLock;

/// Dedicated tokio runtime for database operations.
///
/// Uses a new_current_thread runtime with 1 worker threads, which is sufficient
/// for SQLite operations, which use a single connection anyway
static DB_RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_current_thread()
        .thread_name("db-runtime")
        .enable_all()
        .build()
        .expect("Failed to create database tokio runtime")
});

/// Block on a future using the dedicated database runtime.
///
/// # Safety regarding `worker_threads = 1` tests
///
/// This is safe because:
/// - `DB_RUNTIME` has its own thread pool (2 threads)
/// - The caller's thread blocks waiting for `DB_RUNTIME`'s threads to complete
/// - Database operations don't depend on the main tokio runtime
/// - No circular dependency = no deadlock
///
/// # Handling calls from async contexts
///
/// If called from within a tokio runtime context, this function spawns a
/// new thread (using `std::thread::scope` to allow borrowing) and calls
/// `block_on` from that thread, which has no tokio context.
///
/// # Usage
///
/// Use this function for all synchronous database operations:
/// - `KVStoreSync` trait implementations (called from LDK threads)
/// - `RlnDatabase` methods (called from various sync contexts)
///
/// For async contexts (like `start_daemon`), prefer using `.await` directly.
pub(crate) fn block_on<F>(future: F) -> F::Output
where
    F: std::future::Future + Send,
    F::Output: Send,
{
    // Check if we're inside a tokio runtime context
    if tokio::runtime::Handle::try_current().is_ok() {
        // We're inside a runtime - spawn a new thread without tokio context
        // Using std::thread::scope allows borrowing from the parent scope
        std::thread::scope(|s| {
            s.spawn(|| DB_RUNTIME.block_on(future))
                .join()
                .expect("DB thread panicked")
        })
    } else {
        // Not inside a runtime - safe to call block_on directly
        DB_RUNTIME.block_on(future)
    }
}
