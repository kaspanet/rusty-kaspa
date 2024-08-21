pub mod rwlock;
pub(crate) mod semaphore;

pub fn semaphore_module_path() -> &'static str {
    semaphore::get_module_path()
}
