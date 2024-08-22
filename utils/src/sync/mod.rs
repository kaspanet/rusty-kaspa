pub mod rwlock;
pub(crate) mod semaphore;

#[cfg(feature = "semaphore-trace")]
pub fn semaphore_module_path() -> &'static str {
    semaphore::get_module_path()
}
