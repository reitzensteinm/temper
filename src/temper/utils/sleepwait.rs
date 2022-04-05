use std::sync::{Condvar, Mutex};

#[derive(Default)]
#[allow(clippy::mutex_atomic)]
pub struct SleepWait {
    ready: Mutex<bool>,
    signal: Condvar,
}

#[allow(clippy::mutex_atomic)]
impl SleepWait {
    pub fn signal(&self) {
        *self.ready.lock().unwrap() = true;
        self.signal.notify_all();
    }
    pub fn wait(&self) {
        let mut ready = self.ready.lock().unwrap();

        while !*ready {
            ready = self.signal.wait(ready).unwrap();
        }
    }
}

#[cfg(test)]
mod test {
    use crate::temper::utils::sleepwait::SleepWait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    #[test]
    pub fn test_wait() {
        let sw = Arc::new(SleepWait::default());
        let prog = Arc::new(AtomicUsize::new(0));

        {
            let sw = sw.clone();
            let prog = prog.clone();
            thread::spawn(move || {
                prog.store(1, Ordering::SeqCst);
                sw.wait();
                prog.store(2, Ordering::SeqCst);
                sw.wait();
            });
        };

        // These sleeps are lazy, and could cause flake.
        // The irony of building a system to test concurrency bugs and writing code like this
        // is not lost on me.
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(prog.load(Ordering::SeqCst), 1);
        sw.signal();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(prog.load(Ordering::SeqCst), 2);
        sw.signal();
    }
}
