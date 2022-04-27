use std::sync::Arc;

use parking_lot::{Condvar, MappedMutexGuard, Mutex, MutexGuard};

struct Data<T> {
    data: T,
    locked: bool,
}

pub struct ConcurrentData<T> {
    data: Arc<Mutex<Data<T>>>,
    cond_var: Arc<Condvar>,
}

impl<T> Clone for ConcurrentData<T> {
    fn clone(&self) -> Self {
        ConcurrentData {
            data: self.data.clone(),
            cond_var: self.cond_var.clone(),
        }
    }
}

impl<T> ConcurrentData<T> {
    pub fn new(data: T) -> Self {
        ConcurrentData {
            data: Arc::new(Mutex::new(Data {
                data,
                locked: false,
            })),
            cond_var: Default::default(),
        }
    }

    pub fn locked_data(&self) -> MappedMutexGuard<T> {
        let mut guard = self.data.lock();
        while guard.locked {
            self.cond_var.wait(&mut guard)
        }
        MutexGuard::map(guard, |data| &mut data.data)
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::concurrent_data::ConcurrentData;

    #[test]
    fn test_multiple_threads_works() {
        let data = ConcurrentData::new(0i32);
        const THREADS: usize = 10;
        for i in 0..THREADS {
            let t_data = data.clone();
            std::thread::spawn(|| {
                let delta = if i & 1 == 0 { 1 } else { -1 };
                *t_data.locked_data() += delta;
            });
        }
        while *data.locked_data() != 0 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
